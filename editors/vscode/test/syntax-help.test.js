const test = require("node:test");
const assert = require("node:assert/strict");
const { findSyntaxHelp } = require("../src/syntax-help");

function occurrence(source, needle, index = 0) {
  let at = -1;
  for (let i = 0; i <= index; i += 1) at = source.indexOf(needle, at + 1);
  assert.notEqual(at, -1, `fixture is missing ${needle} occurrence ${index}`);
  return at;
}

function help(source, needle, index = 0, inside = 0) {
  return findSyntaxHelp(source, occurrence(source, needle, index) + inside);
}

test("documents declarations, schema types, and exact field modifiers", () => {
  const source = `versions 1..3
schema Product {
title: text(1..80) @public
count: int(0..10)
ratio: float
ready: bool @optional
state: enum(draft, active)
manual: file(pdf)
icon: image(png 64x64)
owner: ref(Person)
detail: $(Detail) @since(2) @removed(3)
items[2..4] {
kind: enum(book, video) @tag
}
}
logic Product {
}
`;
  const cases = [
    ["versions", 0, "versions"], ["schema", 0, "schema"], ["text", 0, "text"], ["int", 0, "int"],
    ["float", 0, "float"], ["bool", 0, "bool"], ["enum", 0, "enum"], ["file", 0, "file"],
    ["image", 0, "image"], ["ref", 0, "ref"], ["$(Detail)", 0, "nested"], ["@public", 0, "public"],
    ["@optional", 0, "optional"], ["@since(2)", 0, "since"], ["@removed(3)", 0, "removed"],
    ["[2..4]", 0, "cardinality"], ["@tag", 0, "tag"], ["logic", 0, "logic"],
  ];
  for (const [needle, index, key] of cases) assert.equal(help(source, needle, index, 1)?.key, key, needle);
});

test("documents logic only in statement and expression roles", () => {
  const source = `schema Product {
items[]: text @optional
price: int
label: text @optional
}
logic Product {
derive .label = prefix-$item
derive? .label = $item
require not .items contains banned and length(.items) >= version else throw "Bad $item"
if .price == 0 or .items exists {
for $item in .items {
derive .label = $item
}
} else {
derive .label = hidden
}
}
`;
  const cases = [
    ["derive ", 0, "derive"], ["derive?", 0, "deriveOptional"], ["require", 0, "require"],
    ["not", 0, "not"], ["contains", 0, "contains"], ["and", 0, "and"], ["length", 0, "length"],
    [">=", 0, "comparison"], ["version", 0, "version"], ["else throw", 0, "else"], ["throw", 0, "throw"],
    ["if", 0, "if"], ["==", 0, "comparison"], ["or", 0, "or"], ["exists", 0, "exists"],
    ["for", 0, "for"], [" in ", 0, "in"], ["$item", 3, "logicVariable"], [".items", 2, "logicPath"],
  ];
  for (const [needle, index, key] of cases) {
    const token = needle.trim();
    const result = help(source, needle, index, needle.indexOf(token));
    assert.equal(result?.key, key, `${needle} occurrence ${index}`);
  }
  assert.equal(help(source, "$item", 0, 1)?.key, "interpolation", "embedded derive value is interpolation");
  assert.equal(help(source, "$item", 1, 1)?.key, "logicVariable", "leading derive operand is a loop variable");
  assert.equal(help(source, "$item", 2, 1)?.key, "interpolation", "throw messages interpolate");
  assert.equal(help(source, "} else", 0, 3)?.key, "else");
});

test("documents instance sigils and compact structures with exact ranges", () => {
  const source = `Product :: @id.atlas, @featured @since(2)
&base_product.*
limits.{soft, hard}: 10
items(key, label): [(a, First), (b, Second)]
tags: [core, public]
entry: #book(title: Abstract Guide)
owner {
team: Knowledge Systems
}
label: ${'${id}'}_display
price: $$99
`;
  const cases = [
    ["::", "header"], ["@id.atlas", "identity"], ["@featured", "headerTag"], ["@since(2)", "since"],
    ["&base_product.*", "clone"], [".{soft, hard}", "multiPath"], ["(key, label)", "tuple"],
    ["[core, public]", "list"], ["#book", "tagObject"],
    ["${id}", "interpolation"], ["$$", "interpolation"],
  ];
  for (const [needle, key] of cases) {
    const result = help(source, needle, 0, Math.min(1, needle.length - 1));
    assert.equal(result?.key, key, needle);
    assert.equal(source.slice(result.start, result.end), needle, `${needle} range`);
    assert.match(result.markdown, /```abstract\n[\s\S]+\n```/);
  }
  const block = help(source, "owner {", 0, "owner ".length);
  assert.equal(block?.key, "bodyBlock");
  assert.equal(source.slice(block.start, block.end), "{");
});

test("does not document comments, ordinary strings, values, identifiers, or near-miss syntax", () => {
  const source = `schema Words {
schema: text
title: text
choice: enum(schema, logic, text, derive)
note: text = schema
flag: bool
}
// logic Words { derive .title = text }
Words :: @id.words, @since
title: schema logic derive enum text :: @since
flag: true
note: "schema logic derive enum text @public @since(2)"
`;
  const absent = [
    ["schema", 1], ["schema", 2], ["logic", 0], ["text", 2], ["derive", 0], ["schema", 3],
    ["logic", 2], ["derive", 2], ["enum", 2], ["text", 4], ["::", 1], ["@since", 1],
    ["schema", 4], ["@public", 0], ["@since(2)", 0],
  ];
  for (const [needle, index] of absent) assert.equal(help(source, needle, index, 1), undefined, `${needle} occurrence ${index}`);
  assert.equal(help(source, "@since", 0, 1)?.key, "headerTag", "bare @since remains a header tag");
  assert.equal(findSyntaxHelp("schemaThing: text", 2), undefined, "identifier substrings do not match");
  assert.equal(help("schema X {\nvalue: text @optionalAnything\n}\n", "@optionalAnything", 0, 1), undefined);
});

test("interpolation context distinguishes schema text, values, conditions, and loop operands", () => {
  const source = `schema Product {
label: text = prefix-$id
}
logic Product {
derive .label = "prefix-$id"
if .label == "$id" {
require .label exists else throw "Missing $id"
}
}
Product :: @id.x
label: "prefix-$id"
`;
  assert.equal(help(source, "$id", 0, 1), undefined, "schema-file text is not an interpolation site");
  assert.equal(help(source, "$id", 1, 1)?.key, "interpolation", "quoted derive values interpolate");
  assert.equal(help(source, "$id", 2, 1), undefined, "quoted conditions are literal");
  assert.equal(help(source, "$id", 3, 1)?.key, "interpolation", "throw messages interpolate");
  assert.equal(help(source, "$id", 4, 1)?.key, "interpolation", "quoted instance values interpolate");
});

test("repeated spellings select the actual grammar capture", () => {
  const source = `schema schema {
text: text
enum_value: enum(text, int)
}
`;
  assert.equal(help(source, "schema", 0, 1)?.key, "schema");
  assert.equal(help(source, "schema", 1, 1), undefined, "schema name is not the keyword");
  assert.equal(help(source, "text", 0, 1), undefined, "field name is not the type");
  assert.equal(help(source, "text", 1, 1)?.key, "text");
  assert.equal(help(source, "text", 2, 1), undefined, "enum member is not a type");
  assert.equal(help(source, "int", 0, 1), undefined, "enum member is not a type");
});
