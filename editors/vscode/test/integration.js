const vscode = require("vscode");
const assert = require("node:assert/strict");
const path = require("path");
const fs = require("fs/promises");

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

  if (process.env.ABSTRACT_TEST_RESTRICTED === "1") {
    assert.equal(vscode.workspace.isTrusted, false);
    const fields = await vscode.commands.executeCommand("vscode.executeCompletionItemProvider", uri, new vscode.Position(2, 0));
    assert.ok(fields.items.some((item) => item.label === "team"));
    const otherUri = vscode.Uri.file(path.join(root, "two/data/other.ab"));
    await vscode.workspace.openTextDocument(otherUri);
    await vscode.commands.executeCommand("abstract.lintCurrentProject");
    await new Promise((resolve) => setTimeout(resolve, 700));
    assert.deepEqual(vscode.languages.getDiagnostics(uri), []);
    assert.deepEqual(vscode.languages.getDiagnostics(otherUri), []);
    console.log("PASS: Restricted Mode keeps completions and suppresses compiler diagnostics/commands");
    return;
  }

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

  const liveUri = vscode.Uri.file(path.join(root, "one/data/live.ab"));
  const liveSchemaUri = vscode.Uri.file(path.join(root, "one/data/live.abt"));
  const liveDocument = await vscode.workspace.openTextDocument(liveUri);
  const liveSchema = await vscode.workspace.openTextDocument(liveSchemaUri);
  await vscode.window.showTextDocument(liveDocument);
  const diskSource = await fs.readFile(liveUri.fsPath, "utf8");
  const diskSchema = await fs.readFile(liveSchemaUri.fsPath, "utf8");
  async function replace(document, text) {
    const edit = new vscode.WorkspaceEdit();
    edit.replace(document.uri, new vscode.Range(document.positionAt(0), document.positionAt(document.getText().length)), text);
    await vscode.workspace.applyEdit(edit);
  }
  await replace(liveSchema, diskSchema.replace("label: text", "label: int"));
  await eventually(() => vscode.languages.getDiagnostics(liveUri).some((d) => d.code === "E412"), "Unsaved schema did not change compiler diagnostics.");
  await replace(liveDocument, diskSource.replace("label: Disk", "label: 42").replace("./note.txt", "./missing.txt"));
  await eventually(() => vscode.languages.getDiagnostics(liveUri).some((d) => d.code === "E421"), "Unsaved instance did not run real asset checks.");
  // Supersede an invalid snapshot before its debounce/run can publish.
  await replace(liveDocument, diskSource.replace("label: Disk", "label: nope"));
  await replace(liveDocument, diskSource.replace("label: Disk", "label: 42"));
  await vscode.commands.executeCommand("abstract.lintCurrentProject");
  await new Promise((resolve) => setTimeout(resolve, 500));
  assert.deepEqual(vscode.languages.getDiagnostics(liveUri), []);
  assert.ok(vscode.languages.getDiagnostics(otherUri).some((d) => d.code === "E412"));
  assert.equal(liveDocument.isDirty, true);
  assert.equal(liveSchema.isDirty, true);
  assert.equal(await fs.readFile(liveUri.fsPath, "utf8"), diskSource);
  assert.equal(await fs.readFile(liveSchemaUri.fsPath, "utf8"), diskSchema);
  assert.equal(await fs.readFile(path.join(root, "one/assets/note.txt"), "utf8"), "actual project asset");
  console.log("PASS: unsaved schema + instance diagnostics, real relative assets, superseded snapshots, two-project isolation, no disk writes");

  // The editor opens the canonical file outside both project directories;
  // both compilers discover it through their own junctions.
  const sharedUri = vscode.Uri.file(path.join(root, "shared/schema.abt"));
  const sharedSchema = await vscode.workspace.openTextDocument(sharedUri);
  const sharedDisk = await fs.readFile(sharedUri.fsPath, "utf8");
  const sharedInstances = ["one", "two"].map((project) => vscode.Uri.file(path.join(root, project, "data/shared-instance.ab")));
  await replace(sharedSchema, sharedDisk.replace("value: int", "value: bool"));
  await eventually(() => sharedInstances.every((target) => vscode.languages.getDiagnostics(target).some((d) => d.code === "E412")), "Canonical unsaved schema did not refresh both linked projects.");
  await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(otherUri));
  assert.equal(await vscode.commands.executeCommand("abstract.compileCurrentProject"), false, "Compile ran with a dirty canonical schema outside its root.");
  await vscode.window.showTextDocument(liveDocument);
  await replace(sharedSchema, sharedDisk);
  await eventually(() => sharedInstances.every((target) => vscode.languages.getDiagnostics(target).length === 0)
    && vscode.languages.getDiagnostics(otherUri).some((d) => d.code === "E412"), "Linked projects did not publish the corrected schema snapshot.");
  assert.equal(await fs.readFile(sharedUri.fsPath, "utf8"), sharedDisk);
  assert.ok(vscode.languages.getDiagnostics(otherUri).some((d) => d.code === "E412"));
  console.log("PASS: canonical unsaved schema outside two project roots refreshes both junction consumers and prevents saved-only Compile");
  await sharedSchema.save();

  const unicodeUri = vscode.Uri.file(path.join(root, "unicode/data/schema.abt"));
  const unicodeDocument = await vscode.workspace.openTextDocument(unicodeUri);
  await eventually(() => vscode.languages.getDiagnostics(unicodeUri).length > 0, "Unicode fixture was not analyzed.");
  const unicodeDisk = await fs.readFile(unicodeUri.fsPath, "utf8");
  const emojiDiagnostic = () => vscode.languages.getDiagnostics(unicodeUri).find((d) => d.code === "E210");
  assert.equal(emojiDiagnostic().range.start.character, unicodeDocument.getText().indexOf("😀"));
  assert.equal(emojiDiagnostic().range.end.character, unicodeDocument.getText().indexOf("😀") + 2);
  // A BOM loaded from disk is encoding metadata; a BOM inserted in the buffer
  // is part of TextDocument text. Both must select exactly the emoji.
  await replace(unicodeDocument, `\uFEFF${unicodeDocument.getText()}`);
  await eventually(() => emojiDiagnostic()?.range.start.character === unicodeDocument.getText().indexOf("😀"), "Unsaved Unicode range did not include the buffer's typed BOM.");
  assert.equal(emojiDiagnostic().range.end.character, unicodeDocument.getText().indexOf("😀") + 2);
  assert.equal(await fs.readFile(unicodeUri.fsPath, "utf8"), unicodeDisk);
  console.log("PASS: real editor UTF-16 ranges handle disk BOM, typed BOM, supplementary scalar and CRLF");

  if (process.env.ABSTRACT_LEGACY_COMPILER_PATH) {
    // Use a real pre-protocol compiler, not a mock claiming compatibility.
    await vscode.workspace.getConfiguration("abstract", liveUri).update("compilerPath", process.env.ABSTRACT_LEGACY_COMPILER_PATH, vscode.ConfigurationTarget.WorkspaceFolder);
    await vscode.commands.executeCommand("abstract.lintCurrentProject");
    assert.deepEqual(vscode.languages.getDiagnostics(liveUri), [], "Old compiler must not pretend to validate unsaved buffers.");
    await replace(liveDocument, diskSource);
    await liveDocument.save();
    await replace(liveSchema, diskSchema);
    await liveSchema.save();
    await replace(liveDocument, diskSource.replace("./note.txt", "./absent.txt"));
    await liveDocument.save();
    await eventually(() => vscode.languages.getDiagnostics(liveUri).some((d) => d.code === "E421"), "Real old compiler fallback did not validate saved files.");
    console.log("PASS: real legacy compiler fallback validates saved sources and refuses unsaved claims");
  } else console.log("SKIP: real legacy compiler compatibility (set ABSTRACT_LEGACY_COMPILER_PATH)");
}

module.exports = { run };
