const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("fs/promises");
const path = require("path");
const { ProjectIndex, discoveryRoot, discover } = require("../src/project-index");
const { schemaFields } = require("../src/language-model");
const { temporaryWorkspace } = require("./test-workspace");

test("discovery matches data casing, ignored directories, source extensions and link cycles", async () => {
  const workspace = await temporaryWorkspace();
  try {
    const file = await workspace.write("DATA/types/Shape.AbT", "schema Shape {\nfield: bool\n}\n");
    await workspace.write("DATA/target/ignored.abt", "schema Ignored {\n}\n");
    await workspace.write("DATA/.hidden/ignored.ab", "Shape :: @id.hidden\n");
    const root = await discoveryRoot(workspace.root);
    assert.equal(path.basename(root), "DATA");
    await fs.symlink(root, path.join(root, "cycle"), process.platform === "win32" ? "junction" : "dir");
    assert.deepEqual(await discover(root), [await fs.realpath(file)]);
  } finally { await workspace.dispose(); }
});

test("unsaved and newly created buffers replace disk sources without crossing project boundaries", async () => {
  const workspace = await temporaryWorkspace();
  try {
    const file = await workspace.write("data/Shape.abt", "schema Shape {\nold: bool\n}\n");
    const projects = new ProjectIndex();
    const index = await projects.get(workspace.root, [
      { file, text: "schema Shape {\nfresh: text\n}\n" },
      { file: path.join(workspace.root, "data/New.abt"), text: "schema New {\n}\n" },
      { file: path.join(workspace.root, "elsewhere/Other.abt"), text: "schema Other {\n}\n" }
    ]);
    assert.deepEqual(schemaFields(index, "Shape").map((f) => f.key), ["fresh"]);
    assert.ok(index.schemas.has("New"));
    assert.ok(!index.schemas.has("Other"));
    await workspace.write("data/Shape.abt", "schema Shape {\nsaved: int\n}\n");
    projects.invalidate();
    assert.deepEqual(schemaFields(await projects.get(workspace.root), "Shape").map((f) => f.key), ["saved"]);
  } finally { await workspace.dispose(); }
});
