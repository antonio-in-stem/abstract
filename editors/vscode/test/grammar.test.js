const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("fs");
const path = require("path");
const { Registry, INITIAL } = require("vscode-textmate");
const onig = require("vscode-oniguruma");

test("TextMate highlights legal numeric/hyphen fields and recovers after an unterminated physical string", async () => {
  const wasm = fs.readFileSync(require.resolve("vscode-oniguruma/release/onig.wasm"));
  await onig.loadWASM(wasm.buffer.slice(wasm.byteOffset, wasm.byteOffset + wasm.byteLength));
  const registry = new Registry({
    onigLib: Promise.resolve({ createOnigScanner: (s) => new onig.OnigScanner(s), createOnigString: (s) => new onig.OnigString(s) }),
    loadGrammar: async () => JSON.parse(fs.readFileSync(path.join(__dirname, "../syntaxes/abstract.tmLanguage.json"), "utf8"))
  });
  try {
    const grammar = await registry.loadGrammar("source.abstract");
    let state = INITIAL;
    const tokenize = (line) => { const result = grammar.tokenizeLine(line, state); state = result.ruleStack; return result.tokens; };
    tokenize("schema Shape {");
    for (const line of ["  Max-Count: int", "  07: bool"]) {
      const tokens = tokenize(line);
      assert.ok(tokens.some((token) => token.scopes.includes("variable.other.field.schema.abstract")));
    }
    tokenize("}");
    const url = tokenize("link: https://example.com // actual comment");
    assert.equal(url.find((t) => t.scopes.includes("comment.line.double-slash.abstract")).startIndex, 26);
    tokenize('name: "unfinished');
    const header = tokenize("Shape :: @id.x");
    assert.ok(header.some((token) => token.scopes.includes("entity.name.type.instance.abstract")));
  } finally { registry.dispose(); }
});
