const { test } = require("node:test");
const assert = require("node:assert/strict");
const { formatDocument } = require("../src/formatter");

test("formatter indents declarations, instances, blocks and continuations without changing tokens", () => {
  const source = "schema Item {\r\nname: text\r\ncopy[] {\r\nkey: enum(\r\nen,\r\nes\r\n) @tag\r\n}\r\n}\r\nItem :: @id.x,\r\n@name.X\r\ncopy(key, value): [\r\n(en, Hello),\r\n]\r\n";
  const expected = "schema Item {\r\n    name: text\r\n    copy[] {\r\n        key: enum(\r\n            en,\r\n            es\r\n        ) @tag\r\n    }\r\n}\r\nItem :: @id.x,\r\n    @name.X\r\n    copy(key, value): [\r\n        (en, Hello),\r\n    ]\r\n";
  assert.equal(formatDocument(source, { insertSpaces: true, tabSize: 4 }), expected);
  assert.equal(formatDocument(expected, { insertSpaces: true, tabSize: 4 }), expected);
});

test("formatter distinguishes body braces from multipaths and brace values", () => {
  const source = "Item :: @id.x\nlimits.{\nsoft,\nhard\n}: 10\nfiles: [./{a,b}.png]\n";
  assert.equal(formatDocument(source, { insertSpaces: false, tabSize: 8 }),
    "Item :: @id.x\n\tlimits.{\n\t\tsoft,\n\t\thard\n\t}: 10\n\tfiles: [./{a,b}.png]\n");
});

test("formatter retains the instance body offset inside nested field blocks", () => {
  const input = [
    "Car :: @id demo",
    "stats {",
    "power: 12",
    "}",
    "name: demo",
    "",
  ].join("\n");
  assert.equal(formatDocument(input, { insertSpaces: true, tabSize: 2 }), [
    "Car :: @id demo",
    "  stats {",
    "    power: 12",
    "  }",
    "  name: demo",
    "",
  ].join("\n"));
});
