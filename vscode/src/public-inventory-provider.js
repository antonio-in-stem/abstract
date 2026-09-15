const vscode = require("vscode");
const path = require("path");
const protocol = require("./analysis-client");
const bindings = require("./schema-bindings");
const model = require("./public-inventory");
const { isSource } = require("./project-index");
const { createSourceCapture } = require("./source-capture");
const resources = require("./schema-resources");

// Explicit invocation only. The command resolves when the native picker is
// ready; its event handlers own subsequent revalidation/navigation/disposal.
function registerPublicInventory(context, resolveProjectPath, report, workspaceRoot = () => undefined) {
  const diagnostics = vscode.languages.createDiagnosticCollection("abstract-public");
  // Only diagnostic text and source identities survive picker closure. Bound
  // even that lightweight history; eviction removes old diagnostics, not files.
  const CACHE_PROJECTS = 16;
  const CACHE_BYTES = 4 * 1024 * 1024;
  const projects = new Map();
  let cachedBytes = 0;
  let active;
  let nextId = 0;
  let invocation = 0;
  let disposed = false;
  const notify = (message, warning = false) => {
    const notification = warning ? vscode.window.showWarningMessage(message) : vscode.window.showInformationMessage(message);
    notification?.then(undefined, () => {});
  };
  const range = (value) => new vscode.Range(value.start.line, value.start.character, value.end.line, value.end.character);
  const inside = (root, file) => {
    if (!root) return false;
    const relative = path.relative(root, file);
    return relative !== ".." && !relative.startsWith(`..${path.sep}`) && !path.isAbsolute(relative);
  };
  function refreshDiagnostics() {
    const all = new Map();
    for (const state of projects.values()) for (const entry of state.diagnostics || []) {
      const id = entry.uri.toString();
      const target = all.get(id) || { uri: entry.uri, diagnostics: [] };
      target.diagnostics.push(entry.diagnostic); all.set(id, target);
    }
    diagnostics.clear();
    for (const value of all.values()) diagnostics.set(value.uri, value.diagnostics);
  }
  function removeRecord(root, expected) {
    const record = projects.get(root);
    if (!record || expected && record !== expected) return;
    projects.delete(root);
    cachedBytes -= record.bytes;
  }
  function release(state) {
    if (!state || state.inFlight || state.picker || state.released) return;
    state.released = true;
    if (active === state) active = undefined;
    state.sources?.clear();
    state.texts?.clear();
    if (state.documents) state.documents.length = 0;
    for (const field of ["sources", "texts", "documents", "document", "diskFiles", "limits", "record",
      "pickerSubscriptions", "opening", "file", "compiler", "cwd", "requestedRoot"]) delete state[field];
  }
  function begin(state) {
    state.inFlight = true;
    state.idle = new Promise((resolve) => { state.finish = resolve; });
  }
  function finish(state) {
    state.inFlight = false;
    state.finish();
    delete state.finish;
    release(state);
  }
  function closePicker(state) {
    if (state?.picker) {
      const picker = state.picker;
      state.picker = undefined;
      for (const subscription of state.pickerSubscriptions || []) subscription.dispose();
      state.pickerSubscriptions = [];
      picker.items = [];
      picker.activeItems = [];
      picker.dispose();
    }
    release(state);
  }
  function invalidate(state, reason, discardDiagnostics = true) {
    if (!state) return;
    if (discardDiagnostics && state.record) removeRecord(bindings.key(state.root), state.record);
    if (state.cancelled) return;
    state.cancelled = true;
    state.operation?.cancel();
    const wasVisible = !!state.picker;
    closePicker(state);
    if (wasVisible && !disposed) report(`Public fields: ${reason} Run Inspect Public Fields again.`);
  }
  function affected(state, uri) {
    if (!uri || uri.scheme !== "file") return false;
    const file = bindings.key(uri.fsPath);
    return inside(state.discovery || state.root, uri.fsPath) || state.identities?.has(file)
      || state.identities?.has(uri.toString()) || state.sources?.has(file)
      || [...(state.sources?.values() || [])].some((s) => s.aliases?.has(uri.toString()));
  }
  function changed(uri) {
    for (const [root, record] of projects) if (!uri || affected(record, uri)) removeRecord(root, record);
    if (active && (!uri || affected(active, uri))) invalidate(active, "The project snapshot changed.");
    refreshDiagnostics();
  }
  function check(state) {
    if (disposed || state.cancelled || state.released || active !== state || !vscode.workspace.isTrusted
      || state.documents?.some(({ document, version }) => document.isClosed || document.version !== version)) {
      throw new Error("Public-field analysis cancelled because the project snapshot changed.");
    }
  }
  const capture = createSourceCapture(vscode, resolveProjectPath, workspaceRoot, check);
  async function execute(state, args, input, timeout = 30000) {
    check(state);
    state.operation = protocol.startProcess(state.compiler, args, { cwd: state.cwd, input, timeout });
    try {
      const result = await state.operation.promise;
      check(state);
      if (result.cancelled) throw new Error("Public-field analysis cancelled.");
      if (Buffer.byteLength(result.stdout || "", "utf8") > protocol.LIMITS.response) throw new Error("Public inventory exceeds the 4 MiB analysis response limit.");
      return result;
    } finally { state.operation = undefined; }
  }
  function publish(state, inventory) {
    const uriFor = (location) => state.sources.get(bindings.key(location.path)).uri;
    const entries = inventory.exportDiagnostics.map((item) => {
      const uri = item.path ? uriFor(item) : state.document.uri;
      const diagnostic = new vscode.Diagnostic(item.range ? range(item.range) : new vscode.Range(0, 0, 0, 0), item.message, vscode.DiagnosticSeverity.Error);
      diagnostic.code = item.code;
      diagnostic.source = "abstract-public";
      for (const note of item.notes) {
        if (note.path && note.range) {
          diagnostic.relatedInformation = [...(diagnostic.relatedInformation || []),
            new vscode.DiagnosticRelatedInformation(new vscode.Location(uriFor(note), range(note.range)), note.message)];
        } else diagnostic.message += `\nnote: ${note.message}`;
      }
      return { uri, diagnostic };
    });
    const root = bindings.key(state.root);
    removeRecord(root);
    if (entries.length) {
      const record = { root: state.root, discovery: state.discovery, identities: new Set(), diagnostics: entries, bytes: 0 };
      const charge = (text) => {
        record.bytes += Buffer.byteLength(text, "utf8");
        if (record.bytes > CACHE_BYTES) throw new Error("Public diagnostic identities/text exceed the 4 MiB retained-result limit.");
      };
      for (const source of state.sources.values()) for (const id of [bindings.key(source.path), ...(source.aliases || [])]) {
        if (!record.identities.has(id)) { charge(id); record.identities.add(id); }
      }
      for (const { uri, diagnostic } of entries) {
        charge(uri.toString()); charge(diagnostic.message);
        for (const note of diagnostic.relatedInformation || []) { charge(note.message); charge(note.location.uri.toString()); }
      }
      while (projects.size >= CACHE_PROJECTS || cachedBytes + record.bytes > CACHE_BYTES) {
        removeRecord(projects.keys().next().value);
      }
      projects.set(root, record);
      cachedBytes += record.bytes;
      state.record = record;
    }
    refreshDiagnostics();
  }
  async function navigate(state, item) {
    if (state.navigating || state.inFlight || state.cancelled || state.released) return;
    state.navigating = true;
    begin(state);
    try {
      // Watchers are advisory: aliases, every disk source, membership and all
      // captured editor versions are checked again after the picker was open.
      await capture.current(state);
      const source = state.sources.get(bindings.key(item.location.path));
      if (!source || bindings.key(await capture.canonical(source.uri.fsPath)) !== bindings.key(source.path)) {
        throw new Error("The selected source alias changed its canonical target.");
      }
      state.opening = source.uri.toString();
      const document = await vscode.workspace.openTextDocument(source.uri);
      state.opening = undefined;
      check(state);
      const admission = resources.budget();
      resources.admit(document.lineCount - 1, admission);
      resources.admit(document.offsetAt(new vscode.Position(document.lineCount, 0)), admission);
      const text = document.getText();
      resources.textBytes(text, admission);
      if (text !== source.text) throw new Error("The selected editor buffer differs from the captured source.");
      if (!state.documents.some((entry) => entry.document === document)) {
        state.documents.push({ document, version: document.version, canonical: source.path });
      }
      await capture.current(state);
      check(state);
      closePicker(state);
      await vscode.window.showTextDocument(document, { preview: true, selection: range(item.location.range) });
      check(state);
    } catch (error) {
      invalidate(state, "Navigation was cancelled.");
      refreshDiagnostics();
      if (!disposed) { report(error.message); notify(error.message, true); }
    } finally { state.opening = undefined; state.navigating = false; finish(state); }
  }
  async function inspect() {
    const document = vscode.window.activeTextEditor?.document;
    if (!document || document.languageId !== "abstract" || document.uri.scheme !== "file") {
      notify("Open an Abstract source file to inspect public fields.");
      return { status: "unavailable", declarations: 0, targets: 0, rows: 0 };
    }
    const request = ++invocation;
    const previous = active;
    invalidate(previous, "A newer inspection replaced this result.", false);
    // A cancelled capture/navigation owns its buffers until its awaits unwind.
    // Wait before admitting another heavy snapshot; superseded waiters do no work.
    await previous?.idle;
    if (disposed || request !== invocation) return { status: "unavailable", declarations: 0, targets: 0, rows: 0 };
    refreshDiagnostics();
    const state = { document, documents: [], root: resolveProjectPath(document) };
    active = state;
    begin(state);
    try {
      await capture.capture(document, state);
      removeRecord(bindings.key(state.root));
      refreshDiagnostics();
      const capability = protocol.capability(await execute(state, ["analyze", "--capabilities"], undefined, 5000));
      if (!capability?.supported || !capability.publicInventory || typeof capability.compiler !== "string") {
        throw new Error("This compiler does not expose public inventory v1. Ordinary diagnostics remain available; no saved-file fallback was used.");
      }
      const id = nextId = (nextId + 1) >>> 0;
      const overlays = [...state.sources.values()].filter((source) => source.overlay)
        .map(({ path, text }) => ({ path, text }));
      const response = protocol.response(await execute(state, ["analyze", state.root, "--stdio", "--public"], protocol.encodeRequest(id, overlays)), id);
      if (response?.compiler !== capability.compiler) throw new Error("Public inventory producer differs from the negotiated compiler.");
      const inventory = model.readInventory(response, state.texts);
      await capture.current(state);
      check(state);
      const rows = model.items(inventory);
      publish(state, inventory);
      state.ready = true;
      const summary = { status: inventory.status, declarations: inventory.declarations.length,
        targets: inventory.fragment?.entries.length || 0, rows: rows.length };
      if (inventory.status === "unavailable" || !rows.length) {
        notify(inventory.reason || (inventory.status === "unavailable"
          ? "Public inventory unavailable. Fix ordinary compiler diagnostics first." : "This project has no nominated public fields."));
        return summary;
      }
      const picker = vscode.window.createQuickPick();
      state.picker = picker;
      picker.title = "Abstract: Inspect Public Fields";
      picker.placeholder = inventory.status === "export-rejected"
        ? "Export rejected; nominations are not individually admitted. Select to navigate."
        : "Nominations and resolved versions; export admission is not runtime authorization.";
      picker.matchOnDescription = true;
      picker.matchOnDetail = true;
      picker.items = rows;
      picker.activeItems = [rows[0]];
      state.pickerSubscriptions = [picker.onDidAccept(() => {
        const selected = picker.selectedItems[0];
        if (selected && rows.includes(selected)) void navigate(state, selected);
      }), picker.onDidHide(() => invalidate(state, "The selection was closed.", false))];
      picker.show();
      report(`Public fields ready: ${summary.status}; ${summary.declarations} nominations, ${summary.targets} targets, ${summary.rows} rows.`);
      return summary;
    } catch (error) {
      invalidate(state, "Inspection was cancelled or unavailable.");
      refreshDiagnostics();
      if (!disposed) { report(`Public fields: ${error.message}`); notify(error.message); }
      return { status: "unavailable", declarations: 0, targets: 0, rows: 0 };
    } finally { finish(state); }
  }
  const watcher = vscode.workspace.createFileSystemWatcher("**/*");
  context.subscriptions.push(diagnostics, watcher,
    watcher.onDidCreate(changed), watcher.onDidChange(changed), watcher.onDidDelete(changed),
    vscode.workspace.onDidChangeTextDocument(({ document }) => { if (isSource(document.uri.fsPath)) changed(document.uri); }),
    vscode.workspace.onDidCloseTextDocument((document) => { if (isSource(document.uri.fsPath)) changed(document.uri); }),
    vscode.workspace.onDidOpenTextDocument((document) => {
      if (isSource(document.uri.fsPath) && active?.opening !== document.uri.toString()) changed(document.uri);
    }),
    vscode.workspace.onDidChangeConfiguration((event) => { if (event.affectsConfiguration("abstract")) changed(); }),
    vscode.commands.registerCommand("abstract.inspectPublicFields", inspect),
    { dispose() { disposed = true; invalidate(active, "Disposed."); projects.clear(); cachedBytes = 0; } });
}

module.exports = { registerPublicInventory };
