const vscode = require("vscode");
const assert = require("node:assert/strict");
const path = require("path");

async function eventually(check, message) {
  const deadline = Date.now() + 15000;
  while (Date.now() < deadline) {
    if (await check()) return;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(message);
}

async function run() {
  const root = process.env.ABSTRACT_TEST_ROOT;
  const uri = vscode.Uri.file(path.join(root, "one/data/product.ab"));
  const document = await vscode.workspace.openTextDocument(uri);
  await vscode.window.showTextDocument(document);
  await vscode.extensions.getExtension("minhocreates.abstract-language").activate();
  console.log(`Testing Abstract in VS Code ${vscode.version}`);

  const fields = await vscode.commands.executeCommand("vscode.executeCompletionItemProvider", uri, new vscode.Position(2, 0));
  assert.ok(fields.items.some((item) => item.label === "team"));
  assert.ok(!fields.items.some((item) => item.label === "status"));
  const values = await vscode.commands.executeCommand("vscode.executeCompletionItemProvider", uri, new vscode.Position(4, 8));
  assert.ok(values.items.some((item) => item.label === "active"));
  const definitions = await vscode.commands.executeCommand("vscode.executeDefinitionProvider", uri, new vscode.Position(2, 2));
  assert.equal(definitions.length, 1);
  assert.ok((definitions[0].uri || definitions[0].targetUri).fsPath.endsWith("schema.abt"));
  const hovers = await vscode.commands.executeCommand("vscode.executeHoverProvider", uri, new vscode.Position(2, 2));
  assert.ok(hovers.some((hover) => hover.contents.some((content) => content.value.includes("team: text"))));
  const symbols = await vscode.commands.executeCommand("vscode.executeDocumentSymbolProvider", uri);
  assert.equal(symbols[0].name, "x");
  const semantic = await vscode.commands.executeCommand("vscode.provideDocumentSemanticTokens", vscode.Uri.file(path.join(root, "one/data/schema.abt")));
  assert.ok(semantic.data.length > 0);

  const actions = await vscode.commands.executeCommand("vscode.executeCodeActionProvider", uri, new vscode.Range(1, 0, 3, 1), "refactor.rewrite");
  const dotted = actions.find((action) => action.title.startsWith("Use a dotted path"));
  assert.ok(dotted?.edit);
  await vscode.workspace.applyEdit(dotted.edit);
  assert.ok(document.getText().includes("owner.team: Example"));
  await document.save();

  // Unsaved schema changes must affect completion immediately, without a
  // compiler run or a write to the user's project.
  const schemaUri = vscode.Uri.file(path.join(root, "one/data/schema.abt"));
  const schema = await vscode.workspace.openTextDocument(schemaUri);
  const edit = new vscode.WorkspaceEdit();
  edit.insert(schemaUri, new vscode.Position(2, 0), "extra: bool @optional\n");
  await vscode.workspace.applyEdit(edit);
  const fresh = await vscode.commands.executeCommand("vscode.executeCompletionItemProvider", uri, new vscode.Position(1, 6));
  assert.ok(fresh.items.some((item) => item.label === "extra"));
  await schema.save();

  const otherUri = vscode.Uri.file(path.join(root, "two/data/other.ab"));
  await vscode.workspace.openTextDocument(otherUri);
  await eventually(() => vscode.languages.getDiagnostics(otherUri).some((d) => d.code === "E412"), "Other project's compiler diagnostic was not published.");
  await vscode.window.showTextDocument(document);
  await vscode.commands.executeCommand("abstract.lintCurrentProject");
  assert.ok(vscode.languages.getDiagnostics(otherUri).some((d) => d.code === "E412"), "Linting a valid project erased another project's error.");
  assert.deepEqual(vscode.languages.getDiagnostics(uri), []);
  console.log("PASS: completions, definitions, hover, symbols, semantic tokens, refactor, unsaved index, multi-project diagnostics");
}

module.exports = { run };
