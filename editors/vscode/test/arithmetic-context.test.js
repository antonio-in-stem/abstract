const test = require("node:test");
const assert = require("node:assert/strict");
const { calcRegionAt, calcRegions, canStartCalcAt } = require("../src/arithmetic-context");

test("recognizes complete derive calculations and condition operands", () => {
  const derive = "derive .total = calc((.price - .discount) * .quantity)";
  assert.equal(calcRegions(derive)[0].role, "derive");
  assert.ok(calcRegionAt(derive, derive.indexOf(".price")));
  const condition = "require calc(.total / 100) <= 500 else throw             ";
  assert.equal(calcRegions(condition)[0].role, "condition");
});

test("rejects near misses and supports an unfinished expression at the cursor", () => {
  for (const value of [
    "derive .x = Calc(.x + 1)",
    "derive .x = calc (.x + 1)",
    "derive .x = prefix calc(.x + 1)",
    "derive .x = calc(.x + 1) suffix",
    "note: calc(.x + 1)"
  ]) assert.deepEqual(calcRegions(value), [], value);
  const unfinished = "derive .x = calc(ro";
  assert.ok(calcRegionAt(unfinished, unfinished.length));
});

test("offers calc only where a numeric expression operand may begin", () => {
  for (const [value, at] of [
    ["derive .x = ca", 12],
    ["require ca <= 5 else throw             ", 8],
    ["require .x <= ca else throw             ", 14]
  ]) assert.equal(canStartCalcAt(value, at), true, value);
  assert.equal(canStartCalcAt("derive .x = prefix ca", 19), false);
  assert.equal(canStartCalcAt("note: ca", 6), false);
});
