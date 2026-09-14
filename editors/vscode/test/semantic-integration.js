const vscode = require("vscode");
const assert = require("node:assert/strict");
const path = require("path");
const fs = require("fs/promises");

async function runSemanticIntegration(root) {
  const schemaUri = vscode.Uri.file(path.join(root, "semantic/data/model.abt"));
  const itemUri = vscode.Uri.file(path.join(root, "semantic/data/item.ab"));
  const copyUri = vscode.Uri.file(path.join(root, "semantic/data/copy.ab"));
  const schema = await vscode.workspace.openTextDocument(schemaUri);
  const item = await vscode.workspace.openTextDocument(itemUri);
  const copy = await vscode.workspace.openTextDocument(copyUri);
  const original = new Map(await Promise.all([schemaUri, itemUri, copyUri].map(async (uri) => [uri.fsPath, await fs.readFile(uri.fsPath, "utf8")])));

  await vscode.window.showTextDocument(schema);
  const fieldPosition = new vscode.Position(1, 2);
  const fieldRefs = await vscode.commands.executeCommand("vscode.executeReferenceProvider", schemaUri, fieldPosition);
  assert.equal(fieldRefs.length, 4, "field identity did not cover declaration, logic and two assignments");
  await assert.rejects(vscode.commands.executeCommand("vscode.executeDocumentRenameProvider", schemaUri, fieldPosition, "peer-offer"),
    /already declared|E302/);
  const fieldEdit = await vscode.commands.executeCommand("vscode.executeDocumentRenameProvider", schemaUri, fieldPosition, "power");
  assert.equal(await vscode.workspace.applyEdit(fieldEdit), true);
  assert.match(schema.getText(), /power: int/);
  assert.match(item.getText(), /power: 2/);
  assert.match(copy.getText(), /power: 3/);
  assert.match(schema.getText(), /schema Detail \{\nvalue: text/, "homonymous field in another schema changed");

  const instancePosition = new vscode.Position(0, 13);
  const instanceRefs = await vscode.commands.executeCommand("vscode.executeReferenceProvider", itemUri, instancePosition);
  assert.equal(instanceRefs.length, 3, "instance identity did not cover declaration, clone and ref value");
  const instanceEdit = await vscode.commands.executeCommand("vscode.executeDocumentRenameProvider", itemUri, instancePosition, "primary-offer");
  assert.equal(await vscode.workspace.applyEdit(instanceEdit), true);
  assert.match(item.getText(), /@id\.primary-offer/);
  assert.match(copy.getText(), /&primary-offer\.\*/);
  assert.match(copy.getText(), /peer_offer: primary-offer/);

  const loopPosition = new vscode.Position(10, 6);
  const loopRefs = await vscode.commands.executeCommand("vscode.executeReferenceProvider", schemaUri, loopPosition);
  assert.equal(loopRefs.length, 2, "sibling loops were not scoped as separate identities");
  const loopEdit = await vscode.commands.executeCommand("vscode.executeDocumentRenameProvider", schemaUri, loopPosition, "row");
  assert.equal(await vscode.workspace.applyEdit(loopEdit), true);
  const lines = schema.getText().split(/\r?\n/);
  assert.match(lines[10], /\$row/); assert.match(lines[11], /\$row\.label/);
  assert.match(lines[13], /\$entry/); assert.match(lines[14], /\$entry\.label/);

  const labelPosition = new vscode.Position(5, 2);
  const labelRefs = await vscode.commands.executeCommand("vscode.executeReferenceProvider", schemaUri, labelPosition);
  assert.equal(labelRefs.length, 5, "field identity did not cover loop, #tag argument and tuple column uses");
  const labelEdit = await vscode.commands.executeCommand("vscode.executeDocumentRenameProvider", schemaUri, labelPosition, "caption");
  assert.equal(await vscode.workspace.applyEdit(labelEdit), true);
  assert.match(item.getText(), /#a\(caption: one\)/);
  assert.match(copy.getText(), /items\(key, caption\)/);
  assert.equal(schema.isDirty, true); assert.equal(item.isDirty, true); assert.equal(copy.isDirty, true);
  for (const [file, text] of original) assert.equal(await fs.readFile(file, "utf8"), text, `${file} was written during Rename`);
  console.log("PASS: field/instance/loop references and scoped Rename in real VS Code, including tag/tuple uses and unchanged disk");
}

module.exports = { runSemanticIntegration };
