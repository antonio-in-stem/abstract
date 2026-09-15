const { test } = require("node:test");
const assert = require("node:assert/strict");
const path = require("path");
const fs = require("fs/promises");
const protocol = require("../src/analysis-client");
const { temporaryWorkspace } = require("./test-workspace");

const compiler = process.env.ABSTRACT_COMPILER_PATH || path.resolve(__dirname, "../../target/debug", process.platform === "win32" ? "abstract.exe" : "abstract");
async function analyze(root, overlays, id = 7) {
  return protocol.startProcess(compiler, ["analyze", root, "--stdio"], { input: protocol.encodeRequest(id, overlays), cwd: root }).promise;
}

test("real compiler analyzes unsaved instance/schema together and keeps relative assets at the actual root", async () => {
  const workspace = await temporaryWorkspace();
  try {
    const schemaText = "schema Item {\nname: text\nicon: file(txt)\n}\n";
    const instanceText = "Item :: @id.x\nname: Disk\nicon: ./note.txt\n";
    const schema = await workspace.write("data/schema.abt", schemaText);
    const file = await workspace.write("data/item.ab", instanceText);
    await workspace.write("assets/note.txt", "real asset");
    const overlays = [{ path: schema, text: schemaText.replace("name: text", "name: int") }, { path: file, text: instanceText.replace("name: Disk", "name: 42") }];
    const valid = protocol.response(await analyze(workspace.root, overlays), 7);
    assert.equal(valid.analyzed, true);
    assert.deepEqual(valid.diagnostics, []);
    overlays[1].text = overlays[1].text.replace("./note.txt", "./missing.txt");
    const invalid = protocol.response(await analyze(workspace.root, overlays), 7);
    assert.ok(invalid.diagnostics.some((d) => d.code === "E421"));
    assert.equal(invalid.diagnostics[0].path.replace(/\\/g, "/").toLowerCase(), file.replace(/\\/g, "/").toLowerCase());
    assert.equal(await fs.readFile(schema, "utf8"), schemaText);
    assert.equal(await fs.readFile(file, "utf8"), instanceText);
    assert.equal(await fs.readFile(path.join(workspace.root, "assets/note.txt"), "utf8"), "real asset");
  } finally { await workspace.dispose(); }
});

test("overlay replacement happens before disk UTF-8 decoding and permits an unsaved new source", async () => {
  const workspace = await temporaryWorkspace();
  try {
    const schema = await workspace.write("data/schema.abt", "schema Item {\nname: text\n}\n");
    await fs.writeFile(schema, Buffer.from([0xFF]));
    const file = path.join(workspace.root, "data/new.ab");
    const result = protocol.response(await analyze(workspace.root, [
      { path: schema, text: "schema Item {\nname: text\n}\n" }, { path: file, text: "Item :: @id.new\nname: New\n" }
    ]), 7);
    assert.deepEqual(result.diagnostics, []);
    assert.equal(result.analyzed, true);
    await assert.rejects(fs.stat(file), { code: "ENOENT" });
    assert.deepEqual(await fs.readFile(schema), Buffer.from([0xFF]));
  } finally { await workspace.dispose(); }
});

test("compiler ranges distinguish disk BOM metadata from exact overlay text and count supplementary UTF-16 units", async () => {
  const workspace = await temporaryWorkspace();
  try {
    const text = "\uFEFFschema Unicode { 😀: text }\r\n";
    const file = await workspace.write("data/schema.abt", text);
    for (const overlays of [[], [{ path: file, text }]]) {
      const diagnostic = protocol.response(await analyze(workspace.root, overlays), 7).diagnostics.find((d) => d.code === "E210");
      assert.ok(diagnostic);
      const start = text.indexOf("😀") - (overlays.length ? 0 : 1);
      assert.deepEqual(diagnostic.range, { start: { line: 0, character: start }, end: { line: 0, character: start + 2 } });
    }
  } finally { await workspace.dispose(); }
});

test("foreign, ignored, and duplicate canonical overlays are refused without changing disk", async () => {
  const workspace = await temporaryWorkspace();
  try {
    await workspace.write("one/data/schema.abt", "schema Item {\n}\n");
    const foreign = await workspace.write("two/data/schema.abt", "schema Other {\n}\n");
    const ignored = await workspace.write("one/data/target/ignored.ab", "unrelated");
    const root = path.join(workspace.root, "one");
    for (const overlay of [{ path: foreign, text: "" }, { path: ignored, text: "" }]) {
      const result = await analyze(root, [overlay]);
      assert.equal(result.error?.code, 2);
      assert.match(result.stderr, /does not belong/);
    }
    const duplicate = await analyze(root, [{ path: foreign, text: "one" }, { path: foreign, text: "two" }]);
    assert.equal(duplicate.error?.code, 2);
    assert.match(duplicate.stderr, /Duplicate canonical/);
    assert.equal(await fs.readFile(ignored, "utf8"), "unrelated");
  } finally { await workspace.dispose(); }
});

test("discovered junction aliases keep canonical diagnostic targets and allow their overlays", async () => {
  const workspace = await temporaryWorkspace();
  try {
    await workspace.write("one/data/own.abt", "schema Own {\n}\n");
    const linked = await workspace.write("shared/source.abt", "schema Shared {\n}\n");
    await fs.symlink(path.join(workspace.root, "shared"), path.join(workspace.root, "one/data/shared"), process.platform === "win32" ? "junction" : "dir");
    const result = protocol.response(await analyze(path.join(workspace.root, "one"), [{ path: linked, text: "schema Shared {\nfield: made_up\n}\n" }]), 7);
    const diagnostic = result.diagnostics.find((d) => d.code === "E304");
    assert.ok(diagnostic);
    assert.equal(path.normalize(diagnostic.path).toLowerCase(), path.normalize(linked).toLowerCase());
    assert.equal(diagnostic.displayPath, "data/shared/source.abt");
  } finally { await workspace.dispose(); }
});

test("compiler rejects malformed wire payloads and declares bounded diagnostic truncation", async () => {
  const workspace = await temporaryWorkspace();
  try {
    await workspace.write("data/schema.abt", "schema Item {\n}\n");
    const file = await workspace.write("data/item.ab", "Item :: @id.x\n");
    const valid = protocol.encodeRequest(7, [{ path: file, text: "a" }]);
    for (const input of [valid.subarray(0, valid.length - 1), Buffer.concat([valid, Buffer.from([0])]), Buffer.concat([valid.subarray(0, valid.length - 1), Buffer.from([0xFF])])]) {
      const result = await protocol.startProcess(compiler, ["analyze", workspace.root, "--stdio"], { input }).promise;
      assert.equal(result.error?.code, 2);
      assert.equal(result.stdout, "");
    }
    const result = protocol.response(await analyze(workspace.root, [{ path: file, text: `${"A".repeat(20000)} :: @id.x\n` }]), 7);
    assert.equal(result.truncated, true);
    assert.ok(Buffer.byteLength(result.diagnostics[0].message) <= 16384);
  } finally { await workspace.dispose(); }
});

test("client rejects lossy Unicode and bounded-input violations before starting a process", () => {
  assert.throws(() => protocol.encodeRequest(1, [{ path: path.resolve("x.ab"), text: "\uD800" }]), /surrogate/);
  assert.throws(() => protocol.encodeRequest(1, Array(129).fill({ path: path.resolve("x.ab"), text: "" })), /limits/);
  assert.throws(() => protocol.encodeRequest(1, [{ path: path.resolve("x.ab"), text: "x".repeat(protocol.LIMITS.text + 1) }]), /limits/);
});

test("process cancellation terminates a running process and marks its response unusable", async () => {
  const operation = protocol.startProcess(process.execPath, ["-e", "setTimeout(() => process.stdout.write('late'), 5000)"]);
  const started = Date.now();
  await new Promise((resolve) => setTimeout(resolve, 50));
  operation.cancel();
  const result = await operation.promise;
  assert.equal(result.cancelled, true);
  assert.equal(result.stdout, "");
  assert.ok(Date.now() - started < 2500);
});

test("capability negotiation distinguishes old compiler support from broken executables", async () => {
  const actual = await protocol.startProcess(compiler, ["analyze", "--capabilities"]).promise;
  assert.equal(protocol.capability(actual).supported, true);
  assert.equal(protocol.capability(actual).values, true);
  assert.equal(protocol.capability({ error: { code: 2 }, stderr: "abstract: error[E801]: Unknown command 'analyze'; expected compile, lint, templates or init.", stdout: "" }).supported, false);
  assert.throws(() => protocol.capability({ error: { code: "ENOENT", message: "missing" }, stderr: "", stdout: "" }), /missing/);
});

test("responses with mismatched request ids or malformed ranges never reach diagnostics", () => {
  const base = { protocol: "abstract-analysis", version: 1, requestId: 5, positionEncoding: "utf-16", analyzed: true, truncated: false, diagnostics: [] };
  assert.throws(() => protocol.response({ stdout: JSON.stringify(base) }, 6), /stale/);
  base.diagnostics.push({ code: "E210", severity: "error", message: "x", notes: [], range: { start: { line: 0, character: 4 }, end: { line: 0, character: 1 } } });
  assert.throws(() => protocol.response({ stdout: JSON.stringify(base) }, 5), /malformed/);
});
