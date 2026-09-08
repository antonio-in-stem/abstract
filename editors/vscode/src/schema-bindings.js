// Pure protocol/model boundary. Names are accepted only from compiler bindings;
// regex below checks the language's identifier grammar, never finds references.
const path = require("path");
const crypto = require("crypto");
const key = (file) => process.platform === "win32" ? path.normalize(file).toLowerCase() : path.normalize(file);
const hash = (text) => crypto.createHash("sha256").update(text, "utf8").digest("hex");
const validName = (name) => typeof name === "string" && /^[A-Za-z][A-Za-z0-9_]*$/.test(name);
const fail = (message) => { throw new Error(message); };

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
  const ids = new Set(); const names = new Set(); const spans = new Map();
  let count = 0;
  const location = (entry, name) => {
    const { start, end } = entry?.range || {};
    const pos = (p) => p && Number.isInteger(p.line) && p.line >= 0 && p.line <= 0x7FFFFFFF
      && Number.isInteger(p.character) && p.character >= 0 && p.character <= 0x7FFFFFFF;
    if (typeof entry?.path !== "string" || !sources.has(key(entry.path)) || !pos(start) || !pos(end)
      || start.line !== end.line || end.character - start.character !== name.length) fail("Invalid schema token location.");
    return `${key(entry.path)}:${start.line}:${start.character}:${end.character}`;
  };
  for (const symbol of graph.symbols) {
    if (!validName(symbol.name) || symbol.kind !== "schema" || typeof symbol.id !== "string" || !symbol.id || ids.has(symbol.id) || names.has(symbol.name)
      || !Array.isArray(symbol.occurrences) || !symbol.occurrences.length) fail("Invalid or ambiguous schema identity.");
    ids.add(symbol.id); names.add(symbol.name);
    const declaration = location(symbol.declaration, symbol.name);
    let declarations = 0;
    for (const occurrence of symbol.occurrences) {
      const span = location(occurrence, symbol.name);
      if (++count > 16384 || spans.has(span) || !["declaration", "reference"].includes(occurrence.role)) fail("Invalid or duplicate schema occurrence.");
      spans.set(span, symbol.id);
      if (occurrence.role === "declaration") {
        declarations += 1;
        if (span !== declaration) fail("Schema declaration does not bind to its occurrence.");
      }
    }
    if (declarations !== 1) fail("Ambiguous schema declaration.");
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
    if (content.slice(start, end) !== symbol.name) fail("Schema occurrence does not match its source.");
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
  if (!graph.symbols.includes(symbol)) fail("Schema identity is not part of this snapshot.");
  if (!validName(newName)) fail("A schema name must start with an ASCII letter and contain only letters, digits and underscores.");
  if (graph.symbols.some((other) => other !== symbol && other.name === newName)) fail(`Schema '${newName}' is already declared.`);
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

module.exports = { key, hash, validName, readBindings, validateSnapshot, symbolAt, rename };
