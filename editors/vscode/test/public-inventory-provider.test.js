const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs/promises");
const path = require("node:path");
const vm = require("node:vm");
const { createRequire } = require("node:module");
const protocol = require("../src/analysis-client");
const { hash } = require("../src/schema-bindings");
const { temporaryWorkspace } = require("./test-workspace");

// Command tests isolate the host/process seams. Compiler semantics are covered
// by the separate Rust and real Extension Host gates, not by this fake process.
async function fixture() {
  const workspace = await temporaryWorkspace();
  const callbacks = new Map();
  const commands = new Map();
  const collections = new Map();
  const documents = [];
  const calls = [];
  const picks = [];
  const shown = [];
  const notices = [];
  const reports = [];
  const captured = [];
  const projects = [];
  const context = { subscriptions: [] };
  const uri = (file) => ({ scheme: "file", fsPath: file, toString() { return file; } });
  const event = (name) => (fn) => { callbacks.set(name, fn); return { dispose() {} }; };
  async function project(name) {
    const root = path.join(workspace.root, name);
    const schemaText = "schema Settings {\n    flag: bool @public = true\n}\n";
    const schema = await workspace.write(`${name}/data/model.abt`, schemaText);
    const instance = await workspace.write(`${name}/data/instances.ab`, "Settings :: @id.main\n");
    const doc = (file, text) => ({ uri: uri(file), languageId: "abstract", text, version: 1, isDirty: false, isClosed: false,
      get lineCount() { return this.text.split("\n").length; }, offsetAt() { return this.text.length; }, getText() { return this.text; } });
    const schemaDoc = doc(schema, schemaText);
    const instanceDoc = doc(instance, "Settings :: @id.main\n");
    documents.push(schemaDoc, instanceDoc);
    const result = { root, schemaDoc, instanceDoc };
    projects.push(result);
    return result;
  }
  const one = await project("one");
  const two = await project("two");
  const vscode = {
    Uri: { file: uri },
    Position: class { constructor(line, character) { this.line = line; this.character = character; } },
    Range: class { constructor(a, b, c, d) { this.start = { line: a, character: b }; this.end = { line: c, character: d }; } },
    Diagnostic: class { constructor(range, message, severity) { Object.assign(this, { range, message, severity }); } },
    DiagnosticSeverity: { Error: 0 },
    Location: class { constructor(uri, range) { Object.assign(this, { uri, range }); } },
    DiagnosticRelatedInformation: class { constructor(location, message) { Object.assign(this, { location, message }); } },
    workspace: {
      isTrusted: true, textDocuments: documents,
      createFileSystemWatcher: () => ({ dispose() {}, onDidCreate: event("created"), onDidDelete: event("deleted"), onDidChange: event("disk") }),
      onDidChangeTextDocument: event("text"), onDidOpenTextDocument: event("open"),
      onDidCloseTextDocument: event("close"), onDidChangeConfiguration: event("config"),
      getConfiguration: () => ({ get: () => "owned-compiler" }),
      openTextDocument: async (selected) => documents.find((d) => d.uri.fsPath === selected.fsPath),
    },
    languages: { createDiagnosticCollection(name) {
      const values = new Map(); collections.set(name, values);
      return { clear() { values.clear(); }, set(uri, diagnostics) { values.set(uri.toString(), diagnostics); }, dispose() {} };
    } },
    commands: { registerCommand(name, callback) { commands.set(name, callback); return { dispose() {} }; } },
    window: {
      activeTextEditor: { document: one.instanceDoc },
      // Deliberately unresolved: informational notification dismissal must not
      // block the command's readiness/unavailable result.
      showInformationMessage(message) { notices.push(message); return new Promise(() => {}); },
      showWarningMessage(message) { notices.push(message); return new Promise(() => {}); },
      async showTextDocument(document, options) { shown.push({ document, options }); },
      createQuickPick() {
        const events = {};
        const pick = { selectedItems: [], shown: false, disposed: false,
          onDidAccept(fn) { events.accept = fn; return { dispose() {} }; },
          onDidHide(fn) { events.hide = fn; return { dispose() {} }; },
          show() { this.shown = true; }, dispose() { this.disposed = true; },
          accept(index = 0) { this.selectedItems = [this.items[index]]; events.accept(); },
          hide() { events.hide(); },
        };
        picks.push(pick); return pick;
      },
    },
  };
  const location = (file, line, start, end) => ({ path: file, range: { start: { line, character: start }, end: { line, character: end } } });
  function response(args, options) {
    const selected = projects.find((candidate) => args[1] === candidate.root);
    const texts = [selected.schemaDoc, selected.instanceDoc];
    const frame = options.input;
    const output = { protocol: "abstract-analysis", version: 1, requestId: frame.readUInt32BE(8), compiler: "1.1.1",
      positionEncoding: "utf-16", analyzed: true, truncated: false, diagnostics: [], publicInventory: {
        version: 1, status: "export-admitted", catalogComplete: true, projectVersions: { min: 1, max: 1 },
        sources: texts.map((d) => ({ path: d.uri.fsPath, sha256: hash(d.text) })),
        declarations: [{ declaringSchema: "Settings", declaringPath: ["flag"], spelling: "flag", nominationSpelling: "@public",
          type: "bool", list: false, optional: false, tag: false, versions: { min: 1, max: 1 },
          declaration: location(selected.schemaDoc.uri.fsPath, 1, 4, 8), nomination: location(selected.schemaDoc.uri.fsPath, 1, 15, 22) }],
        roots: [{ rootSchema: "Settings", rootInstanceId: "main", declaration: location(selected.instanceDoc.uri.fsPath, 0, 0, 8) }],
        exportDiagnostics: [], fragment: { formatVersion: 1, dependencyProfile: "independent-scalars-v1", complete: true,
          bindingStatus: "unbound", entries: [{ rootSchema: "Settings", rootInstanceId: "main", path: ["flag"],
            declaringSchema: "Settings", declaringPath: ["flag"], declarationVersions: { min: 1, max: 1 }, domain: { type: "bool" },
            variants: [{ versions: { min: 1, max: 1 }, presence: "present", writable: true, default: true }] }] },
      } };
    return output;
  }
  const state = { workspace, one, two, project, captured, vscode, documents, calls, picks, shown, notices, reports, collections, callbacks,
    capability: true, alter: (value) => value,
    inspect: () => commands.get("abstract.inspectPublicFields")(),
    async dispose() { for (const item of context.subscriptions) item.dispose(); await workspace.dispose(); },
  };
  const wrapped = { ...protocol, startProcess(compiler, args, options) {
    calls.push({ compiler, args, options });
    let value = args.includes("--capabilities")
      ? { protocol: "abstract-analysis", version: 1, positionEncoding: "utf-16", compiler: "1.1.1", ...(state.capability ? { publicInventory: 1 } : {}) }
      : state.alter(response(args, options));
    return { cancel() {}, promise: Promise.resolve({ stdout: JSON.stringify(value), stderr: "", cancelled: false }) };
  } };
  const file = path.resolve(__dirname, "../src/public-inventory-provider.js");
  const module = { exports: {} };
  const original = createRequire(file);
  const sourceCapture = original("./source-capture");
  const tracedCapture = { ...sourceCapture, createSourceCapture(...args) {
    const actual = sourceCapture.createSourceCapture(...args);
    return { ...actual, capture(document, snapshot) {
      captured.push(snapshot);
      return actual.capture(document, snapshot);
    } };
  } };
  // Optional archived provider is used solely for red/green lifecycle evidence;
  // production files are never restored or rewritten by the test.
  const source = process.env.PUBLIC_INVENTORY_PROVIDER_SOURCE || file;
  vm.runInNewContext(await fs.readFile(source, "utf8"), { module, exports: module.exports, Buffer,
    require: (name) => name === "vscode" ? vscode : name === "./analysis-client" ? wrapped
      : name === "./source-capture" ? tracedCapture : original(name) }, { filename: source });
  module.exports.registerPublicInventory(context, (document) => projects.find((candidate) => document.uri.fsPath.startsWith(candidate.root + path.sep))?.root,
    (message) => reports.push(message), () => workspace.root);
  return state;
}

function released(snapshot) {
  assert.ok(!snapshot.sources || snapshot.sources.size === 0, "captured source texts must be released");
  assert.ok(!snapshot.texts || snapshot.texts.size === 0, "snapshot text index must be released");
  assert.ok(!snapshot.documents || snapshot.documents.length === 0, "captured editor references must be released");
  assert.equal(snapshot.document, undefined, "initiating TextDocument must not remain owned by a closed snapshot");
}
async function until(predicate) {
  const deadline = Date.now() + 3000;
  while (!predicate()) {
    if (Date.now() >= deadline) throw new Error("Expected asynchronous host seam was not reached.");
    await new Promise((resolve) => setImmediate(resolve));
  }
}
const rejected = (response) => {
  const inventory = response.publicInventory;
  inventory.status = "export-rejected"; delete inventory.fragment; inventory.roots = [];
  inventory.exportDiagnostics = [{ code: "E702", severity: "error", message: "Export rejected.",
    ...inventory.declarations[0].nomination, notes: [] }];
  return response;
};

test("command returns native-picker readiness and sends exact dirty overlays only for its project", async () => {
  const f = await fixture();
  try {
    f.one.schemaDoc.text += "// own dirty source\n"; f.one.schemaDoc.isDirty = true;
    f.two.schemaDoc.text += "// private foreign dirty source\n"; f.two.schemaDoc.isDirty = true;
    const summary = await f.inspect();
    assert.equal(summary.status, "export-admitted");
    assert.equal(summary.declarations, 1); assert.equal(summary.targets, 1);
    assert.equal(f.picks.length, 1); assert.equal(f.picks[0].shown, true); assert.equal(f.shown.length, 0);
    assert.equal(f.picks[0].activeItems[0], f.picks[0].items[0]);
    const run = f.calls[1];
    assert.deepEqual(Array.from(run.args), ["analyze", f.one.root, "--stdio", "--public"]);
    assert.equal(run.options.input.readUInt32BE(12), 1);
    assert.ok(run.options.input.includes(Buffer.from(f.one.schemaDoc.text)));
    assert.equal(run.options.input.includes(Buffer.from(f.two.schemaDoc.text)), false);
    assert.equal(f.collections.has("abstract-public"), true);
  } finally { await f.dispose(); }
});

test("old capability and producer mismatch return unavailable without saved-file fallback or notification dismissal", async () => {
  const f = await fixture();
  try {
    f.capability = false;
    assert.equal((await f.inspect()).status, "unavailable");
    assert.equal(f.calls.length, 1); assert.equal(f.picks.length, 0);
    f.capability = true;
    f.alter = (response) => ({ ...response, compiler: "different" });
    assert.equal((await f.inspect()).status, "unavailable");
    assert.equal(f.picks.length, 0); assert.match(f.notices.at(-1), /negotiated compiler/);
    assert.ok(f.calls.every(({ args }) => args[0] === "analyze"));
  } finally { await f.dispose(); }
});

test("untrusted invocation starts no child process and does not block on a notification", async () => {
  const f = await fixture();
  try {
    f.vscode.workspace.isTrusted = false;
    assert.equal((await f.inspect()).status, "unavailable");
    assert.equal(f.calls.length, 0); assert.equal(f.picks.length, 0);
  } finally { await f.dispose(); }
});

test("an unrelated open source with a missing parent does not enter or block the project capture", async () => {
  const f = await fixture();
  try {
    const file = path.join(f.workspace.root, "removed-other-project", "data", "new.ab");
    let reads = 0;
    f.documents.push({ ...f.two.instanceDoc, uri: { scheme: "file", fsPath: file, toString() { return file; } },
      getText() { reads += 1; throw new Error("unrelated buffer must not be captured"); } });
    assert.equal((await f.inspect()).status, "export-admitted");
    assert.equal(reads, 0);
  } finally { await f.dispose(); }
});

test("export diagnostics are separate, retain related positions and stay isolated from another project's changes", async () => {
  const f = await fixture();
  try {
    f.alter = (response) => {
      const inventory = response.publicInventory;
      inventory.status = "export-rejected"; delete inventory.fragment; inventory.roots = [];
      const source = inventory.declarations[0].nomination;
      inventory.exportDiagnostics = [{ code: "E702", severity: "error", message: "Export rejected.", ...source,
        notes: [{ message: "Dependency here.", ...source }] }];
      return response;
    };
    assert.equal((await f.inspect()).status, "export-rejected");
    const collection = f.collections.get("abstract-public");
    const diagnostics = collection.get(f.one.schemaDoc.uri.toString());
    assert.equal(diagnostics[0].source, "abstract-public");
    assert.equal(diagnostics[0].relatedInformation.length, 1);
    assert.ok(f.picks[0].items.every((item) => !item.description.includes("admitted")));
    f.alter = (response) => response;
    f.vscode.window.activeTextEditor = { document: f.two.instanceDoc };
    assert.equal((await f.inspect()).status, "export-admitted");
    assert.equal(collection.has(f.one.schemaDoc.uri.toString()), true);
    f.two.schemaDoc.version += 1;
    f.callbacks.get("text")({ document: f.two.schemaDoc });
    assert.equal(f.picks.at(-1).disposed, true);
    assert.equal(collection.has(f.one.schemaDoc.uri.toString()), true);
  } finally { await f.dispose(); }
});

test("empty admitted inventory resolves without a picker or waiting for informational UI", async () => {
  const f = await fixture();
  try {
    f.alter = (response) => {
      response.publicInventory.declarations = []; response.publicInventory.roots = [];
      response.publicInventory.fragment.entries = []; return response;
    };
    const summary = await f.inspect();
    assert.equal(summary.status, "export-admitted"); assert.equal(summary.rows, 0);
    assert.equal(f.picks.length, 0); assert.match(f.notices[0], /no nominated/);
  } finally { await f.dispose(); }
});

test("lifecycle releases completed and invalidated heavy snapshots across many projects and bounds diagnostic history", async () => {
  const f = await fixture();
  try {
    f.alter = rejected;
    for (let i = 0; i < 20; i += 1) {
      const project = await f.project(`lifetime-${i}`);
      f.vscode.window.activeTextEditor = { document: project.instanceDoc };
      assert.equal((await f.inspect()).status, "export-rejected");
      const current = f.captured.at(-1);
      assert.ok(current.sources.size > 0, "open picker still owns its revalidation snapshot");
      f.picks.at(-1).hide();
      for (const snapshot of f.captured) released(snapshot);
    }
    const diagnostics = f.collections.get("abstract-public");
    assert.equal(diagnostics.size, 16, "retained diagnostic projects have a fixed count bound");
    assert.equal([...diagnostics.keys()].some((key) => key.includes(`lifetime-0${path.sep}`)), false);
    const latest = f.vscode.window.activeTextEditor.document;
    f.callbacks.get("text")({ document: latest });
    assert.equal(diagnostics.size, 15, "closed snapshot diagnoses still invalidate by lightweight source identity");
    assert.equal((await f.inspect()).status, "export-rejected");
    const invalidated = f.captured.at(-1);
    f.callbacks.get("text")({ document: latest });
    released(invalidated);
  } finally { await f.dispose(); }
});

test("lifecycle releases unavailable and empty results without retaining a historical source snapshot", async () => {
  const f = await fixture();
  try {
    f.capability = false;
    assert.equal((await f.inspect()).status, "unavailable");
    released(f.captured.at(-1));
    f.capability = true;
    f.alter = (response) => {
      response.publicInventory.declarations = []; response.publicInventory.roots = [];
      response.publicInventory.fragment.entries = []; return response;
    };
    assert.equal((await f.inspect()).status, "export-admitted");
    for (const snapshot of f.captured) released(snapshot);
  } finally { await f.dispose(); }
});

test("lifecycle diagnostic text budget evicts older projects before the project-count limit", async () => {
  const f = await fixture();
  try {
    f.alter = (response) => {
      rejected(response);
      const diagnostic = response.publicInventory.exportDiagnostics[0];
      response.publicInventory.exportDiagnostics = Array.from({ length: 100 }, () => ({ ...diagnostic, message: "x".repeat(16384) }));
      return response;
    };
    for (let i = 0; i < 3; i += 1) {
      const project = await f.project(`diagnostic-bytes-${i}`);
      f.vscode.window.activeTextEditor = { document: project.instanceDoc };
      assert.equal((await f.inspect()).status, "export-rejected");
      f.picks.at(-1).hide();
    }
    const diagnostics = f.collections.get("abstract-public");
    assert.equal(diagnostics.size, 2, "three 1.6 MiB diagnostic records must not exceed the 4 MiB retained-text cap");
    assert.ok([...diagnostics.keys()].every((key) => !key.includes(`diagnostic-bytes-0${path.sep}`)));
    for (const snapshot of f.captured) released(snapshot);
  } finally { await f.dispose(); }
});

test("lifecycle replacement during deferred navigation cancels the old display and waits before another capture", async () => {
  const f = await fixture();
  try {
    assert.equal((await f.inspect()).status, "export-admitted");
    const first = f.captured[0];
    let entered = false;
    let resume;
    f.vscode.workspace.openTextDocument = () => { entered = true; return new Promise((resolve) => { resume = resolve; }); };
    f.picks[0].accept();
    await until(() => entered);
    f.vscode.window.activeTextEditor = { document: f.two.instanceDoc };
    const replacement = f.inspect();
    assert.ok(first.sources.size > 0, "an in-flight navigation retains its snapshot until unwind");
    await new Promise((resolve) => setImmediate(resolve));
    assert.equal(f.captured.length, 1, "the replacement must not allocate another snapshot before cancellation unwinds");
    resume(f.one.schemaDoc);
    assert.equal((await replacement).status, "export-admitted");
    released(first);
    assert.equal(f.shown.length, 0, "superseded navigation never invokes the editor display API");
    f.vscode.workspace.openTextDocument = async () => f.two.schemaDoc;
    f.picks.at(-1).accept();
    await until(() => f.shown.length === 1);
    assert.equal(f.shown[0].document, f.two.schemaDoc);
    await until(() => !f.captured.at(-1).sources);
    released(f.captured.at(-1));
  } finally { await f.dispose(); }
});

test("lifecycle closing a picker during deferred navigation prevents display and then releases the snapshot", async () => {
  const f = await fixture();
  try {
    await f.inspect();
    const current = f.captured[0];
    let entered = false;
    let resume;
    f.vscode.workspace.openTextDocument = () => { entered = true; return new Promise((resolve) => { resume = resolve; }); };
    f.picks[0].accept();
    await until(() => entered);
    f.picks[0].hide();
    assert.ok(current.sources.size > 0);
    resume(f.one.schemaDoc);
    await until(() => !current.navigating);
    assert.equal(f.shown.length, 0);
    released(current);
  } finally { await f.dispose(); }
});
