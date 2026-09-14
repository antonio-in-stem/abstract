const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("fs/promises");
const path = require("path");
const vm = require("vm");
const { createRequire } = require("module");
const protocol = require("../src/analysis-client");
const { temporaryWorkspace } = require("./test-workspace");
const compiler = process.env.ABSTRACT_COMPILER_PATH || path.resolve(__dirname, "../../../target/debug", process.platform === "win32" ? "abstract.exe" : "abstract");

async function setup(hook = () => {}) {
  const workspace = await temporaryWorkspace();
  const schemaText = "schema Item {\nvalue: int\n}\n";
  const text = "Item :: @id.x\nvalue: 1\n";
  const schemaFile = await workspace.write("data/model.abt", schemaText);
  const file = await workspace.write("data/item.ab", text);
  const callbacks = new Map(); const providers = {}; const calls = [];
  const event = (name) => (callback) => { callbacks.set(name, callback); return { dispose() {} }; };
  const uri = (file) => ({ fsPath: file, scheme: "file", toString() { return file; } });
  const document = (file, text) => ({ uri: uri(file), text, version: 1, isDirty: false, isClosed: false,
    get lineCount() { return this.text.split("\n").length; }, offsetAt() { return this.text.length; }, getText() { return this.text; } });
  const doc = document(file, text); const schema = document(schemaFile, schemaText);
  const documents = [doc, schema];
  const context = { subscriptions: [] };
  const vscode = {
    Uri: { file: uri }, Position: class { constructor(line, character) { this.line = line; this.character = character; } },
    Range: class { constructor(...values) { this.values = values; } },
    Location: class { constructor(uri, range) { this.uri = uri; this.range = range; } },
    MarkdownString: class { constructor(value = "") { this.value = value; } },
    Hover: class { constructor(contents) { this.contents = contents; } },
    WorkspaceEdit: class { constructor() { this.edits = []; } replace(...edit) { this.edits.push(edit); } },
    workspace: {
      isTrusted: true, textDocuments: documents,
      createFileSystemWatcher: () => ({ dispose() {}, onDidCreate: event("create"), onDidChange: event("disk"), onDidDelete: event("delete") }),
      onDidChangeTextDocument: event("text"), onDidCloseTextDocument: event("close"), onDidChangeConfiguration: event("config"),
      onDidOpenTextDocument: event("open"),
      getConfiguration: () => ({ get: () => compiler }),
      openTextDocument: async (uri) => documents.find((d) => d.uri.fsPath === uri.fsPath),
    },
    languages: {
      registerReferenceProvider: (_, provider) => { providers.references = provider; return { dispose() {} }; },
      registerHoverProvider: (_, provider) => { providers.hover = provider; return { dispose() {} }; },
      registerRenameProvider: (_, provider) => { providers.rename = provider; return { dispose() {} }; },
    },
  };
  const tokenCallbacks = [];
  const token = { isCancellationRequested: false, onCancellationRequested(callback) { tokenCallbacks.push(callback); return { dispose() {} }; } };
  const state = { workspace, schemaFile, file, doc, schema, vscode, providers, calls, token,
    change(target, value) { target.text = value; target.version += 1; target.isDirty = true; callbacks.get("text")({ document: target }); },
    open(target) { documents.push(target); callbacks.get("open")(target); },
    cancel() { token.isCancellationRequested = true; for (const callback of tokenCallbacks) callback(); },
    async dispose() { for (const item of context.subscriptions) item.dispose(); await workspace.dispose(); },
  };
  const providerPath = path.resolve(__dirname, "../src/schema-providers.js");
  const requireActual = createRequire(providerPath);
  const module = { exports: {} };
  const wrapped = { ...protocol, startProcess(executable, args, options) {
    calls.push(args);
    const operation = protocol.startProcess(executable, args, options);
    return { cancel: () => operation.cancel(), promise: operation.promise.then(async (result) => { await hook(state, args, result); return result; }) };
  } };
  vm.runInNewContext(await fs.readFile(providerPath, "utf8"), { module, exports: module.exports, TextDecoder,
    require: (name) => name === "vscode" ? vscode : name === "./analysis-client" ? wrapped : requireActual(name) }, { filename: providerPath });
  module.exports.registerSchemaFeatures(context, () => workspace.root, () => {});
  return state;
}

test("oversized sparse saved source is rejected before any compiler process", async () => {
  const f = await setup();
  try {
    const file = await f.workspace.write("data/oversized.ab", "");
    const handle = await fs.open(file, "r+");
    try { await handle.truncate(4 * 1024 * 1024 + 1); } finally { await handle.close(); }
    await assert.rejects(f.providers.rename.prepareRename(f.doc, { line: 0, character: 2 }, f.token), /Schema source exceeds/);
    assert.equal(f.calls.length, 0);
  } finally { await f.dispose(); }
});

test("oversized open documents reject before full-text materialization or unbounded offset lookup", async () => {
  for (const tooManyLines of [false, true]) {
    const f = await setup(); let gets = 0; let offsets = 0;
    try {
      Object.defineProperty(f.schema, "lineCount", { get: () => tooManyLines ? 4 * 1024 * 1024 + 2 : 1 });
      f.schema.offsetAt = (position) => {
        offsets += 1; assert.equal(position.line, 1); assert.equal(position.character, 0);
        return 4 * 1024 * 1024 + 1;
      };
      f.schema.getText = () => { gets += 1; throw new Error("full text must not be materialized"); };
      await assert.rejects(f.providers.rename.prepareRename(f.doc, { line: 0, character: 2 }, f.token), /per-source admission/);
      assert.equal(gets, 0); assert.equal(offsets, tooManyLines ? 0 : 1); assert.equal(f.calls.length, 0);
    } finally { await f.dispose(); }
  }
});

test("aggregate saved bytes and UTF-8 dirty buffers reject before compiler launch", async () => {
  const f = await setup();
  try {
    for (let i = 0; i < 4; i += 1) {
      const file = await f.workspace.write(`data/part${i}.ab`, "");
      const handle = await fs.open(file, "r+");
      try { await handle.truncate(4 * 1024 * 1024); } finally { await handle.close(); }
    }
    await assert.rejects(f.providers.rename.prepareRename(f.doc, { line: 0, character: 2 }, f.token), /aggregate source admission/);
    assert.equal(f.calls.length, 0);
  } finally { await f.dispose(); }
  const dirty = await setup();
  try {
    dirty.change(dirty.schema, "😀".repeat(1024 * 1024 + 1));
    await assert.rejects(dirty.providers.rename.prepareRename(dirty.doc, { line: 0, character: 2 }, dirty.token), /per-source admission/);
    assert.equal(dirty.calls.length, 0);
  } finally { await dirty.dispose(); }
});

test("discovery rejects the 1,025th source, including new unsaved sources", async () => {
  const f = await setup();
  try {
    for (let start = 0; start < 1022; start += 32) {
      await Promise.all(Array.from({ length: Math.min(32, 1022 - start) }, (_, i) => f.workspace.write(`data/empty${start + i}.ab`, "")));
    }
    const extra = { ...f.doc, text: "", isDirty: true, uri: { scheme: "file", fsPath: path.join(f.workspace.root, "data/new.ab"), toString() { return this.fsPath; } } };
    f.open(extra);
    await assert.rejects(f.providers.rename.prepareRename(f.doc, { line: 0, character: 2 }, f.token), /1,024-source/);
    assert.equal(f.calls.length, 0);
    await fs.writeFile(extra.uri.fsPath, "");
    await assert.rejects(f.providers.rename.prepareRename(f.doc, { line: 0, character: 2 }, f.token), /1,024-source/);
    assert.equal(f.calls.length, 0);
  } finally { await f.dispose(); }
});

test("saved-file growth during analysis and candidate expansion reject without candidate compilation", async () => {
  let grown = false;
  const f = await setup(async (state, args) => {
    if (args.includes("--symbols") && !grown) {
      grown = true; const handle = await fs.open(state.schemaFile, "r+");
      try { await handle.truncate(4 * 1024 * 1024 + 1); } finally { await handle.close(); }
    }
  });
  try {
    await assert.rejects(f.providers.rename.provideRenameEdits(f.doc, { line: 0, character: 2 }, "Entry", f.token), /per-source admission/);
    assert.equal(f.calls.filter((args) => args.includes("--symbols")).length, 1);
  } finally { await f.dispose(); }
  const rename = await setup();
  try {
    await assert.rejects(rename.providers.rename.provideRenameEdits(rename.doc, { line: 0, character: 2 }, "A".repeat(4 * 1024 * 1024), rename.token), /per-source admission/);
    assert.equal(rename.calls.filter((args) => args.includes("--symbols")).length, 1);
  } finally { await rename.dispose(); }
});

test("provider excludes declaration on request and revalidates proposed rename with the real compiler", async () => {
  const f = await setup();
  try {
    const refs = await f.providers.references.provideReferences(f.doc, { line: 0, character: 2 }, { includeDeclaration: false }, f.token);
    assert.equal(refs.length, 1); assert.equal(refs[0].uri.fsPath, f.file);
    const edit = await f.providers.rename.provideRenameEdits(f.doc, { line: 0, character: 2 }, "Entry", f.token);
    assert.equal(edit.edits.length, 2);
    assert.equal(f.calls.filter((args) => args.includes("--symbols")).length, 3); // refs + before + candidate
    assert.equal(f.doc.text, "Item :: @id.x\nvalue: 1\n");
  } finally { await f.dispose(); }
});

test("document edits while the compiler result is pending cannot produce a WorkspaceEdit", async () => {
  let changed = false;
  const f = await setup((state, args) => {
    if (args.includes("--symbols") && !changed) { changed = true; state.change(state.doc, state.doc.text + "\n"); }
  });
  try { await assert.rejects(f.providers.rename.provideRenameEdits(f.doc, { line: 0, character: 2 }, "Entry", f.token), /snapshot changed/); }
  finally { await f.dispose(); }
});

test("closed-loop cancellation and untrusted workspaces refuse edits; trust gate starts no compiler", async () => {
  const f = await setup((state, args) => { if (args.includes("--symbols")) state.cancel(); });
  try { await assert.rejects(f.providers.rename.provideRenameEdits(f.doc, { line: 0, character: 2 }, "Entry", f.token), /cancelled/); }
  finally { await f.dispose(); }
  const restricted = await setup();
  try {
    restricted.vscode.workspace.isTrusted = false;
    await assert.rejects(restricted.providers.rename.prepareRename(restricted.doc, { line: 0, character: 2 }, restricted.token), /Workspace Trust/);
    assert.equal(restricted.calls.length, 0);
  } finally { await restricted.dispose(); }
});

test("stale rename preparation and unwatched disk changes are rejected independently of editor events", async () => {
  const f = await setup();
  try {
    await f.providers.rename.prepareRename(f.doc, { line: 0, character: 2 }, f.token);
    f.change(f.schema, f.schema.text + "// changed\n");
    await assert.rejects(f.providers.rename.provideRenameEdits(f.doc, { line: 0, character: 2 }, "Entry", f.token), /after rename preparation/);
  } finally { await f.dispose(); }
  let changed = false;
  const disk = await setup(async (state, args) => {
    if (args.includes("--symbols") && !changed) { changed = true; await fs.appendFile(state.schemaFile, "// external disk change\n"); }
  });
  try { await assert.rejects(disk.providers.rename.provideRenameEdits(disk.doc, { line: 0, character: 2 }, "Entry", disk.token), /saved source changed/); }
  finally { await disk.dispose(); }
});

test("compiler diagnostic-only capability cannot enable semantic rename", async () => {
  const f = await setup((_state, args, result) => {
    if (args.includes("--capabilities")) {
      const value = JSON.parse(result.stdout); delete value.schemaBindings; result.stdout = JSON.stringify(value);
    }
  });
  try {
    await assert.rejects(f.providers.rename.prepareRename(f.doc, { line: 0, character: 2 }, f.token), /does not expose semantic bindings/);
    assert.equal(f.calls.length, 1);
  } finally { await f.dispose(); }
});

test("linked schemas outside discovery root allow contextual references but reject partial external rename", async () => {
  const f = await setup();
  try {
    const shared = await f.workspace.write("shared/model.abt", f.schema.text);
    const before = await fs.readFile(shared, "utf8");
    await fs.writeFile(f.schemaFile, ""); f.schema.text = "";
    await fs.symlink(path.join(f.workspace.root, "shared"), path.join(f.workspace.root, "data/shared"), process.platform === "win32" ? "junction" : "dir");
    const references = await f.providers.references.provideReferences(f.doc, { line: 0, character: 2 }, { includeDeclaration: true }, f.token);
    assert.equal(references.length, 2);
    assert.ok(references.some((entry) => entry.uri.fsPath === shared));
    await assert.rejects(f.providers.rename.provideRenameEdits(f.doc, { line: 0, character: 2 }, "Entry", f.token), /outside this project's discovery root/);
    assert.equal(await fs.readFile(shared, "utf8"), before);
  } finally { await f.dispose(); }
});

test("multiple identical open aliases reject Rename instead of editing only one buffer", async () => {
  const f = await setup();
  try {
    const aliasRoot = path.join(f.workspace.root, "aliases");
    await fs.symlink(path.join(f.workspace.root, "data"), aliasRoot, process.platform === "win32" ? "junction" : "dir");
    const alias = { ...f.schema, uri: { scheme: "file", fsPath: path.join(aliasRoot, "model.abt"), toString() { return this.fsPath; } } };
    f.vscode.workspace.textDocuments.push(alias);
    const references = await f.providers.references.provideReferences(f.doc, { line: 0, character: 2 }, { includeDeclaration: true }, f.token);
    assert.equal(references.length, 2);
    await assert.rejects(f.providers.rename.provideRenameEdits(f.doc, { line: 0, character: 2 }, "Entry", f.token), /duplicate editor aliases/);
    assert.equal(f.schema.text, alias.text);
  } finally { await f.dispose(); }
});

test("retargeting an open alias outside watcher coverage cannot redirect a pending rename", async () => {
  let remapped = false;
  const f = await setup(async (state, args) => {
    if (args.includes("--symbols") && !remapped) {
      remapped = true;
      await fs.unlink(path.join(state.workspace.root, "aliases"));
      await fs.symlink(path.join(state.workspace.root, "elsewhere"), path.join(state.workspace.root, "aliases"), process.platform === "win32" ? "junction" : "dir");
    }
  });
  try {
    const original = await fs.readFile(f.schemaFile, "utf8");
    const other = await f.workspace.write("elsewhere/model.abt", "schema Other {\n}\n");
    const aliasRoot = path.join(f.workspace.root, "aliases");
    await fs.symlink(path.join(f.workspace.root, "data"), aliasRoot, process.platform === "win32" ? "junction" : "dir");
    f.schema.uri = { scheme: "file", fsPath: path.join(aliasRoot, "model.abt"), toString() { return this.fsPath; } };
    await assert.rejects(f.providers.rename.provideRenameEdits(f.doc, { line: 0, character: 2 }, "Entry", f.token), /alias changed its canonical target/);
    assert.equal(await fs.readFile(f.schemaFile, "utf8"), original);
    assert.equal(await fs.readFile(other, "utf8"), "schema Other {\n}\n");
  } finally { await f.dispose(); }
});

test("a new editor alias opened while analysis is pending invalidates the editing snapshot", async () => {
  let opened = false;
  const f = await setup((state, args) => {
    if (args.includes("--symbols") && !opened) {
      opened = true;
      state.open({ ...state.schema, uri: { scheme: "file", fsPath: path.join(state.workspace.root, "aliases/model.abt"), toString() { return this.fsPath; } } });
    }
  });
  try {
    await fs.symlink(path.join(f.workspace.root, "data"), path.join(f.workspace.root, "aliases"), process.platform === "win32" ? "junction" : "dir");
    await assert.rejects(f.providers.rename.provideRenameEdits(f.doc, { line: 0, character: 2 }, "Entry", f.token), /snapshot changed/);
  } finally { await f.dispose(); }
});
