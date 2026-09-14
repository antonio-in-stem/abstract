const vscode = require("vscode");
const fs = require("fs");
const path = require("path");
const protocol = require("./analysis-client");
const { discoveryRoot, discover } = require("./project-index");

function registerDiagnostics(context, { diagnostics, resolveProjectPath, workspaceRoot, lintSaved, report }) {
  const states = new Map();
  const capabilities = new Map();
  const reportedFallbacks = new Set();
  let nextId = 0;
  let disposed = false;
  const status = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 25);
  status.name = "Abstract diagnostics";
  status.command = "abstract.showDiagnosticStatus";

  function rootOf(document) {
    const target = resolveProjectPath(document);
    if (!target) return "";
    let root;
    try { root = fs.realpathSync.native(target); } catch { root = path.resolve(target); }
    if (path.basename(root).toLowerCase() === "data") root = path.dirname(root);
    return root;
  }
  function activeStatus() {
    const document = vscode.window.activeTextEditor?.document;
    if (!document || document.languageId !== "abstract") { status.hide(); return; }
    const state = states.get(rootOf(document));
    status.text = !vscode.workspace.isTrusted ? "Abstract: Restricted Mode" : state?.label || "Abstract: No project";
    status.tooltip = state?.detail || "Compiler diagnostics for the current Abstract project.";
    status.show();
  }
  function label(state, text, detail) { state.label = text; state.detail = detail; activeStatus(); }
  function refresh(uri) {
    const items = [...states.values()].flatMap((state) => state.entries.get(uri.toString())?.items || []);
    if (items.length) diagnostics.set(uri, items); else diagnostics.delete(uri);
  }
  function publish(state, uri, items) {
    state.entries.set(uri.toString(), { uri, items });
    refresh(uri);
  }
  function cancel(state) {
    if (!state) return;
    clearTimeout(state.timer);
    state.operation?.cancel();
    const previous = [...state.entries.values()];
    state.entries.clear();
    for (const { uri } of previous) refresh(uri);
    state.resolve?.();
  }
  const same = (state) => !disposed && states.get(state.root) === state && vscode.workspace.isTrusted;
  const documentsFor = (root) => vscode.workspace.textDocuments.filter((d) => d.languageId === "abstract" && d.uri.scheme === "file" && rootOf(d) === root);
  function canonicalFile(file) { try { return fs.realpathSync.native(file); } catch { return file; } }
  async function hasUnsaved(document) {
    if (!vscode.workspace.textDocuments.some((d) => d.languageId === "abstract" && d.uri.scheme === "file" && d.isDirty)) return false;
    const root = rootOf(document);
    if (!root) return false;
    const members = new Set(await discover(await discoveryRoot(root)));
    return vscode.workspace.textDocuments.some((d) => d.languageId === "abstract" && d.uri.scheme === "file" && d.isDirty
      && (rootOf(d) === root || members.has(canonicalFile(d.uri.fsPath))));
  }
  function changed(document) {
    if (document.languageId !== "abstract" || document.uri.scheme !== "file") return;
    const ownRoot = rootOf(document);
    const canonical = canonicalFile(document.uri.fsPath);
    for (const state of [...states.values()]) {
      if (state.root !== ownRoot && state.members?.has(canonical)) schedule(state.document);
    }
    schedule(document);
  }
  function compilerKey(compiler, cwd) {
    const candidate = path.isAbsolute(compiler) ? compiler : compiler.includes(path.sep) ? path.resolve(cwd, compiler) : "";
    try { const stat = fs.statSync(candidate); return `${compiler}\0${cwd}\0${stat.mtimeMs}\0${stat.size}`; } catch { return `${compiler}\0${cwd}`; }
  }

  function schedule(document, immediate = false) {
    if (document.languageId !== "abstract" || document.uri.scheme !== "file") return Promise.resolve();
    const root = rootOf(document);
    if (!root) { activeStatus(); return Promise.resolve(); }
    const previous = states.get(root);
    cancel(previous);
    const state = { root, document, entries: new Map(), members: previous?.members, id: nextId = (nextId + 1) >>> 0 };
    states.set(root, state);
    if (!vscode.workspace.isTrusted) { activeStatus(); return Promise.resolve(); }
    label(state, "Abstract: Checking…", "Analyzing the current editor snapshot.");
    return new Promise((resolve) => {
      state.resolve = resolve;
      const run = () => analyze(state).catch((error) => {
        if (!same(state)) return;
        const diagnostic = new vscode.Diagnostic(new vscode.Range(0, 0, 0, 0), error.message, vscode.DiagnosticSeverity.Warning);
        diagnostic.source = "abstract-tooling";
        publish(state, document.uri, [diagnostic]);
        label(state, "Abstract: Analysis unavailable", error.message);
        report(error.message);
      }).finally(resolve);
      if (immediate) run(); else state.timer = setTimeout(run, 250);
    });
  }

  async function analyze(state) {
    if (!same(state)) return;
    const compiler = vscode.workspace.getConfiguration("abstract", state.document.uri).get("compilerPath", "abstract");
    const cwd = workspaceRoot(state.document);
    const key = compilerKey(compiler, cwd);
    let supported = capabilities.get(key);
    if (!supported) {
      state.operation = protocol.startProcess(compiler, ["analyze", "--capabilities"], { cwd, timeout: 5000 });
      supported = protocol.capability(await state.operation.promise);
      if (!same(state) || !supported) return;
      capabilities.set(key, supported);
    }
    // Include an open canonical source even when it is reached through a
    // junction and its editor URI lives outside this project's lexical root.
    // The compiler independently validates membership before applying bytes.
    state.members = new Set(await discover(await discoveryRoot(state.root)));
    const documents = vscode.workspace.textDocuments.filter((d) => d.languageId === "abstract" && d.uri.scheme === "file"
      && (rootOf(d) === state.root || state.members.has(canonicalFile(d.uri.fsPath))));
    if (!same(state)) return;
    if (!supported.supported) {
      label(state, "Abstract: Saved-file diagnostics", supported.reason);
      if (!reportedFallbacks.has(key)) { report(supported.reason); reportedFallbacks.add(key); }
      if (!documents.some((d) => d.isDirty)) {
        await lintSaved(state.document, { publish: (uri, items) => publish(state, uri, items), isCurrent: () => same(state),
          setOperation(operation) { state.operation = operation; if (!same(state)) operation.cancel(); } });
      }
      return;
    }
    const versions = new Map(documents.map((d) => [d.uri.toString(), d.version]));
    const paths = new Map();
    const overlays = [];
    for (const document of documents) {
      let canonical;
      canonical = canonicalFile(document.uri.fsPath);
      paths.set(path.normalize(canonical), document.uri);
      if (document.isDirty) overlays.push({ path: canonical, text: document.getText() });
    }
    const input = protocol.encodeRequest(state.id, overlays);
    if (!same(state)) return;
    state.operation = protocol.startProcess(compiler, ["analyze", state.root, "--stdio"], { cwd, input });
    const result = await state.operation.promise;
    if (!same(state) || documents.some((d) => d.version !== versions.get(d.uri.toString()))) return;
    const response = protocol.response(result, state.id);
    if (!response) return;
    const uriFor = (item) => item.path ? paths.get(path.normalize(item.path)) || vscode.Uri.file(item.path) : state.document.uri;
    const rangeFor = (item) => item.range ? new vscode.Range(item.range.start.line, item.range.start.character, item.range.end.line, item.range.end.character) : new vscode.Range(0, 0, 0, 0);
    const byUri = new Map();
    for (const item of response.diagnostics) {
      const uri = uriFor(item);
      const diagnostic = new vscode.Diagnostic(rangeFor(item), item.message, vscode.DiagnosticSeverity.Error);
      diagnostic.code = item.code; diagnostic.source = "abstract";
      for (const note of item.notes) {
        if (note.path && note.range) {
          diagnostic.relatedInformation = [...(diagnostic.relatedInformation || []), new vscode.DiagnosticRelatedInformation(new vscode.Location(uriFor(note), rangeFor(note)), note.message)];
        } else diagnostic.message += `\nnote: ${note.message}`;
      }
      const entry = byUri.get(uri.toString()) || { uri, items: [] };
      entry.items.push(diagnostic); byUri.set(uri.toString(), entry);
    }
    for (const { uri, items } of byUri.values()) publish(state, uri, items);
    const detail = response.truncated ? "Compiler diagnostics were truncated by protocol limits." : response.analyzed ? "Compiler analysis includes unsaved buffers and checks assets at the real project root." : "Project discovery failed before source analysis.";
    label(state, response.truncated ? "Abstract: Diagnostics truncated" : "Abstract: Live diagnostics", detail);
  }

  const watcher = vscode.workspace.createFileSystemWatcher("**/*");
  const changedOnDisk = (uri) => {
    for (const state of [...states.values()]) {
      const relative = path.relative(state.root, uri.fsPath);
      if (state.members?.has(canonicalFile(uri.fsPath)) || (relative && !relative.startsWith(`..${path.sep}`) && relative !== ".." && !path.isAbsolute(relative))) schedule(state.document);
    }
  };
  context.subscriptions.push(status, watcher,
    watcher.onDidCreate(changedOnDisk), watcher.onDidChange(changedOnDisk), watcher.onDidDelete(changedOnDisk),
    vscode.workspace.onDidOpenTextDocument(changed),
    vscode.workspace.onDidSaveTextDocument(changed),
    vscode.workspace.onDidChangeTextDocument(({ document, contentChanges }) => { if (contentChanges.length) changed(document); }),
    vscode.workspace.onDidCloseTextDocument((document) => {
      const root = rootOf(document);
      const canonical = canonicalFile(document.uri.fsPath);
      for (const state of [...states.values()]) if (state.root !== root && state.members?.has(canonical)) schedule(state.document);
      const remaining = documentsFor(root);
      if (remaining.length) schedule(remaining[0]);
      else { cancel(states.get(root)); states.delete(root); activeStatus(); }
    }),
    vscode.window.onDidChangeActiveTextEditor(activeStatus),
    vscode.workspace.onDidGrantWorkspaceTrust(() => { for (const document of vscode.workspace.textDocuments) schedule(document); }),
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (!e.affectsConfiguration("abstract")) return;
      for (const state of states.values()) cancel(state);
      states.clear(); capabilities.clear(); reportedFallbacks.clear();
      for (const document of vscode.workspace.textDocuments) schedule(document);
    }),
    { dispose() { disposed = true; for (const state of states.values()) cancel(state); states.clear(); } }
  );
  for (const document of vscode.workspace.textDocuments) schedule(document);
  function readiness(document) {
    if (document?.isUntitled && document.languageId === "abstract") {
      return { ready: false, label: "Abstract: Save required",
        detail: "Save this source inside an Abstract project to enable compiler diagnostics." };
    }
    if (!document || document.uri.scheme !== "file" || !/\.abt?$/i.test(document.uri.fsPath)) {
      return { ready: false, label: "Abstract: No source selected", detail: "Open an Abstract .ab or .abt source." };
    }
    if (document.languageId !== "abstract") return { ready: false, label: "Abstract: Language association conflict",
      detail: `VS Code opened this file as '${document.languageId}'. Run 'Abstract: Use for .ab and .abt in This Workspace'.` };
    if (!vscode.workspace.isTrusted) return { ready: false, label: "Abstract: Restricted Mode", detail: "Trust this workspace to run compiler diagnostics." };
    const root = rootOf(document);
    if (!root) return { ready: false, label: "Abstract: No project", detail: "Open a project with a data directory or configure abstract.projectPath." };
    const state = states.get(root);
    return { ready: Boolean(state && !/unavailable|No project|Restricted|Checking/.test(state.label || "")),
      label: state?.label || "Abstract: Checking…", detail: state?.detail || "Waiting for the compiler capability check." };
  }
  return { schedule, hasUnsaved, readiness };
}

module.exports = { registerDiagnostics };
