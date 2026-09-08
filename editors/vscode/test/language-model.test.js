const { test } = require("node:test");
const assert = require("node:assert/strict");
const model = require("../src/language-model");

const schema = `schema Owner {
  team: text
  07: int @optional
}
schema Pack {
  label: text
}
schema Product {
  id: text(1..64)
  Max-Count: int(1..4)
  enabled: bool
  status: enum(New, In-Progress, done)
  owner: $(Owner)
  pack: ref(Pack)
  capabilities[] {
    key: enum(search, sync) @tag
    active: bool = true
  }
}
`;
const source = (file, text) => ({ file, text });
function fixture(marked, template = schema, file = "product.ab") {
  const offset = marked.indexOf("|");
  const text = marked.replace("|", "");
  const index = model.createIndex([source("templates.abt", template), source("pack.ab", "Pack :: @id.winter\nlabel: Winter\n"), source(file, text)]);
  return { index, file, offset, text };
}
const labels = (f) => model.completions(f.index, f.file, f.offset).map((item) => item.label);

test("lexical masking keeps URLs and handles even and odd escape runs", () => {
  assert.equal(model.scanLine("url: https://example.com // comment").masked.trim(), "url: https://example.com");
  assert.equal(model.scanLine('x: "\\\\" // comment').inString, false);
  assert.equal(model.scanLine('x: "a\\\"b" // comment').inString, false);
  assert.equal(model.scanLine('x: "unterminated').inString, true);
});

test("schemas preserve exact names, normalize identifiers, list cardinalities and nested shapes", () => {
  const index = fixture("").index;
  assert.equal(model.resolveField(index, "Product", ["max_count"]).type, "int(1..4)");
  assert.equal(model.resolveField(index, "Product", ["OWNER", "07"]).type, "int");
  assert.equal(model.resolveField(index, "product", ["status"]), undefined);
  assert.equal(model.resolveField(index, "Product", ["capabilities", "key"]), undefined);
  assert.equal(model.parseSource("fallback.ab", "Pack :: @ID.Winter-Pack\n").instances[0].name, "winter_pack");
});

test("root and nested field completion obey scope and forbid list traversal", () => {
  assert.deepEqual(labels(fixture("Product :: @id.x\n|")), ["max_count", "enabled", "status", "owner", "pack", "capabilities"]);
  assert.deepEqual(labels(fixture("Product :: @id.x\nowner.|")), ["team", "07"]);
  assert.deepEqual(labels(fixture("Product :: @id.x\nowner {\n |\n}")), ["team", "07"]);
  assert.deepEqual(labels(fixture("Product :: @id.x\ncapabilities.|")), []);
});

test("schema-directed values include normalized enums, bool, ref ids and #tag members", () => {
  assert.deepEqual(labels(fixture("Product :: @id.x\nstatus: |")), ["new", "in_progress", "done"]);
  assert.deepEqual(labels(fixture("Product :: @id.x\nenabled: |")), ["true", "false"]);
  assert.deepEqual(labels(fixture("Product :: @id.x\npack: |")), ["winter"]);
  assert.deepEqual(labels(fixture("Product :: @id.x\ncapabilities: [#|")), ["search", "sync"]);
  assert.deepEqual(labels(fixture("Product :: @id.x, @status.|")), ["new", "in_progress", "done"]);
  assert.ok(!labels(fixture("Product :: @id.x, @|")).includes("owner"));
});

test("strings and comments do not offer unrelated fields", () => {
  assert.deepEqual(labels(fixture('Product :: @id.x\nstatus: "|"')), []);
  assert.deepEqual(labels(fixture("Product :: @id.x\n// |")), []);
  assert.deepEqual(labels(fixture("Product :: @id.x\nowner.team: arbitrary |")), []);
});

test("current buffer and ambiguous schema declarations are handled conservatively", () => {
  const f = fixture("Product :: @id.x\n|");
  f.index = model.createIndex([source("schema.abt", schema), source("duplicate.abt", "schema Product {\nother: text\n}\n"), source(f.file, f.text)]);
  assert.deepEqual(labels(f), []);
});

test("definitions resolve exact schema, normalized nested fields and ref ids", () => {
  for (const [marked, name, file] of [
    ["Pro|duct :: @id.x\n", "Product", "templates.abt"],
    ["Product :: @id.x\nOWNER.te|am: example\n", "team", "templates.abt"],
    ["Product :: @id.x\npack: wi|nter\n", "winter", "pack.ab"]
  ]) {
    const f = fixture(marked);
    const [found] = model.definitions(f.index, f.file, f.offset);
    assert.equal(found?.name, name);
    assert.equal(found?.file, file);
  }
});

test("completion replaces the whole token and preserves existing punctuation", () => {
  const f = fixture("Product :: @id.x\nsta|tus: done");
  const item = model.completions(f.index, f.file, f.offset).find((i) => i.label === "status");
  assert.equal(f.text.slice(item.from, item.to), "status");
  assert.equal(item.insert, "status");
});

test("multiline enum declarations and header continuation stay one statement", () => {
  const template = "schema Product {\n status: enum(\n A,\n B\n )\n}\n";
  assert.deepEqual(labels(fixture("Product :: @id.x,\n @status.|", template)), ["a", "b"]);
});

test("type and modifier completion does not invent callable primitive syntax", () => {
  const f = fixture("schema New {\nfield: |\n}", "", "schema.abt");
  const items = model.completions(f.index, f.file, f.offset);
  assert.equal(items.find((item) => item.label === "bool").insert, "bool");
  assert.equal(items.find((item) => item.label === "enum").insert, "enum($1)");
  assert.ok(labels(fixture("schema New {\ngroup {\nx: text @|\n}\n}", "", "schema.abt")).includes("tag"));
  assert.ok(!labels(fixture("schema New {\nx: text @|\n}", "", "schema.abt")).includes("tag"));
});

test("public nomination is contextual, non-recursive and distinct from runtime eligibility", () => {
  const marked = fixture("schema New {\nfield: text @pu|\n}", "", "schema.abt");
  const item = model.completions(marked.index, marked.file, marked.offset).find((entry) => entry.label === "public");
  assert.equal(item.insert, "public");
  assert.match(item.detail, /Runtime bindings are still required/);
  assert.ok(!labels(fixture("schema New {\nfield: text @public @|\n}", "", "schema.abt")).includes("public"));
  for (const source of ['schema New {\nfield: text = "@pu|"\n}', "schema New {\n// @pu|\n}", "Product :: @id.x\nlabel: @pu|"])
    assert.ok(!labels(fixture(source, "", source.startsWith("schema") ? "schema.abt" : "product.ab")).includes("public"));
  const text = 'schema New {\ngroup @public {\nchild: text\n}\nlabel: text @public = "@public"\nprivate: text = @public\nquoted: text = "@public"\ncomment: text // @public\n}\n';
  const index = model.createIndex([source("schema.abt", text)]);
  assert.equal(model.resolveField(index, "New", ["group"]).public, true);
  assert.equal(model.resolveField(index, "New", ["group", "child"]).public, false);
  assert.equal(model.resolveField(index, "New", ["label"]).public, true);
  for (const field of ["private", "quoted", "comment"])
    assert.equal(model.resolveField(index, "New", [field]).public, false, field);
});

test("nested logic and else bodies keep the schema context for dotted paths", () => {
  const f = fixture("logic Product {\n if .enabled {\n  derive .status: active\n } else {\n  require .owner.|\n }\n}\n", schema, "logic.abt");
  assert.deepEqual(labels(f), ["team", "07"]);
});

test("dotted-path refactor preserves annotation and value bytes, skips comments on braces and list paths", () => {
  const f = fixture('Product :: @id.x\n  owner {\n    team: "a, b" @since(2) // comment\n  }\n|');
  const [edit] = model.abbreviations(f.index, f.file);
  assert.equal(edit.replacement, '  owner.team: "a, b" @since(2) // comment');
  for (const body of ["owner { // comment\nteam: x\n}", "owner {\nteam: x\n} // comment", "capabilities {\nactive: true\n}", "owner {\n// keep\nteam: x\n}"]) {
    const invalid = fixture(`Product :: @id.x\n${body}\n|`);
    assert.equal(model.abbreviations(invalid.index, invalid.file).length, 0);
  }
});
