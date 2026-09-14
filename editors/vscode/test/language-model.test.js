const { test } = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");
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

test("dotted-path refactor preserves values and comments, flattens repeated prefixes, and skips unsafe paths", () => {
  const f = fixture('Product :: @id.x\n  owner {\n    team: "a, b" @since(2) // comment\n  }\n|');
  const [edit] = model.abbreviations(f.index, f.file);
  assert.equal(edit.replacement, '  owner.team: "a, b" @since(2) // comment');
  const multiple = fixture("Product :: @id.x\nowner {\n // keep\n team: x\n 07: 2\n}\n|");
  assert.equal(model.abbreviations(multiple.index, multiple.file)[0].replacement,
    " // keep\nowner.team: x\nowner.07: 2");
  for (const body of ["owner { // comment\nteam: x\n}", "owner {\nteam: x\n} // comment", "capabilities {\nactive: true\n}"]) {
    const invalid = fixture(`Product :: @id.x\n${body}\n|`);
    assert.equal(model.abbreviations(invalid.index, invalid.file).length, 0);
  }
});

test("numeric function completion is confined to explicit calc expressions", () => {
  const inside = fixture("logic Product {\n derive .max_count = calc(ro|)\n}\n", schema, "logic.abt");
  const items = model.completions(inside.index, inside.file, inside.offset);
  assert.ok(items.some((item) => item.label === "round" && item.kind === "Function"));
  assert.ok(items.some((item) => item.label === "sum"));
  assert.equal(items.find((item) => item.label === "pow").insert, "pow(${1:base}, ${2:integerExponent})");
  assert.ok(items.some((item) => item.label === "length"));
  assert.ok(labels(fixture("logic Product {\n derive .max_count = ro|\n}\n", schema, "logic.abt")).every((label) => label !== "round"));
  assert.ok(labels(fixture("Product :: @id.x\nlabel: calc(ro|)\n", schema)).every((label) => label !== "round"));
  assert.ok(labels(fixture('Product :: @id.x\nlabel: "calc(ro|)"\n', schema)).every((label) => label !== "round"));
  const start = fixture("logic Product {\n derive .max_count = ca|\n}\n", schema, "logic.abt");
  assert.equal(model.completions(start.index, start.file, start.offset)[0].insert, "calc(${1:expression})");
  assert.ok(labels(fixture("Product :: @id.x\nlabel: ca|\n", schema)).every((label) => label !== "calc"));
});

test("definitions resolve field declarations for schema hover metadata", () => {
  const file = "schema-hover.abt";
  const text = "schema Car {\npower: int = 10\nstats {\nlegacy: int @removed(3)\n}\n}\n";
  const index = model.createIndex([{ file, text }]);
  const power = model.definitions(index, file, text.indexOf("power") + 2);
  assert.equal(power[0].default, "10");
  const legacy = model.definitions(index, file, text.indexOf("legacy") + 2);
  assert.equal(legacy[0].removed, 3);
});

test("tuple columns/cells, tag arguments, assets and loop element fields complete from schema shape", () => {
  const extended = `${schema}\nschema Rich {\nassets[] {\nkind: enum(icon, model) @tag\nenabled: bool\nnested {\nlabel: text\n}\n}\nimage: image(png 16x16)\n}\n`;
  const complete = (marked, file = "rich.ab") => {
    const value = fixture(marked, extended, file);
    value.index.assets = ["./textures/icon.png", "./notes/readme.txt"];
    return labels(value);
  };
  assert.deepEqual(complete("Rich :: @id.x\nassets(ki|): (icon)"), ["kind", "enabled", "nested"]);
  assert.deepEqual(complete("Rich :: @id.x\nassets(kind, enabled): (icon, |)"), ["true", "false"]);
  assert.deepEqual(complete("Rich :: @id.x\nassets: [#icon(en|)]"), ["enabled", "nested"]);
  assert.deepEqual(complete("Rich :: @id.x\nassets: [#icon(enabled: |)]"), ["true", "false"]);
  assert.deepEqual(complete("Rich :: @id.x\nimage: ./tex|"), ["./textures/icon.png"]);
  assert.deepEqual(complete("logic Rich {\nfor $asset in .assets {\nrequire $asset.|\n}\n}", "rich.abt"),
    ["kind", "enabled", "nested"]);
});

test("instance hover target follows nested body prefixes and authored dotted segments", () => {
  const file = path.resolve("hover.ab");
  const text = "Car :: @id.demo\nstats {\npower: 12\n}\nstats.speed: 3\n";
  const index = model.createIndex([{ file, text }]);
  assert.deepEqual(model.instanceFieldTarget(index, file, text.indexOf("power") + 2), { id: "demo", path: ["stats", "power"] });
  assert.deepEqual(model.instanceFieldTarget(index, file, text.indexOf("speed") + 2), { id: "demo", path: ["stats", "speed"] });
});
