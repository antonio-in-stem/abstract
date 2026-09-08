const vscode = require("vscode");
const fs = require("fs/promises");
const path = require("path");
const protocol = require("./analysis-client");
const bindings = require("./schema-bindings");
const resources = require("./schema-resources");
const { discoveryRoot, discover, isSource } = require("./project-index");

// IPO: capture source versions -> request compiler identities -> validate the
// same snapshot and proposed overlays -> return locations or a WorkspaceEdit.
// No disk writes and no fallback to the tolerant completion/navigation index.
function registerSchemaFeatures(context, resolveProjectPath, report, workspaceRoot = () => undefined) {
  let revision = 0; let requestId = 0; let disposed = false;
  const operations = new Set(); const prepared = new Map();
  const changed = () => { revision += 1; for (const operation of operations) operation.cancel(); };
  const watcher = vscode.workspace.createFileSystemWatcher("**/*");
  context.subscriptions.push(watcher, watcher.onDidCreate(changed), watcher.onDidChange(changed), watcher.onDidDelete(changed),
    vscode.workspace.onDidChangeTextDocument(({ document }) => { if (isSource(document.uri.fsPath)) changed(); }),
    vscode.workspace.onDidOpenTextDocument((document) => { if (isSource(document.uri.fsPath)) changed(); }),
    vscode.workspace.onDidCloseTextDocument((document) => { if (isSource(document.uri.fsPath)) changed(); }),
    vscode.workspace.onDidChangeConfiguration((event) => { if (event.affectsConfiguration("abstract")) changed(); }),
    { dispose() { disposed = true; changed(); prepared.clear(); } });

  async function canonical(file) {
    try { return await fs.realpath(file); }
    catch { return path.join(await fs.realpath(path.dirname(file)), path.basename(file)); }
  }
  function check(state) {
    if (disposed || !vscode.workspace.isTrusted || state.revision !== revision || state.token?.isCancellationRequested
      || state.documents.some(({ document, version }) => document.isClosed || document.version !== version)) {
      throw new Error("Schema operation cancelled because the project snapshot changed.");
    }
  }
  async function execute(state, args, input, timeout = 30000) {
    check(state);
    const operation = protocol.startProcess(state.compiler, args, { cwd: state.cwd, input, timeout });
    operations.add(operation);
    const subscription = state.token?.onCancellationRequested(() => operation.cancel());
    try {
      const result = await operation.promise;
      check(state);
      if (result.cancelled) throw new Error("Schema operation cancelled.");
      return result;
    } finally { subscription?.dispose(); operations.delete(operation); }
  }
  async function capture(document, token) {
    if (!vscode.workspace.isTrusted) throw new Error("Compiler schema references and rename require Workspace Trust.");
    const target = resolveProjectPath(document);
    if (!target) throw new Error("Open this source in an Abstract project before requesting schema references or rename.");
    const state = { revision, token, documents: [], root: await fs.realpath(target), sources: new Map() };
    state.cwd = workspaceRoot(document) || state.root;
    state.compiler = vscode.workspace.getConfiguration("abstract", document.uri).get("compilerPath", "abstract");
    state.limits = { ...resources.LIMITS, check: () => check(state) };
    state.discovery = await discoveryRoot(state.root, state.limits);
    state.diskFiles = await discover(state.discovery, state.limits);
    const admission = resources.budget();
    const diskSet = new Set(state.diskFiles.map(bindings.key));
    for (const open of vscode.workspace.textDocuments.filter((d) => d.uri.scheme === "file" && isSource(d.uri.fsPath))) {
      const file = await canonical(open.uri.fsPath);
      const relative = path.relative(state.discovery, file);
      const inside = relative && relative !== ".." && !relative.startsWith(`..${path.sep}`) && !path.isAbsolute(relative)
        && !relative.split(path.sep).slice(0, -1).some((part) => part.startsWith(".") || ["node_modules", "target", "build", "out"].includes(part.toLowerCase()));
      if (!diskSet.has(bindings.key(file)) && !inside) continue;
      state.documents.push({ document: open, version: open.version, canonical: file });
      const previous = state.sources.get(bindings.key(file));
      if (!previous) resources.sourceCount(state.sources.size + 1);
      const openBudget = previous ? resources.budget() : admission;
      // lineCount is cheap in the host. Bound it before offsetAt, which may
      // construct a line-offset index; neither call joins the complete text.
      resources.admit(open.lineCount - 1, openBudget);
      resources.admit(open.offsetAt(new vscode.Position(open.lineCount, 0)), openBudget);
      const text = open.getText();
      if (previous && previous.text !== text) throw new Error("Two editor aliases contain conflicting source text.");
      const bytes = previous?.bytes ?? resources.textBytes(text, admission);
      const aliases = previous?.aliases || new Set();
      aliases.add(open.uri.toString());
      state.sources.set(bindings.key(file), { path: file, text, bytes, overlay: previous?.overlay || open.isDirty || !diskSet.has(bindings.key(file)), uri: open.uri, aliases });
    }
    for (const file of state.diskFiles) if (!state.sources.has(bindings.key(file))) {
      resources.sourceCount(state.sources.size + 1);
      const { text, bytes } = await resources.readSource(file, admission, () => check(state));
      state.sources.set(bindings.key(file), { path: file, text, bytes, overlay: false, uri: vscode.Uri.file(file) });
    }
    state.file = await canonical(document.uri.fsPath);
    state.texts = new Map([...state.sources].map(([file, source]) => [file, source.text]));
    check(state);
    // Revalidate saved files before even capability negotiation: open editors
    // can hold old text while their backing files have grown outside watchers.
    await current(state);
    const capability = protocol.capability(await execute(state, ["analyze", "--capabilities"], undefined, 5000));
    if (!capability?.schemaBindings) throw new Error("This compiler does not expose schema bindings v1. Update the compiler to use schema references and rename.");
    state.graph = await analyze(state);
    await current(state);
    return state;
  }
  async function analyze(state, replacements = new Map()) {
    const admission = resources.budget();
    for (const [file, source] of state.sources) resources.textBytes(replacements.get(file) ?? source.text, admission);
    const overlays = [...state.sources].filter(([file, source]) => source.overlay || replacements.has(file))
      .map(([file, source]) => ({ path: source.path, text: replacements.get(file) ?? source.text }));
    const id = requestId = (requestId + 1) >>> 0;
    const result = await execute(state, ["analyze", state.root, "--stdio", "--symbols"], protocol.encodeRequest(id, overlays));
    const graph = bindings.readBindings(protocol.response(result, id));
    const expected = new Map(state.texts);
    for (const [file, text] of replacements) expected.set(file, text);
    bindings.validateSnapshot(graph, expected);
    return graph;
  }
  async function current(state) {
    check(state);
    for (const { document, canonical: expected } of state.documents) {
      if (bindings.key(await canonical(document.uri.fsPath)) !== bindings.key(expected)) {
        throw new Error("An editor source alias changed its canonical target during schema analysis.");
      }
    }
    const files = await discover(state.discovery, state.limits);
    if (files.length !== state.diskFiles.length || files.some((file, i) => bindings.key(file) !== bindings.key(state.diskFiles[i]))) throw new Error("Project membership changed during schema analysis.");
    // File watchers are advisory. Re-read disk-backed bytes even when a file
    // lies outside the workspace via a discovered junction.
    const admission = resources.budget();
    for (const source of state.sources.values()) {
      if (source.overlay) resources.textBytes(source.text, admission);
      else {
        const { text } = await resources.readSource(source.path, admission, () => check(state));
        if (text !== source.text) throw new Error("A saved source changed during schema analysis.");
      }
    }
    check(state);
  }
  const range = (entry) => new vscode.Range(entry.range.start.line, entry.range.start.character, entry.range.end.line, entry.range.end.character);
  const fingerprint = (state) => state.graph.sources.map((source) => `${bindings.key(source.path)}:${source.sha256}`).sort().join("\n");
  const selected = (state, position) => {
    const match = bindings.symbolAt(state.graph, state.file, position);
    if (!match) throw new Error("This position is not a schema symbol. Fields, instance IDs and loop variables are not supported by this increment.");
    return match;
  };
  const selector = { language: "abstract", scheme: "file" };
  context.subscriptions.push(
    vscode.languages.registerReferenceProvider(selector, {
      async provideReferences(document, position, options, token) {
        try {
          const state = await capture(document, token);
          const match = bindings.symbolAt(state.graph, state.file, position);
          if (!match) return [];
          return match.symbol.occurrences.filter((entry) => options.includeDeclaration || entry.role !== "declaration")
            .map((entry) => new vscode.Location(state.sources.get(bindings.key(entry.path)).uri, range(entry)));
        } catch (error) { report(`Schema references: ${error.message}`); throw error; }
      }
    }),
    vscode.languages.registerRenameProvider(selector, {
      async prepareRename(document, position, token) {
        const state = await capture(document, token);
        const match = selected(state, position);
        prepared.set(document.uri.toString(), { fingerprint: fingerprint(state), version: document.version, name: match.symbol.name });
        return { range: range(match.occurrence), placeholder: match.symbol.name };
      },
      async provideRenameEdits(document, position, newName, token) {
        const preparation = prepared.get(document.uri.toString());
        prepared.delete(document.uri.toString());
        const state = await capture(document, token);
        const match = selected(state, position);
        if (preparation && (preparation.version !== document.version || preparation.fingerprint !== fingerprint(state) || preparation.name !== match.symbol.name)) {
          throw new Error("The project changed after rename preparation; start Rename again.");
        }
        // Reject expansion before constructing candidate strings. Names and
        // bound occurrences are ASCII; byte growth is exact for the edits.
        if (!bindings.validName(newName)) throw new Error("A schema name must start with an ASCII letter and contain only letters, digits and underscores.");
        const counts = new Map();
        for (const entry of match.symbol.occurrences) {
          const file = bindings.key(entry.path); counts.set(file, (counts.get(file) || 0) + 1);
        }
        const candidateBudget = resources.budget();
        for (const [file, source] of state.sources) resources.bytes(source.bytes + (newName.length - match.symbol.name.length) * (counts.get(file) || 0), candidateBudget);
        const replacements = bindings.rename(state.graph, match.symbol, newName, state.texts);
        for (const file of replacements.keys()) {
          if (state.sources.get(file).aliases?.size > 1) {
            throw new Error("Close duplicate editor aliases of the same source before Rename; all open buffers must share one editing identity.");
          }
          const relative = path.relative(state.discovery, state.sources.get(file).path);
          if (!relative || relative === ".." || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) {
            throw new Error("Rename would edit a linked source outside this project's discovery root. Its other consumers cannot be resolved safely.");
          }
        }
        if (replacements.size) await analyze(state, replacements);
        // Public WorkspaceEdit cannot attach our snapshot versions to F2's
        // later application/preview; checks cover only provider return. Closed
        // sources are read again below; opening them adds no version guarantee.
        await current(state);
        const edit = new vscode.WorkspaceEdit();
        if (replacements.size) for (const entry of match.symbol.occurrences) {
          edit.replace(state.sources.get(bindings.key(entry.path)).uri, range(entry), newName);
        }
        return edit;
      }
    })
  );
}

module.exports = { registerSchemaFeatures };
