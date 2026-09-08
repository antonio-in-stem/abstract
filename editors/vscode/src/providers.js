const vscode = require("vscode");
const fs = require("fs/promises");
const model = require("./language-model");
const { ProjectIndex, isSource } = require("./project-index");

function registerLanguageFeatures(context, resolveProjectPath, reportError) {
  const selector = { language: "abstract", scheme: "file" };
  const projects = new ProjectIndex();
  let revision = 0;
  const watcher = vscode.workspace.createFileSystemWatcher("**/*");
  const invalidate = (uri) => { if (isSource(uri.fsPath)) projects.invalidate(); };
  context.subscriptions.push(watcher, watcher.onDidCreate(invalidate), watcher.onDidChange(invalidate), watcher.onDidDelete(invalidate),
    vscode.workspace.onDidChangeTextDocument(({ document }) => { if (isSource(document.uri.fsPath)) revision += 1; }),
    vscode.workspace.onDidSaveTextDocument((d) => invalidate(d.uri)),
    vscode.workspace.onDidChangeConfiguration((e) => { if (e.affectsConfiguration("abstract.projectPath")) projects.invalidate(); }));

  async function snapshot(document, token) {
    const started = revision;
    const target = resolveProjectPath(document);
    if (!target) return { index: model.createIndex([{ file: document.uri.fsPath, text: document.getText() }]), file: document.uri.fsPath };
    try {
      const index = await projects.get(target, vscode.workspace.textDocuments.filter((d) => d.uri.scheme === "file" && isSource(d.uri.fsPath))
        .map((d) => ({ file: d.uri.fsPath, text: d.getText() })), token);
      const file = await fs.realpath(document.uri.fsPath).catch(() => document.uri.fsPath);
      if (token?.isCancellationRequested || !index || started !== revision) return undefined;
      return { index, file };
    } catch (error) { reportError(`Language features: ${error.message}`); return undefined; }
  }

  function location(entry) {
    // Entries use offsets in their own source; reading a document via VS Code
    // preserves unsaved positions and avoids converting UTF-8 byte columns.
    return vscode.workspace.openTextDocument(vscode.Uri.file(entry.file)).then((document) =>
      new vscode.Location(document.uri, new vscode.Range(document.positionAt(entry.start), document.positionAt(entry.end))));
  }

  context.subscriptions.push(
    vscode.languages.registerCompletionItemProvider(selector, {
      async provideCompletionItems(document, position, token) {
        const state = await snapshot(document, token);
        if (!state) return [];
        return model.completions(state.index, state.file, document.offsetAt(position)).map((entry) => {
          const item = new vscode.CompletionItem(entry.label, vscode.CompletionItemKind[entry.kind] || vscode.CompletionItemKind.Text);
          item.insertText = entry.snippet ? new vscode.SnippetString(entry.insert || entry.label) : entry.insert || entry.label;
          item.range = new vscode.Range(document.positionAt(entry.from), document.positionAt(entry.to));
          item.detail = entry.detail;
          if (entry.symbol) item.documentation = new vscode.MarkdownString().appendCodeblock(entry.symbol.declaration, "abstract");
          return item;
        });
      }
    }, ".", ":", "@", "#", "&", "(", ","),
    vscode.languages.registerDefinitionProvider(selector, {
      async provideDefinition(document, position, token) {
        const state = await snapshot(document, token);
        return state ? Promise.all(model.definitions(state.index, state.file, document.offsetAt(position)).map(location)) : [];
      }
    }),
    vscode.languages.registerHoverProvider(selector, {
      async provideHover(document, position, token) {
        const state = await snapshot(document, token);
        if (!state) return undefined;
        const entries = model.definitions(state.index, state.file, document.offsetAt(position));
        if (!entries.length) return undefined;
        return new vscode.Hover(entries.map((entry) => {
          const content = new vscode.MarkdownString().appendCodeblock(entry.declaration, "abstract");
          if (entry.public) content.appendMarkdown(`\n\n${model.PUBLIC_NOMINATION_DETAIL}`);
          return content;
        }));
      }
    }),
    vscode.languages.registerDocumentSymbolProvider(selector, {
      provideDocumentSymbols(document) {
        const parsed = model.parseSource(document.uri.fsPath, document.getText());
        const symbols = [];
        function add(entry, container = "") {
          symbols.push(new vscode.SymbolInformation(entry.name, entry.fields ? vscode.SymbolKind.Struct : vscode.SymbolKind.Field,
            container, new vscode.Location(document.uri, new vscode.Range(document.positionAt(entry.start), document.positionAt(entry.end)))));
          for (const field of entry.fields || []) add(field, container ? `${container}.${entry.name}` : entry.name);
        }
        for (const schema of parsed.schemas) add(schema);
        for (const instance of parsed.instances) symbols.push(new vscode.SymbolInformation(instance.name, vscode.SymbolKind.Object,
          instance.schema, new vscode.Location(document.uri, new vscode.Range(document.positionAt(instance.start), document.positionAt(instance.end)))));
        return symbols;
      }
    }),
    vscode.languages.registerCodeActionsProvider(selector, {
      async provideCodeActions(document, range, codeContext, token) {
        if (codeContext.only && !codeContext.only.contains(vscode.CodeActionKind.RefactorRewrite)) return [];
        const state = await snapshot(document, token);
        if (!state) return [];
        return model.abbreviations(state.index, state.file).filter((edit) =>
          edit.start <= document.offsetAt(range.end) && edit.end >= document.offsetAt(range.start)).map((edit) => {
          const action = new vscode.CodeAction(edit.title, vscode.CodeActionKind.RefactorRewrite);
          action.edit = new vscode.WorkspaceEdit();
          action.edit.replace(document.uri, new vscode.Range(document.positionAt(edit.start), document.positionAt(edit.end)), edit.replacement);
          return action;
        });
      }
    }, { providedCodeActionKinds: [vscode.CodeActionKind.RefactorRewrite] })
  );
}

module.exports = { registerLanguageFeatures };
