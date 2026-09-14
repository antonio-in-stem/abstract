// Syntax-only recognition for Abstract 1.2 calculation islands. Callers pass
// text with strings and comments masked so authored prose cannot become code.
const CALC_FUNCTIONS = ["abs", "min", "max", "clamp", "div", "round", "floor", "ceil", "sqrt", "pow", "sum", "avg", "length"];

function conditionBounds(mask) {
  let match = /^\s*require\b/.exec(mask);
  if (match) {
    const end = mask.search(/\belse\s+throw\b/);
    return { start: match[0].length, end: end < 0 ? mask.length : end };
  }
  match = /^\s*(?:}\s*else\s+|else\s+)?if\b/.exec(mask);
  if (match) {
    const end = mask.lastIndexOf("{");
    return { start: match[0].length, end: end < match[0].length ? mask.length : end };
  }
  return undefined;
}

function validStart(mask, start) {
  const before = mask.slice(0, start);
  if (/^\s*derive\??\s+[^=]+?=\s*$/.test(before)) return "derive";
  const condition = conditionBounds(mask);
  return condition && start >= condition.start && start < condition.end ? "condition" : undefined;
}

function calcRegions(mask) {
  const regions = [];
  for (const match of mask.matchAll(/\bcalc\(/g)) {
    const start = match.index;
    const role = validStart(mask, start);
    if (!role) continue;
    let depth = 1;
    let end = mask.length;
    for (let i = start + match[0].length; i < mask.length; i += 1) {
      if (mask[i] === "(") depth += 1;
      else if (mask[i] === ")" && --depth === 0) { end = i + 1; break; }
    }
    const closed = depth === 0;
    if (role === "derive" && closed
        && !/^(?:\s+@(?:since|removed)\(\d+\)){0,2}\s*$/.test(mask.slice(end))) continue;
    regions.push({ start, open: start + 4, contentStart: start + 5, end, closed, role });
  }
  return regions;
}

function calcRegionAt(mask, offset, includeWrapper = false) {
  return calcRegions(mask).find((region) => offset >= (includeWrapper ? region.start : region.contentStart)
    && offset <= (includeWrapper ? region.end - 1 : region.closed ? region.end - 1 : region.end));
}

function canStartCalcAt(mask, offset) {
  const before = mask.slice(0, offset);
  if (/^\s*derive\??\s+[^=]+?=\s*$/.test(before)) return true;
  const condition = conditionBounds(mask);
  if (!condition || offset < condition.start || offset > condition.end) return false;
  const operandPrefix = before.slice(condition.start);
  return /^\s*$/.test(operandPrefix)
    || /(?:==|!=|>=|<=|>|<|\b(?:and|or|not)\b)\s*$/.test(operandPrefix);
}

module.exports = { CALC_FUNCTIONS, calcRegions, calcRegionAt, canStartCalcAt };
