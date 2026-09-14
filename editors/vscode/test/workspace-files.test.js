const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

test("exercise multi-root workspaces select Abstract at workspace scope and name existing folders", () => {
  const exercises = path.resolve(__dirname, "../../../examples/exercises");
  for (const name of ["abstract-starters.code-workspace", "abstract-solutions.code-workspace"]) {
    const workspace = JSON.parse(fs.readFileSync(path.join(exercises, name), "utf8"));
    assert.deepEqual(workspace.settings?.["files.associations"], { "*.ab": "abstract", "*.abt": "abstract" });
    assert.ok(workspace.folders.length > 1, `${name} must remain a multi-root workspace`);
    for (const folder of workspace.folders) {
      assert.equal(typeof folder.path, "string");
      assert.ok(fs.statSync(path.resolve(exercises, folder.path)).isDirectory(), `${name}: missing ${folder.path}`);
    }
  }
});
