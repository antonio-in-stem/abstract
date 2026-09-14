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
  let document = await vscode.workspace.openTextDocument(uri);
  await vscode.window.showTextDocument(document);
  await vscode.extensions.getExtension("antonio-in-stem.abstract-language").activate();
  console.log(`Testing Abstract in VS Code ${vscode.version}`);

  assert.equal(document.languageId, "swift", "conflicting user association fixture did not apply");
  await vscode.workspace.getConfiguration("files").update("associations",
    { "*.ab": "abstract", "*.abt": "abstract" }, vscode.ConfigurationTarget.Workspace);
  await eventually(() => document.languageId === "abstract",
    "root workspace associations did not override the global Swift association in a multi-root workspace");
  assert.equal(vscode.workspace.getConfiguration("files").get("associations")["*.ab"], "abstract");
  console.log("PASS: root workspace associations override a global conflict that folder-only settings cannot fix in multi-root mode");
  await vscode.workspace.getConfiguration("files").update("associations",
    { "*.ab": "swift", "*.abt": "swift" }, vscode.ConfigurationTarget.Workspace);
  await eventually(() => document.languageId === "swift", "test could not restore the conflicting workspace association");
  assert.equal(await vscode.commands.executeCommand("abstract.useForWorkspace", uri), true);
  await eventually(() => document.languageId === "abstract", "workspace recovery command did not select Abstract");

  const syntaxDocument = await vscode.workspace.openTextDocument({ language: "abstract", content:
    "schema Scratch {\nlabel: text @optional\n}\n// logic and derive are prose here\n" });
  const syntaxHovers = await vscode.commands.executeCommand("vscode.executeHoverProvider", syntaxDocument.uri, new vscode.Position(0, 2));
  const syntaxHover = syntaxHovers.find((hover) => hover.contents.some((content) => (content.value || content).includes("Schema declaration")));
  assert.ok(syntaxHover, "untitled Abstract document did not provide offline syntax help");
  assert.deepEqual(syntaxHover.range, new vscode.Range(0, 0, 0, 6));
  const typeHovers = await vscode.commands.executeCommand("vscode.executeHoverProvider", syntaxDocument.uri, new vscode.Position(1, 8));
  assert.ok(typeHovers.some((hover) => hover.contents.some((content) => (content.value || content).includes("`text` type"))));
  const proseHovers = await vscode.commands.executeCommand("vscode.executeHoverProvider", syntaxDocument.uri, new vscode.Position(3, 4));
  assert.deepEqual(proseHovers, [], "syntax help matched a keyword spelling inside a comment");
  console.log("PASS: offline contextual syntax hover works in an untitled buffer with precise ranges and ignores comments");

  const constraintDocument = await vscode.workspace.openTextDocument({ language: "abstract", content:
    'schema Contact {\ncity: text(1..60) = "Mexico City"\nwheels[4..4]: text\n}\n' });
  for (const [line, column] of [[1, 18], [2, 7], [2, 8], [2, 10]]) {
    const constraintHovers = await vscode.commands.executeCommand("vscode.executeHoverProvider", constraintDocument.uri, new vscode.Position(line, column));
    assert.ok(constraintHovers.length, `missing constraint help at ${line}:${column}`);
    assert.ok(constraintHovers.some((hover) => hover.contents.some((content) => /default|exactly 4/i.test(content.value || content))),
      `constraint help did not explain its meaning at ${line}:${column}`);
  }

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
    await assert.rejects(vscode.commands.executeCommand("vscode.prepareRename", uri, new vscode.Position(0, 2)), /Workspace Trust/);
    console.log("PASS: Restricted Mode keeps completions and suppresses compiler diagnostics/commands and semantic rename");
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
  assert.ok(hovers.some((hover) => hover.contents.some((content) => content.value.includes("Runtime bindings are still required"))));
  const publicCompletion = await vscode.commands.executeCommand("vscode.executeCompletionItemProvider",
    vscode.Uri.file(path.join(root, "one/data/schema.abt")), new vscode.Position(1, 13));
  assert.ok(publicCompletion.items.some((item) => item.label === "public" && item.detail.includes("Author nomination")));
  const symbols = await vscode.commands.executeCommand("vscode.executeDocumentSymbolProvider", uri);
  assert.equal(symbols[0].name, "x");
  const semantic = await vscode.commands.executeCommand("vscode.provideDocumentSemanticTokens", vscode.Uri.file(path.join(root, "one/data/schema.abt")));
  assert.ok(semantic.data.length > 0);

  const actions = await vscode.commands.executeCommand("vscode.executeCodeActionProvider", uri, new vscode.Range(1, 0, 3, 1), "refactor.rewrite");
  const dotted = actions.find((action) => action.title.startsWith("Flatten body block"));
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

  const powerUri = vscode.Uri.file(path.join(root, "one/data/power.ab"));
  const powerDocument = await vscode.workspace.openTextDocument(powerUri);
  const powerDisk = await fs.readFile(powerUri.fsPath, "utf8");
  await replace(powerDocument, powerDisk.replace("100", "101"));
  await eventually(() => vscode.languages.getDiagnostics(powerUri).some((d) => d.code === "E413"),
    "Dirty power=101 did not publish E413 in the real VS Code host.");
  assert.equal(powerDocument.isDirty, true);
  assert.equal(await fs.readFile(powerUri.fsPath, "utf8"), powerDisk);
  console.log("PASS: conflicting language association recovery and dirty power=101 live diagnostic");

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

  const hoverUri = vscode.Uri.file(path.join(root, "hover/data/car.ab"));
  const hoverSchemaUri = vscode.Uri.file(path.join(root, "hover/data/car.abt"));
  const compiledHovers = await vscode.commands.executeCommand("vscode.executeHoverProvider", hoverUri, new vscode.Position(2, 3));
  const compiledText = compiledHovers.flatMap((hover) => hover.contents).map((content) => content.value || content).join("\n");
  assert.match(compiledText, /Effective compiled value/); assert.match(compiledText, /"standard"/);
  const versionHovers = await vscode.commands.executeCommand("vscode.executeHoverProvider", hoverUri, new vscode.Position(3, 3));
  const versionText = versionHovers.flatMap((hover) => hover.contents).map((content) => content.value || content).join("\n");
  assert.match(versionText, /versions 1–2.*7/); assert.match(versionText, /version 3.*field absent/);
  const schemaHovers = await vscode.commands.executeCommand("vscode.executeHoverProvider", hoverSchemaUri, new vscode.Position(3, 3));
  const schemaText = schemaHovers.flatMap((hover) => hover.contents).map((content) => content.value || content).join("\n");
  assert.match(schemaText, /Declared default/); assert.match(schemaText, /10/);
  console.log("PASS: compiler-backed hover shows derived values and version projections; schema hover shows declared defaults");

  const mathUri = vscode.Uri.file(path.join(root, "math/data/bill.ab"));
  const mathSchemaUri = vscode.Uri.file(path.join(root, "math/data/bill.abt"));
  await vscode.workspace.openTextDocument(mathUri);
  await vscode.workspace.openTextDocument(mathSchemaUri);
  const mathHovers = await vscode.commands.executeCommand("vscode.executeHoverProvider", mathUri, new vscode.Position(3, 2));
  assert.ok(mathHovers.some((hover) => hover.contents.some((content) => /420/.test(content.value || content))), "arithmetic result missing from compiled-value hover");
  const calcHovers = await vscode.commands.executeCommand("vscode.executeHoverProvider", mathSchemaUri, new vscode.Position(6, 17));
  assert.ok(calcHovers.some((hover) => hover.contents.some((content) => /numeric arithmetic/.test(content.value || content))), "calc syntax help missing");
  const mathReferences = await vscode.commands.executeCommand("vscode.executeReferenceProvider", mathSchemaUri, new vscode.Position(1, 2));
  assert.ok(mathReferences.some((location) => location.uri.toString() === mathSchemaUri.toString() && location.range.start.line === 6), "calc field read missing from semantic references");
  console.log("PASS: calc syntax hover, computed value 420 and compiler-owned arithmetic references");

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
  await require("./schema-integration").runSchemaIntegration(root);
  await require("./semantic-integration").runSemanticIntegration(root);
}

module.exports = { run };
