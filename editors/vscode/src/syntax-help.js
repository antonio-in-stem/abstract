// Offline, syntax-only hover help for Abstract 1.x. The matcher is deliberately
// conservative: a spelling gets help only when its surrounding grammar role is
// clear, so prose values and user identifiers do not masquerade as keywords.
const { parseSource, scanLine } = require("./language-model");

const ID = "[A-Za-z0-9_][A-Za-z0-9_-]*";
const SCHEMA = "[A-Za-z][A-Za-z0-9_]*";
const TYPE = `(?:\\$\\(${SCHEMA}\\)|(?:text|int|float|enum|file|image|ref)(?:\\([^)]*\\))?|bool\\b)`;

const HELP = {
  schema: ["Schema declaration", "Declares the fields and validation rules for a named object shape.", "schema Product {\n    name: text\n}"],
  logic: ["Logic declaration", "Attaches ordered validation and derivation rules to a declared schema.", "logic Product {\n    require .price >= 0 else throw \"Price must be non-negative.\"\n}"],
  versions: ["Project versions", "Declares the closed range of versions the project compiles. Without it, the range is `1..1`.", "versions 1..3"],
  text: ["`text` type", "Accepts quoted or bare text. Optional ranges constrain the number of Unicode scalar values.", "title: text(1..80)"],
  int: ["`int` type", "Accepts unquoted integers. Optional ranges constrain the numeric value.", "priority: int(1..5)"],
  float: ["`float` type", "Accepts unquoted integers or decimal/exponent numbers. Optional ranges constrain the numeric value.", "price: float(0..9999.99)"],
  bool: ["`bool` type", "Accepts exactly the unquoted values `true` or `false`.", "featured: bool = false"],
  enum: ["`enum` type", "Restricts a value to one of the declared, normalized members.", "status: enum(draft, active, retired)"],
  file: ["`file` type", "Accepts an asset path with one of the listed extensions, resolved under the project `assets` directory.", "manual: file(pdf, txt)"],
  image: ["`image` type", "Accepts an asset image with an allowed format and optional pixel-size constraint.", "icon: image(png 128x128, webp *x256)"],
  ref: ["`ref` type", "Stores the normalized id of an instance of the named schema.", "owner: ref(Person)"],
  nested: ["Nested schema type", "Validates an embedded object against another schema; it does not create a referenced instance.", "owner: $(Person)"],
  optional: ["`@optional` modifier", "Omits the field from output when no authored, defaulted, or derived value supplies it.", "subtitle: text @optional"],
  tag: ["`@tag` modifier", "Marks one scalar field inside a group as the discriminator used by `#tag` shorthand and keyed-list merging.", "items[] {\n    kind: enum(book, video) @tag\n    title: text\n}"],
  public: ["`@public` modifier", "Nominates this field for an author-defined public contract. Nomination alone does not grant runtime access, mutability, or acceptance by an export profile.", "preview: bool @public = true"],
  since: ["`@since(n)` annotation", "Makes a field, body statement, or whole instance exist/apply from version `n` onward within the project range.", "glow: bool @since(2) = false"],
  removed: ["`@removed(n)` annotation", "Makes a field, body statement, or whole instance exist/apply only before version `n`.", "legacy_tint: int @removed(3) @optional"],
  header: ["Instance header", "Starts an instance of the named schema. Header tags may follow as compact root-field assignments.", "Product :: @id.atlas, @featured"],
  identity: ["Instance identity", "Sets the instance id in its header. The id is normalized and cannot be assigned in the body.", "Product :: @id.winter-pack"],
  headerTag: ["Header tag", "Compactly assigns a root field: `@name.value` assigns a value and bare `@name` assigns `true`.", "Product :: @id.atlas, @featured"],
  clone: ["Clone statement", "Copies another same-schema instance's authored data before defaults, interpolation, and logic. `.*` copies all writable fields.", "&base_product.*"],
  tagObject: ["Tag object", "Uses a group's `@tag` field as shorthand; arguments fill the remaining fields.", "items: [#book(title: Abstract Guide)]"],
  interpolation: ["Value interpolation", "Substitutes an available scalar once: an instance root scalar; in logic, a current root scalar or an in-scope scalar loop variable. Use `${name}` for an explicit boundary and `$$` for a literal dollar sign.", "label: ${id}_display"],
  cardinality: ["List cardinality", "Declares a list and optionally its minimum and maximum element count.", "tags[2..4]: text"],
  list: ["Bracketed list", "Writes a list value. Abstract also accepts a bare comma-separated list; nested lists are invalid.", "tags: [core, public]"],
  multiPath: ["Multi-path assignment", "Assigns one value to each named child under the same path prefix.", "limits.{soft, hard}: 10"],
  tuple: ["Tuple-array assignment", "Maps each tuple cell to the corresponding named column, producing list elements.", "items(key, label): [(a, First), (b, Second)]"],
  bodyBlock: ["Body block", "Uses the path as a prefix for every assignment nested inside the block.", "owner {\n    team: Knowledge Systems\n}"],
  derive: ["`derive` statement", "Writes the target path during logic execution, replacing any value already present.", "derive .shipping = standard"],
  deriveOptional: ["`derive?` statement", "Writes only when the target path is absent. A schema default is filled first, so `derive?` cannot apply to a defaulted field.", "derive? .shipping = standard"],
  require: ["`require` statement", "Stops compilation with the quoted message when its condition is false.", "require .price >= 0 else throw \"Price must be non-negative.\""],
  if: ["`if` statement", "Runs the first branch whose condition is true; optional `else if` and `else` branches may follow.", "if .featured == true {\n    derive .badge = featured\n}"],
  else: ["`else` branch", "Provides the next conditional branch or the fallback branch of an `if`; in `require`, it introduces `throw`.", "if .active {\n    derive .state = live\n} else {\n    derive .state = hidden\n}"],
  throw: ["`throw` message", "Supplies the quoted failure message for a `require`. Scalar `$variables` in this message are interpolated.", "require .owner exists else throw \"Missing owner for $id.\""],
  for: ["`for` statement", "Runs its body once for each element of a list field or literal list.", "for $item in .items {\n    require $item.label exists else throw \"Missing label.\"\n}"],
  in: ["`in` loop keyword", "Separates a loop variable from the list or literal list it iterates.", "for $item in .items {\n    derive .count = length(.items)\n}"],
  logicVariable: ["Loop variable", "Names the current element of the nearest matching `for` loop; fields are read with dotted segments.", "require $item.label exists else throw \"Missing label.\""],
  logicPath: ["Logic path", "Reads a field from the object being evaluated. A leading dot selects the root object.", "require .owner.contact exists else throw \"Missing contact.\""],
  and: ["Logical AND", "Requires both surrounding conditions to be true. `and` and `&&` are equivalent.", "if .enabled and .owner exists {\n    derive .ready = true\n}"],
  or: ["Logical OR", "Requires either surrounding condition to be true. `or` and `||` are equivalent.", "if .draft or .preview {\n    derive .visible = true\n}"],
  not: ["Logical NOT", "Negates the following comparison. It binds more loosely than comparison operators.", "require not .flags contains banned else throw \"Banned.\""],
  contains: ["`contains` operator", "Tests whether a list contains a value or text contains a substring.", "if .tags contains featured {\n    derive .visible = true\n}"],
  exists: ["`exists` operator", "Tests whether a field or indexed value is present; it does not inspect the filesystem.", "if .owner.contact exists {\n    derive .contactable = true\n}"],
  length: ["`length(…)` function", "Returns the element count of a list or the Unicode scalar count of text; an absent optional value has length zero.", "derive .tag_count = length(.tags)"],
  version: ["`version` built-in", "Evaluates to the integer project version currently being compiled.", "if version >= 2 {\n    derive .modern = true\n}"],
  comparison: ["Comparison operator", "Compares two logic operands. Available operators are `==`, `!=`, `>`, `>=`, `<`, and `<=`.", "require .price >= 0 else throw \"Negative price.\""],
};

function lineAt(text, offset) {
  const start = text.lastIndexOf("\n", Math.max(0, offset - 1)) + 1;
  let end = text.indexOf("\n", offset);
  if (end < 0) end = text.length;
  if (end > start && text[end - 1] === "\r") end -= 1;
  return { start, end, text: text.slice(start, end), offset: offset - start };
}

function regionAt(line, offset) {
  let quoted = false;
  for (let i = 0; i < line.length; i += 1) {
    if (!quoted && line[i] === "/" && line[i + 1] === "/" && (i === 0 || /[ \t]/.test(line[i - 1]))) {
      return offset >= i ? "comment" : "code";
    }
    if (quoted && line[i] === "\\") {
      if (offset === i || offset === i + 1) return "string";
      i += 1;
      continue;
    }
    if (line[i] === '"') {
      if (offset === i) return "string";
      quoted = !quoted;
      continue;
    }
    if (offset === i) return quoted ? "string" : "code";
  }
  return quoted ? "string" : "code";
}

const TOKEN_PATTERNS = [
  /@(?:since|removed)\(\d+\)/g,
  /@[A-Za-z0-9_][A-Za-z0-9_-]*(?:\.(?:"(?:\\.|[^"\\])*"|[^\s,@]+))?/g,
  /\$\([A-Za-z][A-Za-z0-9_]*\)/g,
  /\$\{[A-Za-z0-9_][A-Za-z0-9_-]*\}|\$\$|\$[A-Za-z0-9_][A-Za-z0-9_-]*/g,
  /&[A-Za-z0-9_][A-Za-z0-9_-]*(?:\.(?:\*|[A-Za-z0-9_][A-Za-z0-9_-]*(?:\.[A-Za-z0-9_][A-Za-z0-9_-]*)*))?/g,
  /#[A-Za-z0-9_][A-Za-z0-9_-]*/g,
  /derive\?/g,
  /::|&&|\|\||==|!=|>=|<=|\.\.|[><!]/g,
  /\.\{[A-Za-z0-9_, \t-]*\}/g,
  /\.[A-Za-z0-9_][A-Za-z0-9_-]*(?:\.(?:[A-Za-z0-9_][A-Za-z0-9_-]*|\$[A-Za-z0-9_][A-Za-z0-9_-]*))*(?:\[\d+\])?/g,
  /\([A-Za-z0-9_][A-Za-z0-9_-]*(?:\s*,\s*[A-Za-z0-9_][A-Za-z0-9_-]*)+\)(?=\s*:)/g,
  /\[[^\]\r\n]*\]/g,
  /\b[A-Za-z][A-Za-z0-9_]*\b/g,
  /[{}\[\]]/g,
];

function tokenAt(line, offset) {
  for (const pattern of TOKEN_PATTERNS) {
    pattern.lastIndex = 0;
    for (const match of line.matchAll(pattern)) {
      if (offset >= match.index && offset < match.index + match[0].length) {
        return { text: match[0], start: match.index, end: match.index + match[0].length };
      }
    }
  }
  return undefined;
}

function captureRange(match, group) {
  const value = match?.[group];
  if (value === undefined) return undefined;
  if (match.indices?.[group]) return { start: match.indices[group][0], end: match.indices[group][1] };
  const start = match.index + match[0].indexOf(value);
  return { start, end: start + value.length };
}

function contains(range, start, end) {
  return range && start >= range.start && end <= range.end;
}

function conditionRange(mask) {
  let match = /^\s*require\b/.exec(mask);
  if (match) {
    const end = mask.search(/\belse\s+throw\b/);
    return end >= 0 ? { start: match[0].length, end } : undefined;
  }
  match = /^\s*(?:}\s*else\s+|else\s+)?if\b/.exec(mask);
  if (match) {
    const end = mask.lastIndexOf("{");
    return end >= match[0].length ? { start: match[0].length, end } : undefined;
  }
  return undefined;
}

function annotationTail(mask) {
  const match = /(?:\s+@(?:since|removed)\(\d+\)){1,2}\s*$/.exec(mask);
  return match ? { start: match.index, end: match.index + match[0].length } : undefined;
}

function isValuePosition(mode, mask, start) {
  if (mode === "instance") {
    const colon = mask.indexOf(":");
    return colon >= 0 && start > colon;
  }
  if (mode === "schema") {
    const equals = mask.indexOf("=");
    return equals >= 0 && start > equals;
  }
  if (mode === "logic") {
    const derive = /^\s*derive\??\b/.test(mask) && mask.indexOf("=") >= 0 && start > mask.indexOf("=");
    const thrown = /\belse\s+throw\s+/.test(mask) && start > mask.search(/\bthrow\b/);
    return derive || thrown;
  }
  return false;
}

function logicOperandPosition(mask, start) {
  const condition = conditionRange(mask);
  if (contains(condition, start, start + 1)) return true;
  const loop = /^\s*for\s+(\$[A-Za-z0-9_][A-Za-z0-9_-]*)\s+in\s+/d.exec(mask);
  if (loop && contains(captureRange(loop, 1), start, start + 1)) return true;
  const derive = /^\s*derive\??\s+([^=]+)=\s*/d.exec(mask);
  if (!derive) return false;
  const target = captureRange(derive, 1);
  if (contains(target, start, start + 1)) return true;
  const expressionStart = derive[0].length;
  return start === expressionStart;
}

function classify(text, absoluteOffset, line, token, region) {
  const parsed = parseSource("memory.ab", text);
  const context = parsed.contexts.find((entry) => absoluteOffset >= entry.start && absoluteOffset <= entry.end);
  const mask = context?.mask || scanLine(line.text).masked;
  const relativeStart = context ? line.start + token.start - context.start : token.start;
  const relativeEnd = relativeStart + token.text.length;
  const mode = context?.mode || "";

  if (region === "string") {
    return mode !== "schema" && /^\$(?:\$|\{|[A-Za-z0-9_])/.test(token.text)
      && isValuePosition(mode, mask, relativeStart) ? "interpolation" : undefined;
  }

  let match = /^\s*(schema)\s+[A-Za-z][A-Za-z0-9_]*\s*\{\s*$/d.exec(mask);
  if (token.text === "schema" && contains(captureRange(match, 1), relativeStart, relativeEnd)) return "schema";
  match = /^\s*(logic)\s+[A-Za-z][A-Za-z0-9_]*\s*\{\s*$/d.exec(mask);
  if (token.text === "logic" && contains(captureRange(match, 1), relativeStart, relativeEnd)) return "logic";
  match = /^\s*(versions)\s+\d+\s*(\.\.)\s*\d+\s*$/d.exec(mask);
  if (token.text === "versions" && contains(captureRange(match, 1), relativeStart, relativeEnd)) return "versions";
  if (token.text === ".." && contains(captureRange(match, 2), relativeStart, relativeEnd)) return "versions";

  const field = new RegExp(`^\\s*(${ID})(\\[[^\\]]*\\])?\\s*:\\s*(${TYPE})`, "d").exec(mask);
  if (mode === "schema" && field) {
    const typeRange = captureRange(field, 3);
    if (contains(typeRange, relativeStart, relativeEnd) && relativeStart === typeRange.start) {
      if (token.text.startsWith("$(")) return "nested";
      if (["text", "int", "float", "bool", "enum", "file", "image", "ref"].includes(token.text)) return token.text;
    }
    if (token.text.startsWith("[") && contains(captureRange(field, 2), relativeStart, relativeEnd)) return "cardinality";
    if ((["@optional", "@tag", "@public"].includes(token.text) || /^@(since|removed)\(\d+\)$/.test(token.text))
        && relativeStart >= field[0].length && (mask.indexOf("=") < 0 || relativeStart < mask.indexOf("="))) {
      return token.text.startsWith("@optional") ? "optional" : token.text.startsWith("@tag") ? "tag"
        : token.text.startsWith("@public") ? "public" : token.text.startsWith("@since") ? "since" : "removed";
    }
  }
  if (mode === "schema" && token.text.startsWith("[") && /^\s*[A-Za-z0-9_][A-Za-z0-9_-]*\s*\[/.test(mask)) return "cardinality";
  if (mode === "schema" && (["@optional", "@tag", "@public"].includes(token.text) || /^@(since|removed)\(\d+\)$/.test(token.text))
      && new RegExp(`^\\s*${ID}(?:\\[[^\\]]*\\])?(?:\\s+@[^{}]+)*\\s*\\{\\s*$`).test(mask)) {
    return token.text.startsWith("@optional") ? "optional" : token.text.startsWith("@tag") ? "tag"
      : token.text.startsWith("@public") ? "public" : token.text.startsWith("@since") ? "since" : "removed";
  }

  const header = new RegExp(`^\\s*${SCHEMA}\\s*(::)`, "d").exec(mask);
  if (header && token.text === "::" && contains(captureRange(header, 1), relativeStart, relativeEnd)) return "header";
  if (header && token.text.startsWith("@") && !/^@(since|removed)\(/.test(token.text)) return /^@id(?:\.|$)/.test(token.text) ? "identity" : "headerTag";

  if (/^@(since|removed)\(/.test(token.text) && contains(annotationTail(mask), relativeStart, relativeEnd)
      && (mode === "instance" || Boolean(header))) return token.text.startsWith("@since") ? "since" : "removed";
  if (mode === "instance" && token.text.startsWith("&") && new RegExp(`^\\s*${token.text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\s*$`).test(mask)) return "clone";
  if (mode === "instance" && token.text.startsWith(".{") && mask.indexOf(token.text) >= 0 && mask.indexOf(":") >= relativeEnd) return "multiPath";
  if (mode === "instance" && /^\([^)]*,[^)]*\)$/.test(token.text) && mask.indexOf(":") >= relativeEnd) return "tuple";
  if (mode === "instance" && token.text === "{" && new RegExp(`^\\s*${ID}(?:\\.${ID})*\\s*\\{\\s*$`).test(mask)) return "bodyBlock";
  if (token.text.startsWith("#") && isValuePosition(mode, mask, relativeStart)) return "tagObject";
  if (token.text.startsWith("[") && isValuePosition(mode, mask, relativeStart)) return "list";
  if (mode !== "schema" && /^\$(?:\$|\{|[A-Za-z0-9_])/.test(token.text) && isValuePosition(mode, mask, relativeStart)
      && !(mode === "logic" && /^\$[A-Za-z0-9_]/.test(token.text) && logicOperandPosition(mask, relativeStart))) return "interpolation";

  if (mode !== "logic") return undefined;
  const first = /^\s*(derive\?(?=\s|$)|(?:derive|require|if|for)\b)/d.exec(mask);
  if (first && contains(captureRange(first, 1), relativeStart, relativeEnd)) {
    return token.text === "derive?" ? "deriveOptional" : token.text;
  }
  if (token.text === "else" && /(?:\belse\s+throw\b|(?:^|})\s*else(?:\s+if)?\b)/.test(mask)) return "else";
  if (token.text === "throw" && /\belse\s+throw\s+/.test(mask)) return "throw";
  if (token.text === "in" && /^\s*for\s+\$[A-Za-z0-9_][A-Za-z0-9_-]*\s+in\b/.test(mask)) return "in";
  if (/^\$[A-Za-z0-9_]/.test(token.text) && logicOperandPosition(mask, relativeStart)) return "logicVariable";
  if (token.text.startsWith(".") && logicOperandPosition(mask, relativeStart)) return "logicPath";

  const condition = conditionRange(mask);
  if (contains(condition, relativeStart, relativeEnd)) {
    if (["and", "or", "not", "contains", "exists", "length", "version"].includes(token.text)) return token.text;
    if (["&&", "||"].includes(token.text)) return token.text === "&&" ? "and" : "or";
    if (["==", "!=", ">", ">=", "<", "<=", "!"].includes(token.text)) return token.text === "!" ? "not" : "comparison";
  }
  if (["length", "version"].includes(token.text) && /^\s*derive\??\b[^=]*=\s*(?:length\s*\(|version\b)/.test(mask)) return token.text;
  return undefined;
}

function markdown(key) {
  const [title, purpose, example] = HELP[key];
  return `**${title}**\n\n${purpose}\n\n\`\`\`abstract\n${example}\n\`\`\``;
}

function findSyntaxHelp(text, offset) {
  if (!Number.isInteger(offset) || offset < 0 || offset > text.length) return undefined;
  const line = lineAt(text, offset);
  if (line.offset < 0 || line.offset >= line.text.length) return undefined;
  const region = regionAt(line.text, line.offset);
  if (region === "comment") return undefined;
  const token = tokenAt(line.text, line.offset);
  if (!token) return undefined;
  const key = classify(text, offset, line, token, region);
  return key ? { key, start: line.start + token.start, end: line.start + token.end, markdown: markdown(key) } : undefined;
}

module.exports = { findSyntaxHelp, markdown };
