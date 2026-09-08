const fs = require("fs/promises");
const path = require("path");
const bindings = require("./schema-bindings");
const resources = require("./schema-resources");
const { discoveryRoot, discover, isSource } = require("./project-index");

// Bounded editor/disk capture shared by schema operations and public inventory.
// The owner supplies cancellation/revision checks; no compiler launch or UI here.
function createSourceCapture(vscode, resolveProjectPath, workspaceRoot, check) {
  const insideDiscovery = (root, file) => {
    const relative = path.relative(root, file);
    return relative && relative !== ".." && !relative.startsWith(`..${path.sep}`) && !path.isAbsolute(relative)
      && !relative.split(path.sep).slice(0, -1).some((part) => part.startsWith(".") || ["node_modules", "target", "build", "out"].includes(part.toLowerCase()));
  };
  async function canonical(file) {
    try { return await fs.realpath(file); }
    catch { return path.join(await fs.realpath(path.dirname(file)), path.basename(file)); }
  }
  async function capture(document, state) {
    if (!vscode.workspace.isTrusted) throw new Error("Compiler source analysis requires Workspace Trust.");
    const target = resolveProjectPath(document);
    if (!target) throw new Error("Open this source in an Abstract project before requesting compiler analysis.");
    Object.assign(state, { documents: [], requestedRoot: target, root: await fs.realpath(target), sources: new Map() });
    state.cwd = workspaceRoot(document) || state.root;
    state.compiler = vscode.workspace.getConfiguration("abstract", document.uri).get("compilerPath", "abstract");
    state.limits = { ...resources.LIMITS, check: () => check(state) };
    state.discovery = await discoveryRoot(state.root, state.limits);
    state.diskFiles = await discover(state.discovery, state.limits);
    const admission = resources.budget();
    const diskSet = new Set(state.diskFiles.map(bindings.key));
    for (const open of vscode.workspace.textDocuments.filter((d) => d.uri.scheme === "file" && isSource(d.uri.fsPath))) {
      let file;
      try { file = await canonical(open.uri.fsPath); }
      catch (error) {
        if (insideDiscovery(state.discovery, open.uri.fsPath) || diskSet.has(bindings.key(open.uri.fsPath))) throw error;
        continue; // An unrelated project's missing parent is not this request's input.
      }
      const inside = insideDiscovery(state.discovery, file);
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
    if (!state.sources.has(bindings.key(state.file))) throw new Error("The active source is not in this project's captured source inventory.");
    state.texts = new Map([...state.sources].map(([file, source]) => [file, source.text]));
    check(state);
    // Revalidate saved files before even capability negotiation: open editors
    // can hold old text while their backing files have grown outside watchers.
    await current(state);
    return state;
  }
  async function current(state) {
    check(state);
    if (bindings.key(await fs.realpath(state.requestedRoot)) !== bindings.key(state.root)) {
      throw new Error("The project alias changed its canonical target during source analysis.");
    }
    if (bindings.key(await discoveryRoot(state.root, state.limits)) !== bindings.key(state.discovery)) {
      throw new Error("Project discovery selection changed during source analysis.");
    }
    // Opening a new dirty alias is a source change even if no editor event was
    // observed. The navigation owner may first admit its own newly opened copy.
    for (const open of vscode.workspace.textDocuments) {
      if (open.uri.scheme !== "file" || !isSource(open.uri.fsPath)
        || state.documents.some((entry) => entry.document === open)) continue;
      let file;
      try { file = await canonical(open.uri.fsPath); }
      catch (error) {
        if (insideDiscovery(state.discovery, open.uri.fsPath) || state.sources.has(bindings.key(open.uri.fsPath))) throw error;
        continue;
      }
      if (state.sources.has(bindings.key(file)) || insideDiscovery(state.discovery, file)) {
        throw new Error("The open editor source inventory changed during analysis.");
      }
    }
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
  return { capture, current, canonical };
}

module.exports = { createSourceCapture };
