const { test } = require("node:test");
const assert = require("node:assert/strict");
const path = require("path");
const fs = require("fs/promises");
const { execFile } = require("child_process");
const { promisify } = require("util");
const { createIndex, abbreviations } = require("../src/language-model");
const { formatDocument } = require("../src/formatter");
const { temporaryWorkspace } = require("./test-workspace");

const compiler = process.env.ABSTRACT_COMPILER_PATH || path.resolve(__dirname, "../../target/debug", process.platform === "win32" ? "abstract.exe" : "abstract");

test("accepted dotted-path edits preserve byte-exact compiler output in all formats and versions", async () => {
  const workspace = await temporaryWorkspace();
  try {
    const schema = "versions 1..3\nschema Product {\nowner {\nteam: text\ncontact: text\n}\n}\n";
    const source = 'Product :: @id.x\n  owner {\n    // keep\n    team: "a, b" @since(2) // keep\n    contact: inbox @since(2)\n  }\n  owner.team: First @removed(2)\n  owner.contact: old @removed(2)\n';
    const schemaFile = await workspace.write("data/schema.abt", schema);
    const file = await workspace.write("data/product.ab", source);
    const index = createIndex([{ file: schemaFile, text: schema }, { file, text: source }]);
    const [edit] = abbreviations(index, file);
    assert.ok(edit);
    const compile = async (format) => {
      const result = await promisify(execFile)(compiler, ["compile", workspace.root, format], { shell: false, windowsHide: true, encoding: "buffer" });
      return result.stdout;
    };
    for (const format of ["JSON", "YML", "RAW"]) {
      await fs.writeFile(file, source);
      const before = await compile(format);
      await fs.writeFile(file, source.slice(0, edit.start) + edit.replacement + source.slice(edit.end));
      assert.deepEqual(await compile(format), before, format);
    }
  } finally { await workspace.dispose(); }
});

test("whole-document formatting preserves byte-exact compiler output in every format and version", async () => {
  const workspace = await temporaryWorkspace();
  try {
    const schema = "versions 1..2\nschema Page {\nvisible: bool\ncopy[] {\nkey: enum(en, es) @tag\nvalue: text\n}\n}\n";
    const source = "Page :: @id.home,\n@visible\ncopy(key, value): [\n(en, Hello),\n(es, Hola),\n]\n";
    const schemaFile = await workspace.write("data/page.abt", schema);
    const sourceFile = await workspace.write("data/page.ab", source);
    const formattedSchema = formatDocument(schema, { insertSpaces: true, tabSize: 4 });
    const formattedSource = formatDocument(source, { insertSpaces: true, tabSize: 4 });
    const compile = async (format) => (await promisify(execFile)(compiler, ["compile", workspace.root, format],
      { shell: false, windowsHide: true, encoding: "buffer" })).stdout;
    for (const format of ["JSON", "YML", "RAW"]) {
      await fs.writeFile(schemaFile, schema); await fs.writeFile(sourceFile, source);
      const before = await compile(format);
      await fs.writeFile(schemaFile, formattedSchema); await fs.writeFile(sourceFile, formattedSource);
      assert.deepEqual(await compile(format), before, format);
    }
  } finally { await workspace.dispose(); }
});
