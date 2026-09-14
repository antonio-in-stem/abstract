// Pure protocol/model boundary. Names are accepted only from compiler bindings;
// regex below checks the language's identifier grammar, never finds references.
const path = require("path");
const crypto = require("crypto");
const key = (file) => process.platform === "win32" ? path.normalize(file).toLowerCase() : path.normalize(file);
const hash = (text) => crypto.createHash("sha256").update(text, "utf8").digest("hex");
const normalize = (name) => name.replace(/[A-Z-]/g, (character) => character === "-" ? "_" : character.toLowerCase());
const validName = (name, kind = "schema") => typeof name === "string" && (kind === "schema"
  ? /^[A-Za-z][A-Za-z0-9_]*$/.test(name)
  : /^[A-Za-z0-9_][A-Za-z0-9_-]*$/.test(name));
const fail = (message) => { throw new Error(message); };
const renameUnavailable = (symbol) => symbol.kind === "instance" && symbol.implicit
  ? "This instance uses its file stem as its ID. Rename the file or add an explicit @id before renaming references."
  : "The compiler cannot prove that every use of this symbol can be renamed safely.";

function readBindings(response) {
  const graph = response.bindings;
  if (!response.analyzed || response.truncated || response.diagnostics.length || graph?.version !== 1 || graph.complete !== true) {
    fail(graph?.reason || "Complete compiler schema bindings are unavailable; fix diagnostics or update the compiler.");
  }
  if (!Array.isArray(graph.sources) || graph.sources.length > 1024 || !Array.isArray(graph.symbols) || graph.symbols.length > 16384) fail("Invalid schema binding graph.");
  const sources = new Set();
  for (const source of graph.sources) {
    if (typeof source.path !== "string" || !path.isAbsolute(source.path) || !/^[a-f0-9]{64}$/.test(source.sha256) || sources.has(key(source.path))) fail("Invalid or duplicate binding source.");
    sources.add(key(source.path));
  }
  const ids = new Set(); const spans = new Map();
  let count = 0;
  const location = (entry, spelling, implicit = false) => {
    const { start, end } = entry?.range || {};
    const pos = (p) => p && Number.isInteger(p.line) && p.line >= 0 && p.line <= 0x7FFFFFFF
      && Number.isInteger(p.character) && p.character >= 0 && p.character <= 0x7FFFFFFF;
    if (typeof entry?.path !== "string" || !sources.has(key(entry.path)) || !pos(start) || !pos(end)
      || start.line !== end.line || end.character - start.character !== spelling.length
      || (!implicit && !spelling.length)) fail("Invalid semantic token location.");
    return `${key(entry.path)}:${start.line}:${start.character}:${end.character}`;
  };
  for (const symbol of graph.symbols) {
    const kind = symbol.kind || "schema";
    const implicit = kind === "instance" && symbol.implicit === true && symbol.renamable === false;
    if (!["schema", "field", "instance", "loop"].includes(kind) || !validName(symbol.name, kind)
      || (kind !== "schema" && symbol.name !== normalize(symbol.name))
      || typeof symbol.id !== "string" || !symbol.id || ids.has(symbol.id)
      || !Array.isArray(symbol.occurrences) || !symbol.occurrences.length) fail("Invalid or ambiguous semantic identity.");
    ids.add(symbol.id);
    const declarationSpelling = symbol.declaration?.spelling ?? (kind === "schema" ? symbol.name : "");
    if (implicit !== (declarationSpelling === "")) fail("Invalid implicit instance declaration.");
    const declaration = location(symbol.declaration, declarationSpelling, implicit);
    let declarations = 0;
    for (const occurrence of symbol.occurrences) {
      const spelling = occurrence.spelling ?? (kind === "schema" ? symbol.name : undefined);
      if (typeof spelling !== "string" || (!implicit && !validName(spelling, kind))) fail("Invalid semantic occurrence spelling.");
      if (kind !== "schema" && spelling && normalize(spelling) !== symbol.name) fail("Semantic occurrence spelling does not match its identity.");
      const isImplicitDeclaration = implicit && occurrence.role === "declaration";
      const span = location(occurrence, spelling, isImplicitDeclaration);
      if (++count > 65536 || (!isImplicitDeclaration && spans.has(span)) || !["declaration", "reference"].includes(occurrence.role)) fail("Invalid or duplicate semantic occurrence.");
      if (!isImplicitDeclaration) spans.set(span, symbol.id);
      if (occurrence.role === "declaration") {
        declarations += 1;
        if (span !== declaration) fail("Semantic declaration does not bind to its occurrence.");
      }
    }
    if (declarations !== 1) fail("Ambiguous semantic declaration.");
  }
  return graph;
}

function offset(text, position) {
  let at = 0;
  for (let line = 0; line < position.line; line += 1) {
    at = text.indexOf("\n", at);
    if (at < 0) fail("Schema range is outside its source.");
    at += 1;
  }
  const end = text.indexOf("\n", at);
  const line = text.slice(at, end < 0 ? text.length : end).replace(/\r$/, "");
  if (position.character > line.length) fail("Schema range is outside its line.");
  return at + position.character;
}

function validateSnapshot(graph, texts) {
  if (graph.sources.length !== texts.size) fail("Project membership changed during schema analysis.");
  for (const source of graph.sources) {
    const content = texts.get(key(source.path));
    if (typeof content !== "string" || hash(content) !== source.sha256) fail("Project source changed during schema analysis.");
  }
  const occupied = new Map();
  for (const symbol of graph.symbols) for (const entry of symbol.occurrences) {
    const content = texts.get(key(entry.path));
    const start = offset(content, entry.range.start); const end = offset(content, entry.range.end);
    const spelling = entry.spelling ?? (symbol.kind === "schema" || !symbol.kind ? symbol.name : "");
    if (content.slice(start, end) !== spelling) fail("Semantic occurrence does not match its source.");
    const intervals = occupied.get(key(entry.path)) || [];
    intervals.push([start, end]); occupied.set(key(entry.path), intervals);
  }
  for (const intervals of occupied.values()) {
    intervals.sort((a, b) => a[0] - b[0]);
    if (intervals.some((value, i) => i && value[0] < intervals[i - 1][1])) fail("Overlapping schema occurrences.");
  }
}

function symbolAt(graph, file, position) {
  for (const symbol of graph.symbols) for (const entry of symbol.occurrences) {
    const { start, end } = entry.range;
    if (key(entry.path) === key(file) && position.line === start.line && position.character >= start.character && position.character < end.character) return { symbol, occurrence: entry };
  }
  return undefined;
}

function rename(graph, symbol, newName, texts) {
  if (!graph.symbols.includes(symbol)) fail("Semantic identity is not part of this snapshot.");
  const kind = symbol.kind || "schema";
  if (symbol.renamable === false) fail(renameUnavailable(symbol));
  if (!validName(newName, kind)) fail(kind === "schema"
    ? "A schema name must start with an ASCII letter and contain only letters, digits and underscores."
    : "This name must contain only ASCII letters, digits, underscores and hyphens.");
  const domain = (entry) => {
    const entryKind = entry.kind || "schema";
    if (entryKind === "schema" || entryKind === "instance") return "";
    return entry.scopeId ?? entry.ownerId ?? entry.owner;
  };
  const identity = kind === "schema" ? newName : normalize(newName);
  if (graph.symbols.some((other) => other !== symbol && (other.kind || "schema") === kind
      && other.name === identity && domain(symbol) !== undefined && domain(other) === domain(symbol))) {
    fail(`${kind[0].toUpperCase()}${kind.slice(1)} '${newName}' is already declared in this scope.`);
  }
  validateSnapshot(graph, texts);
  const edits = new Map();
  for (const entry of symbol.occurrences) {
    const file = key(entry.path); const list = edits.get(file) || [];
    list.push(entry); edits.set(file, list);
  }
  const changed = new Map();
  for (const [file, list] of edits) {
    const original = texts.get(file);
    const replacements = list.map((entry) => ({ start: offset(original, entry.range.start), end: offset(original, entry.range.end) })).sort((a, b) => b.start - a.start);
    let content = original;
    for (const edit of replacements) content = content.slice(0, edit.start) + newName + content.slice(edit.end);
    if (content !== original) changed.set(file, content);
  }
  return changed;
}

module.exports = { key, hash, normalize, validName, renameUnavailable, readBindings, validateSnapshot, symbolAt, rename };
