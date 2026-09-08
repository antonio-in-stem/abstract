const vscode = require("vscode");
const assert = require("node:assert/strict");
const path = require("path");
const fs = require("fs/promises");

async function runSchemaIntegration(root) {
  const uri = vscode.Uri.file(path.join(root, "symbols/data/item.ab"));
  const schemaUri = vscode.Uri.file(path.join(root, "symbols/data/model.abt"));
  const document = await vscode.workspace.openTextDocument(uri);
  const schema = await vscode.workspace.openTextDocument(schemaUri);
  await vscode.window.showTextDocument(document);
  const oldSource = await fs.readFile(uri.fsPath, "utf8");
  const oldSchema = await fs.readFile(schemaUri.fsPath, "utf8");
  const references = await vscode.commands.executeCommand("vscode.executeReferenceProvider", uri, new vscode.Position(0, 2));
  assert.equal(references.length, 3);
  assert.equal(new Set(references.map((entry) => entry.uri.toString())).size, 2);
  for (const reference of references) {
    const source = await vscode.workspace.openTextDocument(reference.uri);
    assert.equal(source.getText(reference.range), "Offer");
  }
  const edit = await vscode.commands.executeCommand("vscode.executeDocumentRenameProvider", uri, new vscode.Position(0, 2), "Catalog");
  assert.equal(edit.size, 2);
  assert.equal([...edit.entries()].reduce((sum, [, entries]) => sum + entries.length, 0), 3);
  assert.equal(document.getText(), oldSource);
  assert.equal(schema.getText(), oldSchema);
  assert.equal(await vscode.workspace.applyEdit(edit), true);
  assert.ok(document.getText().startsWith("Catalog ::"));
  assert.ok(schema.getText().includes("schema Catalog {"));
  assert.ok(schema.getText().includes("logic Catalog {"));
  assert.ok(schema.getText().includes('throw "Offer"'));
  assert.ok(schema.getText().includes("// Offer"));
  assert.equal(document.isDirty, true); assert.equal(schema.isDirty, true);
  assert.equal(await fs.readFile(uri.fsPath, "utf8"), oldSource);
  assert.equal(await fs.readFile(schemaUri.fsPath, "utf8"), oldSchema);
  const unsaved = await vscode.commands.executeCommand("vscode.executeReferenceProvider", schemaUri, new vscode.Position(0, 9));
  assert.equal(unsaved.length, 3);
  await assert.rejects(vscode.commands.executeCommand("vscode.executeDocumentRenameProvider", uri, new vscode.Position(0, 2), "Detail"), /already declared/);
  assert.ok(document.getText().startsWith("Catalog ::"));
  const privateUri = vscode.Uri.file(path.join(root, "symbols/data/detail.abt"));
  // VS Code 1.92.0's command adapter passes trailing args as an array when
  // creating a transient model (editorExtensions.ts:463); Rename expects a
  // string. Exercise the editor flow with its actual open document model.
  await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(privateUri));
  const privateRefs = await vscode.commands.executeCommand("vscode.executeReferenceProvider", privateUri, new vscode.Position(0, 9));
  assert.equal(privateRefs.length, 3);
  const privateEdit = await vscode.commands.executeCommand("vscode.executeDocumentRenameProvider", privateUri, new vscode.Position(0, 9), "PrivateDetail");
  assert.equal(privateEdit.size, 2);
  assert.equal(await vscode.workspace.applyEdit(privateEdit), true);
  assert.ok(schema.getText().includes("$(PrivateDetail)"));
  assert.ok(schema.getText().includes("ref(PrivateDetail)"));
  const unsupported = await vscode.commands.executeCommand("vscode.executeReferenceProvider", uri, new vscode.Position(1, 2));
  assert.equal(unsupported.length, 0);
  console.log("PASS: compiler schema references and Rename in real VS Code, declaration/logic/header/nested/ref, unsaved multi-file edits, collisions, unchanged disk and homonymous text");
}

module.exports = { runSchemaIntegration };
