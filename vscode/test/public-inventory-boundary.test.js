const { test } = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");
const fs = require("node:fs/promises");
const vm = require("node:vm");
const { createRequire } = require("node:module");
const bindings = require("../src/schema-bindings");
const { readInventory, items } = require("../src/public-inventory");
const protocol = require("../src/analysis-client");
const { temporaryWorkspace } = require("./test-workspace");
const { createSourceCapture } = require("../src/source-capture");
const { discoveryRoot, discover } = require("../src/project-index");

// Independently authored wire/source pairs. No compiler invocation, disk source
// mutation, or production helper constructs the expected inventory. These test
// protocol admission and presentation, not the compiler's dependency analysis.
function fixture(version = 1, directory = __dirname) {
  const schemaPath = path.resolve(directory, "owned-inventory-schema.abt");
  const rootPath = path.resolve(directory, "owned-inventory-root.ab");
  const lines = [
    `versions ${version}..${version}`,
    "schema PublicSettings {",
    "    flag: bool @public = true",
    "    maybe: int @optional @public",
    "    note: text(1..8) @public = \"A😀é\"",
    "    offset: float @public = -0.0",
    "    quota: int(-9223372036854775808..9223372036854775807) @public = 9007199254740993",
    "}",
  ];
  const schema = `${lines.join("\r\n")}\r\n`;
  const root = "PublicSettings :: @id.main\n";
  const texts = new Map([[bindings.key(schemaPath), schema], [bindings.key(rootPath), root]]);
  const location = (file, line, start, token) => ({ path: file,
    range: { start: { line, character: start }, end: { line, character: start + token.length } } });
  const field = (name, type, line, optional = false) => ({
    declaringSchema: "PublicSettings", declaringPath: [name], spelling: name,
    type, list: false, optional, tag: false, versions: { min: version, max: version },
    declaration: location(schemaPath, line, lines[line].indexOf(name), name),
    nomination: location(schemaPath, line, lines[line].indexOf("@public"), "@public"),
    nominationSpelling: "@public",
  });
  const interval = { min: version, max: version };
  const target = (name, domain, value, present = true) => ({
    rootSchema: "PublicSettings", rootInstanceId: "main", path: [name],
    declaringSchema: "PublicSettings", declaringPath: [name],
    declarationVersions: { ...interval }, domain,
    variants: [{ versions: { ...interval }, presence: present ? "present" : "absent",
      writable: present, ...(present ? { default: value } : {}) }],
  });
  const response = {
    protocol: "abstract-analysis", version: 1, requestId: 7, compiler: "1.1.0",
    positionEncoding: "utf-16", analyzed: true, truncated: false, diagnostics: [],
    publicInventory: {
      version: 1, status: "export-admitted", catalogComplete: true,
      projectVersions: { ...interval },
      sources: [...texts].map(([file, text]) => ({ path: file, sha256: bindings.hash(text) })),
      declarations: [field("flag", "bool", 2), field("maybe", "int", 3, true),
        field("note", "text", 4), field("offset", "float", 5), field("quota", "int", 6)],
      roots: [{ rootSchema: "PublicSettings", rootInstanceId: "main",
        declaration: location(rootPath, 0, 0, "PublicSettings") }],
      exportDiagnostics: [],
      fragment: { formatVersion: 1, dependencyProfile: "independent-scalars-v1", complete: true,
        bindingStatus: "unbound", entries: [
          target("flag", { type: "bool" }, true),
          target("maybe", { type: "int", ranges: [] }, undefined, false),
          target("note", { type: "text", ranges: [{ min: "1", max: "8" }], lengthUnit: "unicode-scalars" }, "A😀é"),
          target("offset", { type: "float", ranges: [] }, "8000000000000000"),
          target("quota", { type: "int", ranges: [{ min: "-9223372036854775808", max: "9223372036854775807" }] }, "9007199254740993"),
        ] },
    },
  };
  return { response, texts, schemaPath, rootPath };
}

const entry = (response, name) => response.publicInventory.fragment.entries.find((value) => value.path[0] === name);
const readableRows = (inventory) => items(inventory).map(({ label, description, detail }) => [label, description, detail].filter(Boolean).join("\n")).join("\n");

function rejectsMutation(mutate) {
  const f = fixture();
  assert.equal(readInventory(f.response, f.texts).status, "export-admitted", "the independent unmodified fixture must first be admitted");
  mutate(f.response, f);
  assert.throws(() => readInventory(f.response, f.texts));
}

test("independent inventory keeps lossless scalar values and presents the actual i64 and float bits", () => {
  const f = fixture();
  const inventory = readInventory(f.response, f.texts);
  const quota = inventory.fragment.entries.find((value) => value.path[0] === "quota");
  const offset = inventory.fragment.entries.find((value) => value.path[0] === "offset");
  assert.equal(quota.variants[0].default, "9007199254740993");
  assert.equal(quota.domain.ranges[0].min, "-9223372036854775808");
  assert.equal(quota.domain.ranges[0].max, "9223372036854775807");
  assert.equal(offset.variants[0].default, "8000000000000000");
  const display = readableRows(inventory);
  assert.match(display, /9007199254740993/);
  assert.match(display, /8000000000000000/);
  assert.doesNotMatch(display, /9007199254740992/);
});

test("numeric wire corruption cannot silently become a different public default", () => {
  for (const value of [9007199254740993, "9223372036854775808", "-9223372036854775809", "+1", "01", "-0"]) {
    rejectsMutation((response) => { entry(response, "quota").variants[0].default = value; });
  }
  for (const value of [0, "800000000000000", "7ff0000000000000", "7ff8000000000000"]) {
    rejectsMutation((response) => { entry(response, "offset").variants[0].default = value; });
  }
});

test("a rejected export retains nominations only, never a partial admitted fragment", () => {
  const f = fixture();
  const inventory = f.response.publicInventory;
  inventory.status = "export-rejected";
  inventory.reason = "The nominated field has a dependency outside the admitted profile.";
  inventory.roots = [];
  inventory.exportDiagnostics = [{ code: "E702", severity: "error", message: "Public export rejected.", notes: [] }];
  delete inventory.fragment;
  const rejected = readInventory(f.response, f.texts);
  assert.equal(rejected.status, "export-rejected");
  assert.equal(rejected.declarations.length, 5);
  assert.equal(rejected.fragment, undefined);
  const withPartial = structuredClone(f.response);
  withPartial.publicInventory.fragment = fixture().response.publicInventory.fragment;
  assert.throws(() => readInventory(withPartial, f.texts));
  const falselyAdmitted = structuredClone(f.response);
  falselyAdmitted.publicInventory.status = "export-admitted";
  assert.throws(() => readInventory(falselyAdmitted, f.texts));
});

test("real diagnostic display paths remain presentation text while navigation uses captured canonical paths", () => {
  const f = fixture();
  const inventory = f.response.publicInventory;
  inventory.status = "export-rejected";
  inventory.roots = [];
  delete inventory.fragment;
  const nominated = inventory.declarations[0].nomination;
  inventory.exportDiagnostics = [{ code: "E702", severity: "error", message: "Public export rejected.",
    displayPath: "data/model.abt", ...structuredClone(nominated),
    notes: [{ message: "Related source.", displayPath: "display-only/../label.ab", path: f.rootPath }] }];
  const accepted = readInventory(f.response, f.texts);
  assert.equal(accepted.status, "export-rejected");
  const rows = items(accepted);
  assert.equal(rows.length, 5);
  assert.ok(rows.every((row) => row.kind === "nomination" && bindings.key(row.location.path) === bindings.key(f.schemaPath)));
  const foreign = structuredClone(f.response);
  foreign.publicInventory.exportDiagnostics[0].notes[0].path = path.resolve(__dirname, "not-a-captured-source.ab");
  assert.throws(() => readInventory(foreign, f.texts));
});

test("ordinary errors, truncation and an incomplete catalog cannot accompany export admission", () => {
  rejectsMutation((response) => { response.analyzed = false; });
  rejectsMutation((response) => { response.truncated = true; });
  rejectsMutation((response) => { response.diagnostics = [{ code: "E302", severity: "error", message: "Duplicate field.", notes: [] }]; });
  rejectsMutation((response) => { response.publicInventory.catalogComplete = false; });
  rejectsMutation((response) => { response.publicInventory.fragment.complete = false; });
  rejectsMutation((response) => { response.publicInventory.fragment.bindingStatus = "bound"; });
});

test("source membership, digests and token locations are checked against exact captured text", () => {
  rejectsMutation((response) => { response.publicInventory.sources.pop(); });
  rejectsMutation((response) => { response.publicInventory.sources.push({ ...response.publicInventory.sources[0] }); });
  rejectsMutation((response) => { response.publicInventory.sources[0].sha256 = "0".repeat(64); });
  rejectsMutation((_response, f) => { f.texts.set(bindings.key(f.schemaPath), f.texts.get(bindings.key(f.schemaPath)) + "// changed without watcher\n"); });
  rejectsMutation((response) => { response.publicInventory.declarations[0].nomination.range.start.character += 1; });
  rejectsMutation((response) => { response.publicInventory.declarations[0].declaration.range.end.character += 1; });
  rejectsMutation((response) => { response.publicInventory.declarations[0].declaration.range.start.line = 999; });
  rejectsMutation((response) => { response.publicInventory.roots[0].declaration.path = path.resolve(__dirname, "foreign-not-in-capture.ab"); });
});

test("materialized targets cannot substitute a declaring schema or root with a same-looking name", () => {
  rejectsMutation((response) => { entry(response, "quota").declaringSchema = "OtherSettings"; });
  rejectsMutation((response) => { entry(response, "quota").declaringPath = ["flag"]; });
  rejectsMutation((response) => { entry(response, "quota").rootInstanceId = "other"; });
  rejectsMutation((response) => { response.publicInventory.roots.push(structuredClone(response.publicInventory.roots[0])); });
  rejectsMutation((response) => { response.publicInventory.declarations.push(structuredClone(response.publicInventory.declarations[0])); });
  rejectsMutation((response) => { response.publicInventory.fragment.entries.push(structuredClone(entry(response, "quota"))); });
});

test("optional absence remains non-writable and carries no invented default", () => {
  const f = fixture();
  const inventory = readInventory(f.response, f.texts);
  const absent = inventory.fragment.entries.find((value) => value.path[0] === "maybe").variants[0];
  assert.equal(absent.presence, "absent");
  assert.equal(absent.writable, false);
  assert.equal(Object.hasOwn(absent, "default"), false);
  rejectsMutation((response) => { entry(response, "maybe").variants[0].writable = true; });
  rejectsMutation((response) => { entry(response, "maybe").variants[0].default = null; });
  rejectsMutation((response) => { entry(response, "quota").variants[0].writable = false; });
});

test("version windows remain exact u32 values rather than accepting unsafe generic JSON integers", () => {
  const maximum = 0xFFFFFFFF;
  const f = fixture(maximum);
  const admitted = readInventory(f.response, f.texts);
  assert.equal(admitted.projectVersions.max, maximum);
  for (const maximum of [-1, 0x100000000, Number.MAX_SAFE_INTEGER + 1, 1.5]) {
    rejectsMutation((response) => { response.publicInventory.projectVersions.max = maximum; });
  }
});

// Exercise the real command and real filesystem capture with a deterministic
// compiler transport stub. No compiler, extension host, or user profile starts.
async function navigationFixture({ schemaOpen = true } = {}) {
  const workspace = await temporaryWorkspace();
  const data = fixture(1, path.join(workspace.root, "data"));
  for (const [file, text] of data.texts) await workspace.write(path.join("data", path.basename(file)), text);
  const callbacks = new Map(); const commands = new Map(); const shown = []; const warnings = [];
  const uri = (file) => ({ fsPath: file, scheme: "file", toString() { return file; } });
  const makeDocument = (file, text) => ({ uri: uri(file), text, version: 1, languageId: "abstract", isClosed: false, isDirty: false,
    get lineCount() { return this.text.split("\n").length; }, offsetAt() { return this.text.length; }, getText() { return this.text; } });
  const rootDocument = makeDocument(data.rootPath, data.texts.get(bindings.key(data.rootPath)));
  const schemaDocument = makeDocument(data.schemaPath, data.texts.get(bindings.key(data.schemaPath)));
  const documents = schemaOpen ? [rootDocument, schemaDocument] : [rootDocument];
  const event = (name) => (callback) => { callbacks.set(name, callback); return { dispose() { if (callbacks.get(name) === callback) callbacks.delete(name); } }; };
  let selectedPicker; let finish;
  const state = { ...data, workspace, rootDocument, schemaDocument, shown, warnings, documents, openCalls: 0, openHook: undefined };
  const vscode = {
    Uri: { file: uri }, Position: class { constructor(line, character) { this.line = line; this.character = character; } },
    Range: class { constructor(...values) { this.values = values; } },
    DiagnosticSeverity: { Error: 0 },
    workspace: {
      isTrusted: true, textDocuments: documents,
      createFileSystemWatcher: () => ({ dispose() {}, onDidCreate: event("create"), onDidChange: event("disk"), onDidDelete: event("delete") }),
      onDidChangeTextDocument: event("text"), onDidOpenTextDocument: event("open"),
      onDidCloseTextDocument: event("close"), onDidChangeConfiguration: event("config"),
      getConfiguration: () => ({ get: () => "owned-fixture-compiler" }),
      async openTextDocument(target) {
        state.openCalls += 1;
        const document = state.openHook ? await state.openHook(target) :
          (bindings.key(target.fsPath) === bindings.key(schemaDocument.uri.fsPath) ? schemaDocument : rootDocument);
        if (!documents.includes(document)) { documents.push(document); callbacks.get("open")?.(document); }
        return document;
      },
    },
    languages: { createDiagnosticCollection: () => ({ clear() {}, set() {}, dispose() {} }) },
    commands: { registerCommand(name, callback) { commands.set(name, callback); return { dispose() { commands.delete(name); } }; } },
    window: {
      activeTextEditor: { document: rootDocument },
      async showInformationMessage(message) { warnings.push(message); },
      async showWarningMessage(message) { warnings.push(message); finish?.({ kind: "warning", message }); },
      async showTextDocument(document, options) { shown.push({ document, options }); finish?.({ kind: "shown", document, options }); return { document }; },
      createQuickPick() {
        const handlers = new Map();
        const subscribe = (name) => (callback) => { handlers.set(name, callback); return { dispose() { handlers.delete(name); } }; };
        selectedPicker = { items: [], selectedItems: [], handlers, disposed: false,
          onDidAccept: subscribe("accept"), onDidHide: subscribe("hide"), show() {}, dispose() { this.disposed = true; } };
        return selectedPicker;
      },
    },
  };
  const providerPath = path.resolve(__dirname, "../src/public-inventory-provider.js");
  const actualRequire = createRequire(providerPath);
  const mockedProtocol = { ...protocol, startProcess(_compiler, args, options) {
    const value = args.includes("--capabilities")
      ? { protocol: "abstract-analysis", version: 1, compiler: "1.1.0", positionEncoding: "utf-16", publicInventory: 1 }
      : { ...structuredClone(data.response), requestId: options.input.readUInt32BE(8) };
    return { promise: Promise.resolve({ error: null, stdout: JSON.stringify(value), stderr: "", cancelled: false }), cancel() {} };
  } };
  const module = { exports: {} }; const context = { subscriptions: [] };
  vm.runInNewContext(await fs.readFile(providerPath, "utf8"), { module, exports: module.exports, Buffer, TextDecoder,
    require: (name) => name === "vscode" ? vscode : name === "./analysis-client" ? mockedProtocol : actualRequire(name) }, { filename: providerPath });
  module.exports.registerPublicInventory(context, () => workspace.root, () => {});
  return Object.assign(state, {
    async inspect() {
      const result = await commands.get("abstract.inspectPublicFields")();
      assert.equal(result.status, "export-admitted", warnings.join("\n"));
      assert.equal(result.targets, 5);
      assert.ok(selectedPicker && !selectedPicker.disposed);
      return result;
    },
    async accept() {
      const row = selectedPicker.items.find((item) => item.kind === "nomination");
      assert.ok(row); selectedPicker.selectedItems = [row];
      const completed = new Promise((resolve) => { finish = resolve; });
      let timer;
      try {
        selectedPicker.handlers.get("accept")();
        const result = await Promise.race([completed, new Promise((_resolve, reject) => { timer = setTimeout(() => reject(new Error("Navigation produced no terminal observation.")), 3000); })]);
        await new Promise(setImmediate); // Let the provider finish after its observed UI call before disposing the fixture.
        return result;
      } finally { clearTimeout(timer); finish = undefined; }
    },
    async dispose() { for (const item of context.subscriptions) item.dispose(); await workspace.dispose(); },
  });
}

test("public navigation positive control opens the bound source and nomination range", async () => {
  const f = await navigationFixture();
  try {
    await f.inspect();
    const result = await f.accept();
    assert.equal(result.kind, "shown");
    assert.equal(result.document, f.schemaDocument);
    assert.equal(f.shown.length, 1);
    const expected = f.response.publicInventory.declarations[0].nomination.range;
    assert.deepEqual(Array.from(result.options.selection.values), [expected.start.line, expected.start.character, expected.end.line, expected.end.character]);
  } finally { await f.dispose(); }
});

test("navigation rejects changed open versions, saved bytes and membership without relying on watcher events", async () => {
  for (const change of ["buffer", "disk", "membership"]) {
    const f = await navigationFixture();
    try {
      await f.inspect();
      if (change === "buffer") { f.schemaDocument.text += "// dirty\n"; f.schemaDocument.version += 1; f.schemaDocument.isDirty = true; }
      else if (change === "disk") await fs.appendFile(f.schemaPath, "// unwatched saved change\n");
      else await f.workspace.write("data/new-source.ab", "");
      const result = await f.accept();
      assert.equal(result.kind, "warning", change);
      assert.equal(f.shown.length, 0, change);
      assert.equal(f.openCalls, 0, change);
    } finally { await f.dispose(); }
  }
});

test("navigation revalidates an open alias after the picker has been displayed", async () => {
  const f = await navigationFixture();
  try {
    const alias = path.join(f.workspace.root, "owned-alias");
    const other = path.join(f.workspace.root, "elsewhere");
    await f.workspace.write("elsewhere/owned-inventory-schema.abt", f.schemaDocument.text);
    await fs.symlink(path.join(f.workspace.root, "data"), alias, process.platform === "win32" ? "junction" : "dir");
    f.schemaDocument.uri = { scheme: "file", fsPath: path.join(alias, path.basename(f.schemaPath)), toString() { return this.fsPath; } };
    await f.inspect();
    await fs.unlink(alias);
    await fs.symlink(other, alias, process.platform === "win32" ? "junction" : "dir");
    const result = await f.accept();
    assert.equal(result.kind, "warning");
    assert.equal(f.openCalls, 0); assert.equal(f.shown.length, 0);
    assert.equal(await fs.readFile(f.schemaPath, "utf8"), f.schemaDocument.text);
    assert.equal(await fs.readFile(path.join(other, path.basename(f.schemaPath)), "utf8"), f.schemaDocument.text);
  } finally { await f.dispose(); }
});

test("a newly opened oversized navigation target rejects before explicit full-text materialization", async () => {
  for (const tooManyLines of [true, false]) {
    const f = await navigationFixture({ schemaOpen: false });
    let gets = 0; let offsets = 0;
    try {
      await f.inspect();
      const oversized = { ...f.schemaDocument,
        lineCount: tooManyLines ? 4 * 1024 * 1024 + 2 : 1,
        offsetAt() { offsets += 1; return 4 * 1024 * 1024 + 1; },
        getText() { gets += 1; throw new Error("The oversized source must not be materialized."); } };
      f.openHook = async () => oversized;
      const result = await f.accept();
      assert.equal(result.kind, "warning");
      assert.equal(f.openCalls, 1);
      assert.equal(gets, 0); assert.equal(offsets, tooManyLines ? 0 : 1);
      assert.equal(f.shown.length, 0);
    } finally { await f.dispose(); }
  }
});

test("source capture rejects a changed discovery selection even when its old file inventory is unchanged", async () => {
  const workspace = await temporaryWorkspace();
  try {
    const sourceText = "Settings :: @id.main\n";
    const sourcePath = await workspace.write("settings.ab", sourceText);
    const document = { uri: { scheme: "file", fsPath: sourcePath, toString() { return sourcePath; } },
      version: 1, isClosed: false, isDirty: false, lineCount: 2,
      offsetAt() { return sourceText.length; }, getText() { return sourceText; } };
    // No watcher/event implementation: membership must be checked from disk.
    const vscode = { Position: class { constructor(line, character) { this.line = line; this.character = character; } },
      workspace: { isTrusted: true, textDocuments: [document], getConfiguration: () => ({ get: () => "unused-compiler" }) } };
    const capture = createSourceCapture(vscode, () => workspace.root, () => workspace.root, () => {});
    const state = await capture.capture(document, {});
    await capture.current(state); // The unchanged selection is a positive control.
    const before = [...state.diskFiles];
    await fs.mkdir(path.join(workspace.root, "data"));
    assert.deepEqual(await discover(state.discovery, state.limits), before,
      "the old source list is deliberately unchanged, isolating discovery selection");
    const selectedNow = await discoveryRoot(state.root, state.limits);
    assert.notEqual(bindings.key(selectedNow), bindings.key(state.discovery));
    assert.deepEqual(await discover(selectedNow, state.limits), []);
    await assert.rejects(capture.current(state), /Project discovery selection changed during source analysis/);
    assert.equal(await fs.readFile(sourcePath, "utf8"), sourceText);
  } finally { await workspace.dispose(); }
});
