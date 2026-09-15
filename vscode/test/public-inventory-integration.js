const vscode = require("vscode");
const assert = require("node:assert/strict");
const path = require("path");
const fs = require("fs/promises");

const inspect = "abstract.inspectPublicFields";
const accept = "workbench.action.acceptSelectedQuickOpenItem";
const close = "workbench.action.closeQuickOpen";
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
async function eventually(check, message) {
  const until = Date.now() + 10000;
  while (Date.now() < until) { if (await check()) return; await pause(50); }
  throw new Error(message);
}
async function replace(document, text) {
  const edit = new vscode.WorkspaceEdit();
  edit.replace(document.uri, new vscode.Range(document.positionAt(0), document.positionAt(document.getText().length)), text);
  assert.equal(await vscode.workspace.applyEdit(edit), true);
}
async function record(name, value = {}) {
  console.log(`PASS: ${name}`);
  if (process.env.ABSTRACT_PUBLIC_TEST_EVIDENCE) await fs.appendFile(
    path.join(process.env.ABSTRACT_PUBLIC_TEST_EVIDENCE, "observations.jsonl"),
    JSON.stringify({ name, vscode: vscode.version, ...value }) + "\n");
}
const publicErrors = (uri) => vscode.languages.getDiagnostics(uri).filter((d) => d.source === "abstract-public");

async function run() {
  const root = process.env.ABSTRACT_TEST_ROOT;
  const uri = vscode.Uri.file(path.join(root, "public/data/item.ab"));
  const schemaUri = vscode.Uri.file(path.join(root, "public/data/model.abt"));
  const document = await vscode.workspace.openTextDocument(uri);
  await vscode.window.showTextDocument(document);
  await vscode.extensions.getExtension("antonio-in-stem.abstract-language").activate();
  const commands = await vscode.commands.getCommands();
  assert.ok(commands.includes(inspect), "Public inventory command was not registered.");
  console.log(`Testing public inventory in actual VS Code ${vscode.version}`);
  if (process.env.ABSTRACT_TEST_RESTRICTED === "1") {
    assert.equal(vscode.workspace.isTrusted, false);
    const result = await vscode.commands.executeCommand(inspect);
    assert.equal(result.status, "unavailable");
    assert.equal(result.targets, 0);
    await record("Restricted Mode refuses compiler-backed public inventory", result);
    return;
  }
  assert.ok(commands.includes(accept) && commands.includes(close), "Host has no native QuickPick accept/close commands.");
  const schemaDisk = await fs.readFile(schemaUri.fsPath, "utf8");
  const instanceDisk = await fs.readFile(uri.fsPath, "utf8");
  let result = await vscode.commands.executeCommand(inspect);
  assert.equal(result.status, "export-admitted");
  assert.equal(result.declarations, 2); assert.equal(result.targets, 1); assert.equal(result.rows, 3);
  await vscode.commands.executeCommand(accept);
  await eventually(() => {
    const editor = vscode.window.activeTextEditor;
    return editor?.document.uri.toString() === schemaUri.toString() && editor.document.getText(editor.selection) === "@public";
  }, "Native QuickPick did not navigate to the exact nomination.");
  const schema = await vscode.workspace.openTextDocument(schemaUri);
  await record("Native QuickPick shows admitted and unused nominations and navigates to exact declaration", result);

  await replace(schema, `// inserted 😀 line\n${schemaDisk}`);
  await replace(document, `${instanceDisk}preview: false\n`);
  await vscode.window.showTextDocument(document);
  result = await vscode.commands.executeCommand(inspect);
  assert.equal(result.status, "export-admitted"); assert.equal(result.targets, 1);
  await vscode.commands.executeCommand(accept);
  await eventually(() => {
    const editor = vscode.window.activeTextEditor;
    return editor?.document.uri.toString() === schemaUri.toString()
      && editor.selection.start.line === 3 && editor.document.getText(editor.selection) === "@public";
  }, "Dirty-source navigation used old source positions.");
  assert.equal(await fs.readFile(schemaUri.fsPath, "utf8"), schemaDisk);
  assert.equal(await fs.readFile(uri.fsPath, "utf8"), instanceDisk);
  await record("Unsaved schema and instance are admitted without source writes; UTF-16 location tracks moved declaration");

  await vscode.window.showTextDocument(document);
  result = await vscode.commands.executeCommand(inspect);
  assert.equal(result.status, "export-admitted");
  await replace(schema, `// superseded\n${schema.getText()}`);
  await vscode.commands.executeCommand(accept); await pause(300);
  assert.equal(vscode.window.activeTextEditor.document.uri.toString(), uri.toString());
  await record("Editing after QuickPick readiness prevents stale declaration navigation");

  await vscode.window.showTextDocument(document);
  result = await vscode.commands.executeCommand(inspect);
  assert.equal(result.status, "export-admitted");
  await fs.writeFile(path.join(root, "public/data/new.abt"), "schema New {\nvalue: bool = false\n}\n");
  await vscode.commands.executeCommand(accept); await pause(300);
  assert.equal(vscode.window.activeTextEditor.document.uri.toString(), uri.toString());
  await record("Source membership change after readiness prevents navigation");

  await replace(schema, `${schemaDisk}\nlogic ASettings {\nderive .result = .preview\n}\n`);
  await vscode.window.showTextDocument(document);
  result = await vscode.commands.executeCommand(inspect);
  assert.equal(result.status, "export-rejected"); assert.equal(result.declarations, 2); assert.equal(result.targets, 0);
  await eventually(() => publicErrors(schemaUri).some((d) => d.code === "E702"), "Export-only rejection did not publish a public diagnostic.");
  assert.ok(publicErrors(schemaUri).some((d) => d.relatedInformation?.length), "Export rejection lost related source information.");
  await vscode.commands.executeCommand(close);
  await record("Ordinary-valid dependent public source is export-rejected with separate navigable diagnostics", result);

  await replace(document, `${instanceDisk}preview: invalid\n`);
  await vscode.window.showTextDocument(document);
  result = await vscode.commands.executeCommand(inspect);
  assert.equal(result.status, "unavailable"); assert.equal(result.targets, 0);
  await eventually(() => vscode.languages.getDiagnostics(uri).some((d) => d.code === "E412"), "Ordinary source error was not reported.");
  assert.equal(publicErrors(schemaUri).length, 0, "Previous export diagnostic survived a superseding source error.");
  await record("Ordinary compilation errors make public inventory unavailable and clear stale export diagnostics", result);

  await replace(document, instanceDisk);
  await replace(schema, schemaDisk.replace("bool @public", "bool @ public"));
  await vscode.window.showTextDocument(document);
  result = await vscode.commands.executeCommand(inspect);
  assert.equal(result.status, "unavailable"); assert.equal(result.targets, 0);
  await eventually(() => vscode.languages.getDiagnostics(schemaUri).some((d) => d.code === "E210"), "Spaced modifier did not preserve the lexer rejection.");
  await record("Whitespace separating @ from public remains a language error", result);

  const otherUri = vscode.Uri.file(path.join(root, "other/data/item.ab"));
  await vscode.workspace.openTextDocument(otherUri);
  await eventually(() => vscode.languages.getDiagnostics(otherUri).some((d) => d.code === "E412"), "Second project did not publish its own error.");
  const emptyUri = vscode.Uri.file(path.join(root, "empty/data/item.ab"));
  await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(emptyUri));
  result = await vscode.commands.executeCommand(inspect);
  assert.equal(result.status, "export-admitted"); assert.equal(result.declarations, 0); assert.equal(result.targets, 0);
  assert.ok(vscode.languages.getDiagnostics(otherUri).some((d) => d.code === "E412"));
  await record("Empty public exposure is explicit and another project's diagnostics survive", result);

  const linkedUri = vscode.Uri.file(path.join(root, "linked/data/item.ab"));
  const sharedUri = vscode.Uri.file(path.join(root, "shared/model.abt"));
  const shared = await vscode.workspace.openTextDocument(sharedUri);
  const sharedDisk = await fs.readFile(sharedUri.fsPath, "utf8");
  await replace(shared, `// canonical dirty source\n${sharedDisk}`);
  await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(linkedUri));
  result = await vscode.commands.executeCommand(inspect);
  assert.equal(result.status, "export-admitted"); assert.equal(result.targets, 1);
  await vscode.commands.executeCommand(accept);
  await eventually(() => {
    const editor = vscode.window.activeTextEditor;
    return editor?.document.uri.toString() === sharedUri.toString() && editor.document.getText(editor.selection) === "@public";
  }, "Dirty canonical source outside the project did not navigate through its junction binding.");
  assert.equal(await fs.readFile(sharedUri.fsPath, "utf8"), sharedDisk);
  await record("Dirty canonical schema outside project is captured through junction and navigates without saving");

  const old = process.env.ABSTRACT_PUBLIC_OLD_COMPILER;
  assert.ok(old, "A real prior compiler is required for this gate.");
  await vscode.workspace.getConfiguration("abstract", linkedUri).update("compilerPath", old, vscode.ConfigurationTarget.WorkspaceFolder);
  await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(linkedUri));
  result = await vscode.commands.executeCommand(inspect);
  assert.equal(result.status, "unavailable"); assert.equal(result.targets, 0);
  assert.equal(await fs.readFile(sharedUri.fsPath, "utf8"), sharedDisk);
  await record("Actual Abstract 1.1.0 capability refuses inventory without saved-source fallback", result);
}

module.exports = { run };
