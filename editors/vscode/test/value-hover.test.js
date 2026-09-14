const test = require("node:test");
const assert = require("node:assert/strict");
const protocol = require("../src/analysis-client");
const hover = require("../src/value-hover");

function response(document) {
  return protocol.response({ cancelled: false, error: null, stderr: "", stdout: JSON.stringify({
    protocol: "abstract-analysis", version: 1, requestId: 7, positionEncoding: "utf-16",
    analyzed: true, truncated: false, diagnostics: [], values: { version: 1, complete: true, document }
  }).replace('9223372036854776000', '9223372036854775807').replace('"NEGATIVE_ZERO"', '-0.0') }, 7);
}

test("compiled hover projects base and overlays while preserving exact numeric lexemes", () => {
  const parsed = response({
    abstract: { format: 1, compiler: "1.3.0", versions: { min: 1, max: 3 } },
    data: [{ template: "Car", id: "demo", power: 3, huge: 9223372036854776000, balance: "NEGATIVE_ZERO" }],
    overlays: [
      { versions: { min: 1, max: 1 }, data: [{ template: "Car", id: "demo", power: 1 }], removed: [] },
      { versions: { min: 2, max: 2 }, data: [], removed: ["demo"] }
    ]
  });
  const compiled = hover.compiledValues(parsed);
  assert.deepEqual(hover.project(compiled, "demo", ["power"]).map(({ min, max, state }) =>
    [min, max, state.kind, state.key]), [[1, 1, "value", "1"], [2, 2, "instance-absent", undefined], [3, 3, "value", "3"]]);
  assert.match(hover.markdown(compiled, "demo", ["huge"]), /9223372036854775807/);
  assert.match(hover.markdown(compiled, "demo", ["balance"]), /-0\.0/);
});

test("compiled hover rejects overlapping per-instance overlays", () => {
  const parsed = response({
    abstract: { format: 1, compiler: "1.3.0", versions: { min: 1, max: 3 } },
    data: [{ template: "Car", id: "demo", power: 3 }],
    overlays: [
      { versions: { min: 1, max: 2 }, data: [{ template: "Car", id: "demo", power: 1 }], removed: [] },
      { versions: { min: 2, max: 2 }, data: [], removed: ["demo"] }
    ]
  });
  assert.throws(() => hover.project(hover.compiledValues(parsed), "demo", ["power"]), /Overlapping/);
});
