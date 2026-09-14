// JSON protocol parser that preserves number lexemes inside values.document.
// Compiler metadata remains ordinary JavaScript numbers for existing validators.
class ExactNumber {
  constructor(source) { this.source = source; Object.freeze(this); }
  toString() { return this.source; }
}

function parseProtocolJson(text) {
  let at = 0;
  const whitespace = () => { while (/\s/.test(text[at] || "")) at += 1; };
  const string = () => {
    const start = at++;
    let escaped = false;
    while (at < text.length) {
      const value = text[at++];
      if (!escaped && value === '"') return JSON.parse(text.slice(start, at));
      if (!escaped && value === "\\") escaped = true;
      else escaped = false;
    }
    throw new Error("Unterminated JSON string.");
  };
  const value = (path = "", exact = false, depth = 0) => {
    if (depth > 128) throw new Error("JSON nesting exceeds the analysis protocol limit.");
    whitespace();
    if (text[at] === '"') return string();
    if (text[at] === "[") {
      at += 1; const result = []; whitespace();
      if (text[at] === "]") { at += 1; return result; }
      for (;;) {
        result.push(value(path, exact, depth + 1)); whitespace();
        if (text[at] === "]") { at += 1; return result; }
        if (text[at++] !== ",") throw new Error("Invalid JSON array.");
      }
    }
    if (text[at] === "{") {
      at += 1; const result = {}; whitespace();
      if (text[at] === "}") { at += 1; return result; }
      for (;;) {
        whitespace(); if (text[at] !== '"') throw new Error("Invalid JSON object key.");
        const key = string(); whitespace();
        if (text[at++] !== ":") throw new Error("Invalid JSON object.");
        const childPath = path ? `${path}.${key}` : key;
        Object.defineProperty(result, key, { value: value(childPath, exact || childPath === "values.document", depth + 1),
          enumerable: true, configurable: true, writable: true });
        whitespace();
        if (text[at] === "}") { at += 1; return result; }
        if (text[at++] !== ",") throw new Error("Invalid JSON object.");
      }
    }
    for (const [literal, parsed] of [["true", true], ["false", false], ["null", null]]) {
      if (text.startsWith(literal, at)) { at += literal.length; return parsed; }
    }
    const match = text.slice(at).match(/^-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?/);
    if (!match) throw new Error("Invalid JSON value.");
    at += match[0].length;
    return exact ? new ExactNumber(match[0]) : Number(match[0]);
  };
  const result = value(); whitespace();
  if (at !== text.length) throw new Error("Trailing data after JSON value.");
  return result;
}

module.exports = { ExactNumber, parseProtocolJson };
