const { test } = require("node:test");
const assert = require("node:assert/strict");
const path = require("path");
const fs = require("fs/promises");
const { execFile } = require("child_process");
const { promisify } = require("util");
const { createIndex, abbreviations } = require("../src/language-model");
const { temporaryWorkspace } = require("./test-workspace");

const compiler = process.env.ABSTRACT_COMPILER_PATH || path.resolve(__dirname, "../../../target/debug", process.platform === "win32" ? "abstract.exe" : "abstract");

test("accepted dotted-path edits preserve byte-exact compiler output in all formats and versions", async () => {
  const workspace = await temporaryWorkspace();
  try {
    const schema = "versions 1..3\nschema Product {\nowner {\nteam: text\n}\n}\n";
    const source = 'Product :: @id.x\n  owner {\n    team: "a, b" @since(2) // keep\n  }\n  owner.team: First @removed(2)\n';
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
