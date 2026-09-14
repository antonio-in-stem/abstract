const { test } = require("node:test");
const assert = require("node:assert/strict");
const path = require("path");
const fs = require("fs/promises");
const protocol = require("../src/analysis-client");
const model = require("../src/schema-bindings");
const { temporaryWorkspace } = require("./test-workspace");

const compiler = process.env.ABSTRACT_COMPILER_PATH || path.resolve(__dirname, "../../../target/debug", process.platform === "win32" ? "abstract.exe" : "abstract");
const schema = "schema Product {\nvalue: int\nowner: $(Owner) @optional\npeer: ref(Owner) @optional\nnote: text = Product\n}\nlogic Product {\nrequire .value >= 0 else throw \"Product\"\n}\nschema product {\n}\n// Product is a comment, not a reference.\n";
const other = "schema Owner {\nlabel: text\n}\n";
const instance = "Product :: @id.x\nvalue: 2\n";
async function fixture() {
  const workspace = await temporaryWorkspace();
  const a = await workspace.write("data/model.abt", schema);
  const b = await workspace.write("data/owner.abt", other);
  const c = await workspace.write("data/item.ab", instance);
  return { ...workspace, a, b, c, texts: new Map([[model.key(a), schema], [model.key(b), other], [model.key(c), instance]]) };
}
async function analyze(root, overlays = []) {
  const result = await protocol.startProcess(compiler, ["analyze", root, "--stdio", "--symbols"], { cwd: root, input: protocol.encodeRequest(19, overlays) }).promise;
  return protocol.response(result, 19);
}
async function compile(root, raw = false) {
  const result = await protocol.startProcess(compiler, ["compile", root, "JSON"], { cwd: root }).promise;
  assert.equal(result.error, null, result.stderr);
  return raw ? result.stdout : JSON.parse(result.stdout);
}

test("compiler binds every schema-use form across files, excluding value text/comments and case-distinct names", async () => {
  const f = await fixture();
  try {
    const response = await analyze(f.root); const graph = model.readBindings(response);
    model.validateSnapshot(graph, f.texts);
    const product = graph.symbols.find((symbol) => symbol.name === "Product");
    const owner = graph.symbols.find((symbol) => symbol.name === "Owner");
    assert.equal(product.occurrences.length, 3); // schema, logic, instance
    assert.equal(owner.occurrences.length, 3); // schema, nested, ref
    assert.equal(graph.symbols.find((symbol) => symbol.name === "product").occurrences.length, 1);
    assert.equal(model.symbolAt(graph, f.a, { line: 7, character: 38 }), undefined); // throw text
    assert.equal(model.symbolAt(graph, f.c, { line: 0, character: 2 }).symbol.id, product.id);
    assert.equal(product.occurrences.filter((entry) => entry.role !== "declaration").length, 2);
  } finally { await f.dispose(); }
});

test("semantic rename changes only the bound schema identity; canonical compiler output has the expected template change", async () => {
  const f = await fixture();
  try {
    const before = await compile(f.root);
    const graph = model.readBindings(await analyze(f.root));
    const product = graph.symbols.find((symbol) => symbol.name === "Product");
    const changes = model.rename(graph, product, "Catalog", f.texts);
    assert.equal(changes.size, 2);
    const afterGraph = model.readBindings(await analyze(f.root, [...changes].map(([file, text]) => ({ path: file, text }))));
    model.validateSnapshot(afterGraph, new Map([...f.texts, ...changes]));
    assert.equal(afterGraph.symbols.find((s) => s.name === "Catalog").occurrences.length, 3);
    assert.ok(changes.get(model.key(f.a)).includes("note: text = Product"));
    assert.ok(changes.get(model.key(f.a)).includes('throw "Product"'));
    assert.ok(changes.get(model.key(f.a)).includes("// Product is a comment"));
    // Only this disposable owner-created fixture is written to exercise the
    // normal canonical compile command; the provider itself never writes files.
    for (const [file, text] of changes) await fs.writeFile(file, text);
    const after = await compile(f.root);
    const expected = structuredClone(before);
    assert.equal(expected.data[0].template, "Product");
    expected.data[0].template = "Catalog";
    assert.deepEqual(after, expected);
  } finally { await f.dispose(); }
});

test("private nested schema rename preserves canonical output bytes and updates nested/ref targets", async () => {
  const f = await fixture();
  try {
    const before = await compile(f.root, true);
    const graph = model.readBindings(await analyze(f.root));
    const changes = model.rename(graph, graph.symbols.find((s) => s.name === "Owner"), "PrivateOwner", f.texts);
    assert.equal(changes.size, 2);
    assert.ok(changes.get(model.key(f.a)).includes("$(PrivateOwner)"));
    assert.ok(changes.get(model.key(f.a)).includes("ref(PrivateOwner)"));
    for (const [file, text] of changes) await fs.writeFile(file, text);
    assert.equal(await compile(f.root, true), before);
  } finally { await f.dispose(); }
});

test("unsaved simultaneous declaration/use edits and new sources bind without writing disk", async () => {
  const f = await fixture();
  try {
    const a = schema.replaceAll("Product", "Catalog"); const c = instance.replace("Product", "Catalog");
    const newPath = path.join(f.root, "data/new.ab"); const newText = "Catalog :: @id.new\nvalue: 8\n";
    const graph = model.readBindings(await analyze(f.root, [{ path: f.a, text: a }, { path: f.c, text: c }, { path: newPath, text: newText }]));
    model.validateSnapshot(graph, new Map([...f.texts, [model.key(f.a), a], [model.key(f.c), c], [model.key(newPath), newText]]));
    assert.equal(graph.symbols.find((s) => s.name === "Catalog").occurrences.length, 4);
    assert.equal(await fs.readFile(f.a, "utf8"), schema);
    assert.equal(await fs.readFile(f.c, "utf8"), instance);
    await assert.rejects(fs.stat(newPath), { code: "ENOENT" });
  } finally { await f.dispose(); }
});

test("BOM, CRLF and supplementary characters retain exact analyzed UTF-16 schema spans", async () => {
  const f = await fixture();
  try {
    const text = "\uFEFF" + schema.replaceAll("Product is a comment", "😀 Product is a comment").replaceAll("\n", "\r\n");
    await fs.writeFile(f.a, text);
    for (const overlays of [[], [{ path: f.a, text }]]) {
      const graph = model.readBindings(await analyze(f.root, overlays));
      const editorText = overlays.length ? text : text.slice(1);
      model.validateSnapshot(graph, new Map([...f.texts, [model.key(f.a), editorText]]));
      assert.equal(graph.symbols.find((s) => s.name === "Product").declaration.range.start.character, 7 + (overlays.length ? 1 : 0));
    }
  } finally { await f.dispose(); }
});

test("duplicate, unresolved and forbidden loop shadowing sources never expose complete bindings", async () => {
  const f = await fixture();
  try {
    for (const [text, code] of [
      [schema + "schema Product {\n}\n", "E301"],
      [schema.replace("$(Owner)", "$(Missing)"), "E309"],
      [schema.replace('require .value >= 0 else throw "Product"', "for $i in [a, b] {\nfor $i in [a, b] {\n}\n}"), "E516"]
    ]) {
      const response = await analyze(f.root, [{ path: f.a, text }]);
      assert.ok(response.diagnostics.some((item) => item.code === code), JSON.stringify(response.diagnostics));
      assert.equal(response.bindings.complete, false);
      assert.deepEqual(response.bindings.symbols, []);
      assert.throws(() => model.readBindings(response), /diagnostics/);
    }
  } finally { await f.dispose(); }
});

test("rename rejects collisions, malformed names, stale sources/membership and malformed binding payloads", async () => {
  const f = await fixture();
  try {
    const response = await analyze(f.root); const graph = model.readBindings(response); const product = graph.symbols.find((s) => s.name === "Product");
    assert.throws(() => model.rename(graph, product, "Owner", f.texts), /already declared/);
    for (const name of ["", "1Name", "two-words", "a.b", "é", "X\nY"]) assert.throws(() => model.rename(graph, product, name, f.texts), /ASCII letter/);
    assert.throws(() => model.rename(graph, product, "Catalog", new Map([...f.texts, [model.key(f.c), instance + "\n"]])), /changed/);
    assert.throws(() => model.validateSnapshot(graph, new Map([...f.texts].slice(1))), /membership/);
    for (const mutate of [
      (r) => { r.truncated = true; }, (r) => { r.bindings.complete = false; },
      (r) => { r.bindings.symbols[0].occurrences.push(r.bindings.symbols[0].occurrences[0]); },
      (r) => { r.bindings.symbols[0].declaration.range.start.character += 1; },
      (r) => { r.bindings.sources[0].sha256 = "incorrect"; }
    ]) { const bad = structuredClone(response); mutate(bad); assert.throws(() => model.readBindings(bad)); }
  } finally { await f.dispose(); }
});

test("feature negotiation requires schemaBindings v1, even when older diagnostic protocol is supported", async () => {
  const actual = protocol.capability(await protocol.startProcess(compiler, ["analyze", "--capabilities"]).promise);
  assert.equal(actual.schemaBindings, true);
  for (const schemaBindings of [undefined, 2, true, "1"]) {
    const value = protocol.capability({ error: null, stdout: JSON.stringify({ protocol: "abstract-analysis", version: 1, positionEncoding: "utf-16", schemaBindings }), stderr: "" });
    assert.equal(value.supported, true); assert.equal(value.schemaBindings, false);
  }
});

test("binding reader preserves normalized field/loop spelling and rejects implicit instance rename", () => {
  const file = path.resolve("semantic-fixture.abt");
  const text = "Max-Count\nmax_count\nLoop-Var\nloop_var\nanchor\nfile-id\n";
  const at = (line, spelling, role) => ({ path: file, spelling, role,
    range: { start: { line, character: 0 }, end: { line, character: spelling.length } } });
  const declaration = (line, spelling) => ({ path: file, spelling,
    range: { start: { line, character: 0 }, end: { line, character: spelling.length } } });
  const graph = {
    version: 1, complete: true, sources: [{ path: file, sha256: model.hash(text) }], symbols: [
      { id: "f", kind: "field", name: "max_count", ownerId: "schema", declaration: declaration(0, "Max-Count"),
        occurrences: [at(0, "Max-Count", "declaration"), at(1, "max_count", "reference")] },
      { id: "l", kind: "loop", name: "loop_var", scopeId: "logic", declaration: declaration(2, "Loop-Var"),
        occurrences: [at(2, "Loop-Var", "declaration"), at(3, "loop_var", "reference")] },
      { id: "i", kind: "instance", name: "file_id", implicit: true, renamable: false,
        declaration: declaration(4, ""), occurrences: [at(4, "", "declaration"), at(5, "file-id", "reference")] }
    ]
  };
  const response = { analyzed: true, truncated: false, diagnostics: [], bindings: graph };
  const parsed = model.readBindings(response); const texts = new Map([[model.key(file), text]]);
  model.validateSnapshot(parsed, texts);
  const changed = model.rename(parsed, parsed.symbols[0], "total-value", texts).get(model.key(file));
  assert.ok(changed.startsWith("total-value\ntotal-value\n"));
  assert.throws(() => model.rename(parsed, parsed.symbols[2], "other", texts), /file stem/);
  parsed.symbols[0].renamable = false;
  assert.throws(() => model.rename(parsed, parsed.symbols[0], "other", texts), /prove.*every use/);
});
