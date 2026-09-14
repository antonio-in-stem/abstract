const vscode = require("vscode");
const path = require("path");
const protocol = require("./analysis-client");
const bindings = require("./schema-bindings");
const resources = require("./schema-resources");
const model = require("./language-model");
const valueHover = require("./value-hover");
const { isSource } = require("./project-index");
const { createSourceCapture } = require("./source-capture");

// IPO: capture source versions -> request compiler identities -> validate the
// same snapshot and proposed overlays -> return locations or a WorkspaceEdit.
// No disk writes and no fallback to the tolerant completion/navigation index.
function registerSchemaFeatures(context, resolveProjectPath, report, workspaceRoot = () => undefined) {
  let revision = 0; let requestId = 0; let disposed = false;
  const operations = new Set(); const prepared = new Map(); const valueCache = new Map();
  const changed = () => { revision += 1; valueCache.clear(); for (const operation of operations) operation.cancel(); };
  const watcher = vscode.workspace.createFileSystemWatcher("**/*");
  context.subscriptions.push(watcher, watcher.onDidCreate(changed), watcher.onDidChange(changed), watcher.onDidDelete(changed),
    vscode.workspace.onDidChangeTextDocument(({ document }) => { if (isSource(document.uri.fsPath)) changed(); }),
    vscode.workspace.onDidOpenTextDocument((document) => { if (isSource(document.uri.fsPath)) changed(); }),
    vscode.workspace.onDidCloseTextDocument((document) => { if (isSource(document.uri.fsPath)) changed(); }),
    vscode.workspace.onDidChangeConfiguration((event) => { if (event.affectsConfiguration("abstract")) changed(); }),
    { dispose() { disposed = true; changed(); prepared.clear(); } });

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
  const sourceCapture = createSourceCapture(vscode, resolveProjectPath, workspaceRoot, check);
  const current = sourceCapture.current;
  async function capture(document, token) {
    const state = await sourceCapture.capture(document, { revision, token });
    const capability = protocol.capability(await execute(state, ["analyze", "--capabilities"], undefined, 5000));
    if (!capability?.schemaBindings) throw new Error("This compiler does not expose semantic bindings v1. Update the compiler to use references and rename.");
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
  async function values(document, position, token) {
    const localIndex = model.createIndex([{ file: document.uri.fsPath, text: document.getText() }]);
    const target = model.instanceFieldTarget(localIndex, document.uri.fsPath, document.offsetAt(position));
    if (!target) return undefined;
    const state = await sourceCapture.capture(document, { revision, token });
    const sourceFingerprint = [...state.texts].map(([file, text]) => `${file}:${bindings.hash(text)}`).sort().join("\n");
    const cached = valueCache.get(state.root);
    let compiled = cached?.revision === revision && cached.sourceFingerprint === sourceFingerprint ? cached.compiled : undefined;
    if (!compiled) {
      const capability = protocol.capability(await execute(state, ["analyze", "--capabilities"], undefined, 5000));
      if (!capability?.values) return undefined;
      const overlays = [...state.sources.values()].filter((source) => source.overlay)
        .map((source) => ({ path: source.path, text: source.text }));
      const id = requestId = (requestId + 1) >>> 0;
      const result = await execute(state, ["analyze", state.root, "--stdio", "--values"], protocol.encodeRequest(id, overlays));
      const response = protocol.response(result, id);
      if (!response.analyzed || response.truncated || response.diagnostics.length || !response.values?.complete) return undefined;
      compiled = valueHover.compiledValues(response);
      valueCache.set(state.root, { revision, sourceFingerprint, compiled });
    }
    await current(state);
    return valueHover.markdown(compiled, target.id, target.path);
  }
  const range = (entry) => new vscode.Range(entry.range.start.line, entry.range.start.character, entry.range.end.line, entry.range.end.character);
  const fingerprint = (state) => state.graph.sources.map((source) => `${bindings.key(source.path)}:${source.sha256}`).sort().join("\n");
  const selected = (state, position) => {
    const match = bindings.symbolAt(state.graph, state.file, position);
    if (!match) throw new Error("This position is not a compiler-bound schema, field, instance ID or loop variable.");
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
        } catch (error) { report(`Semantic references: ${error.message}`); throw error; }
      }
    }),
    vscode.languages.registerHoverProvider(selector, {
      async provideHover(document, position, token) {
        try {
          const content = await values(document, position, token);
          return content ? new vscode.Hover(new vscode.MarkdownString(content)) : undefined;
        } catch (error) { report(`Compiled value hover: ${error.message}`); return undefined; }
      }
    }),
    vscode.languages.registerRenameProvider(selector, {
      async prepareRename(document, position, token) {
        const state = await capture(document, token);
        const match = selected(state, position);
        if (match.symbol.renamable === false) {
          throw new Error(bindings.renameUnavailable(match.symbol));
        }
        prepared.set(document.uri.toString(), { fingerprint: fingerprint(state), version: document.version,
          name: match.symbol.name, kind: match.symbol.kind || "schema" });
        return { range: range(match.occurrence), placeholder: match.occurrence.spelling || match.symbol.name };
      },
      async provideRenameEdits(document, position, newName, token) {
        const preparation = prepared.get(document.uri.toString());
        prepared.delete(document.uri.toString());
        const state = await capture(document, token);
        const match = selected(state, position);
        if (preparation && (preparation.version !== document.version || preparation.fingerprint !== fingerprint(state)
            || preparation.name !== match.symbol.name || preparation.kind !== (match.symbol.kind || "schema"))) {
          throw new Error("The project changed after rename preparation; start Rename again.");
        }
        // Reject expansion before constructing candidate strings. Names and
        // bound occurrences are ASCII; byte growth is exact for the edits.
        const kind = match.symbol.kind || "schema";
        if (!bindings.validName(newName, kind)) throw new Error(kind === "schema"
          ? "A schema name must start with an ASCII letter and contain only letters, digits and underscores."
          : "This name must contain only ASCII letters, digits, underscores and hyphens.");
        const growth = new Map();
        for (const entry of match.symbol.occurrences) {
          const file = bindings.key(entry.path);
          growth.set(file, (growth.get(file) || 0) + newName.length - (entry.spelling ?? match.symbol.name).length);
        }
        const candidateBudget = resources.budget();
        for (const [file, source] of state.sources) resources.bytes(source.bytes + (growth.get(file) || 0), candidateBudget);
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
