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
    const publicTokens = tokenize('  caption: text @public = "@public" // @public');
    const publicModifiers = publicTokens.filter((token) => token.scopes.includes("storage.modifier.schema.abstract"));
    assert.equal(publicModifiers.length, 1, "the declaration marker, not its quoted value or comment");
    assert.equal(publicModifiers[0].startIndex, 16);
    tokenize("}");
    const url = tokenize("link: https://example.com // actual comment");
    assert.equal(url.find((t) => t.scopes.includes("comment.line.double-slash.abstract")).startIndex, 26);
    tokenize('name: "unfinished');
    const header = tokenize("Shape :: @id.x");
    assert.ok(header.some((token) => token.scopes.includes("entity.name.type.instance.abstract")));
  } finally { registry.dispose(); }
});

test("TextMate distinguishes instance keys, tuple columns, and bare values across nested and multiline bodies", async () => {
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
    const scopedText = (line, scope) => {
      const tokens = tokenize(line);
      return tokens.filter((token) => token.scopes.includes(scope)).map((token) => line.slice(token.startIndex, token.endIndex));
    };

    tokenize("Product :: @id.atlas");
    assert.deepEqual(scopedText("owner {", "variable.other.field.instance.abstract"), ["owner"]);
    assert.deepEqual(scopedText("  team: Knowledge Systems", "variable.other.field.instance.abstract"), ["team"]);
    state = INITIAL;
    tokenize("Product :: @id.atlas");
    const tupleHeader = "copy(key, value): [";
    const tupleTokens = tokenize(tupleHeader);
    assert.deepEqual(tupleTokens.filter((t) => t.scopes.includes("variable.other.field.instance.abstract")).map((t) => tupleHeader.slice(t.startIndex, t.endIndex)), ["copy"]);
    assert.deepEqual(tupleTokens.filter((t) => t.scopes.includes("variable.parameter.tuple.abstract")).map((t) => tupleHeader.slice(t.startIndex, t.endIndex)), ["key", "value"]);
    assert.deepEqual(scopedText("  (en_us, Welcome),", "string.unquoted.bare.abstract"), ["en_us", "Welcome"]);
    assert.deepEqual(scopedText("  (es_es, Bienvenido),", "string.unquoted.bare.abstract"), ["es_es", "Bienvenido"]);
    const scalar = "status: finished";
    const scalarTokens = tokenize(scalar);
    assert.ok(scalarTokens.some((t) => t.scopes.includes("variable.other.field.instance.abstract") && scalar.slice(t.startIndex, t.endIndex) === "status"));
    assert.ok(scalarTokens.some((t) => t.scopes.includes("string.unquoted.bare.abstract") && scalar.slice(t.startIndex, t.endIndex) === "finished"));
    const preservationCases = [
      ["asset: ./images/hero.png // retained", "string.unquoted.path.file.abstract"],
      ["label: ${id}_fleet", "variable.other.interpolation.abstract"],
      ["count: 12", "constant.numeric.integer.abstract"],
      ["enabled: true", "constant.language.boolean.abstract"],
      ["channel: beta @since(2)", "storage.modifier.version.abstract"]
    ];
    for (const [line, scope] of preservationCases) {
      const tokens = tokenize(line);
      assert.ok(tokens.some((t) => t.scopes.includes(scope)), `${scope} on ${line}`);
    }
    const tupleComment = "  (es_mx, Hola) // retained";
    assert.ok(tokenize(tupleComment).some((t) => t.scopes.includes("comment.line.double-slash.abstract")));
  } finally { registry.dispose(); }
});

test("TextMate scopes nested calc arithmetic without coloring similarly named data", async () => {
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
    tokenize("logic Invoice {");
    const line = "derive .total = calc(sum(.lines.total) + max(abs($delta), length(.lines)))";
    const tokens = tokenize(line);
    const texts = (scope) => tokens.filter((token) => token.scopes.includes(scope)).map((token) => line.slice(token.startIndex, token.endIndex));
    assert.deepEqual(texts("support.function.calculation.abstract"), ["calc"]);
    assert.deepEqual(texts("support.function.numeric.abstract"), ["sum", "max", "abs", "length"]);
    assert.deepEqual(texts("keyword.operator.arithmetic.abstract"), ["+"]);
    assert.ok(texts("variable.other.path.abstract").includes(".lines.total"));
    tokenize("}");
    const data = "note: calc(sum + max)";
    const dataTokens = tokenize(data);
    assert.equal(dataTokens.some((token) => token.scopes.includes("support.function.numeric.abstract")), false);
    assert.ok(dataTokens.some((token) => token.scopes.includes("string.unquoted.bare.abstract")));
  } finally { registry.dispose(); }
});

test("TextMate scopes the contact screenshot as Abstract instance keys and whole bare values", async () => {
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
    tokenize("Person :: @id.ada");
    for (const [line, key, value] of [
      ["contact.email: ada@example.test", "contact.email", "ada@example.test"],
      ["contact.city: London", "contact.city", "London"],
      ["name: Ada", "name", "Ada"]
    ]) {
      const tokens = tokenize(line);
      assert.deepEqual(tokens.filter((token) => token.scopes.includes("variable.other.field.instance.abstract"))
        .map((token) => line.slice(token.startIndex, token.endIndex)), [key]);
      assert.deepEqual(tokens.filter((token) => token.scopes.includes("string.unquoted.bare.abstract"))
        .map((token) => line.slice(token.startIndex, token.endIndex)), [value]);
      assert.equal(tokens.some((token) => token.scopes.includes("storage.modifier.header.abstract")), false);
      assert.equal(tokens.some((token) => token.scopes.includes("variable.other.path.abstract")), false);
    }
  } finally { registry.dispose(); }
});
