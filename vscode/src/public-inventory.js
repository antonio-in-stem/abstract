const path = require("path");
const { key, hash, validName } = require("./schema-bindings");

// Pure compiler-owned inventory admission and presentation. This checks the
// protocol/source binding, not export eligibility or runtime authorization.
const PROFILE = "independent-scalars-v1";
const LIMITS = Object.freeze({ sources: 1024, declarations: 16384, targets: 16384,
  variants: 262144, rows: 32768, presentationBytes: 4 * 1024 * 1024, path: 64, versions: 4096 });
const fail = (message) => { throw new Error(`Public inventory: ${message}`); };
const object = (v) => v && typeof v === "object" && !Array.isArray(v);
const identifier = (v) => typeof v === "string" && /^[a-z0-9_]+$/.test(v);
const tuple = (...parts) => JSON.stringify(parts);
const unicode = (v) => typeof v === "string" && !/[\uD800-\uDFFF]/u.test(v);
function keys(value, required, optional = []) {
  if (!object(value) || required.some((name) => !Object.hasOwn(value, name))
    || Object.keys(value).some((name) => !required.includes(name) && !optional.includes(name))) fail("malformed or unknown fields.");
}
function array(value, cap) {
  if (!Array.isArray(value) || value.length > cap) fail("collection limit or shape.");
  return value;
}
function window(value) {
  keys(value, ["min", "max"]);
  if (![value.min, value.max].every((n) => Number.isInteger(n) && n >= 1 && n <= 0xFFFFFFFF)
    || value.min > value.max || value.max - value.min >= LIMITS.versions) fail("invalid version interval.");
  return value;
}
const contains = (outer, inner) => outer.min <= inner.min && outer.max >= inner.max;
function segments(value) {
  if (!array(value, LIMITS.path).length || value.some((p) => !identifier(p))) fail("invalid structured field path.");
  return value;
}
function int(value) {
  if (typeof value !== "string" || !/^(0|-[1-9][0-9]*|[1-9][0-9]*)$/.test(value) || value.length > 20) fail("invalid exact i64.");
  const n = BigInt(value);
  if (n < -9223372036854775808n || n > 9223372036854775807n) fail("i64 outside its domain.");
  return n;
}
function float(value) {
  if (typeof value !== "string" || !/^[0-9a-f]{16}$/.test(value)) fail("invalid exact f64 bits.");
  const n = Buffer.from(value, "hex").readDoubleBE();
  if (!Number.isFinite(n)) fail("non-finite f64.");
  return n;
}
function domain(value) {
  if (!object(value)) fail("missing scalar domain.");
  if (["int", "float", "text"].includes(value.type)) {
    keys(value, value.type === "text" ? ["type", "ranges", "lengthUnit"] : ["type", "ranges"]);
    if (value.type === "text" && value.lengthUnit !== "unicode-scalars") fail("unsupported text length unit.");
    for (const range of array(value.ranges, 16384)) {
      keys(range, ["min", "max"]);
      const low = value.type === "float" ? float(range.min) : int(range.min);
      const high = value.type === "float" ? float(range.max) : int(range.max);
      if (low > high || value.type === "text" && low < 0n) fail("invalid domain range.");
    }
  } else if (value.type === "bool") keys(value, ["type"]);
  else if (value.type === "enum") {
    keys(value, ["type", "members"]);
    if (!array(value.members, 16384).length || value.members.some((m) => !identifier(m))
      || new Set(value.members).size !== value.members.length) fail("invalid enum domain.");
  } else fail("unsupported scalar domain.");
}
function scalar(value, type) {
  if (type === "int") int(value);
  else if (type === "float") float(value);
  else if (type === "bool") { if (typeof value !== "boolean") fail("invalid boolean default."); }
  else if (!unicode(value) || type === "enum" && !identifier(value)) fail("invalid text/enum default.");
}

function readInventory(response, texts) {
  const inventory = response?.publicInventory;
  if (!inventory || response.truncated) fail("complete inventory unavailable; the compiler response was missing or truncated.");
  keys(inventory, ["version", "status", "catalogComplete", "sources", "declarations", "roots", "exportDiagnostics"],
    ["reason", "projectVersions", "fragment"]);
  if (inventory.version !== 1 || !["unavailable", "export-rejected", "export-admitted"].includes(inventory.status)
    || inventory.reason !== undefined && !unicode(inventory.reason)) fail("unsupported inventory protocol.");
  array(inventory.sources, LIMITS.sources);
  array(inventory.declarations, LIMITS.declarations);
  array(inventory.roots, LIMITS.targets);
  array(inventory.exportDiagnostics, 100);
  if (inventory.status === "unavailable") {
    if (inventory.catalogComplete !== false || inventory.sources.length || inventory.declarations.length
      || inventory.roots.length || inventory.exportDiagnostics.length || inventory.fragment !== undefined
      || inventory.projectVersions !== undefined) fail("partial unavailable inventory.");
    return inventory;
  }
  if (response.analyzed !== true || response.diagnostics.length || inventory.catalogComplete !== true) fail("inventory attached to unsuccessful language analysis.");
  const project = window(inventory.projectVersions);
  const sourceTexts = new Map();
  for (const source of inventory.sources) {
    keys(source, ["path", "sha256"]);
    if (typeof source.path !== "string" || !path.isAbsolute(source.path) || !/^[a-f0-9]{64}$/.test(source.sha256)
      || sourceTexts.has(key(source.path))) fail("invalid or duplicate source identity.");
    const text = texts.get(key(source.path));
    if (typeof text !== "string" || hash(text) !== source.sha256) fail("source hash does not match the captured snapshot.");
    let at = 0;
    const offsets = [];
    const lines = text.split("\n").map((line) => { offsets.push(at); at += line.length + 1; return line.replace(/\r$/, ""); });
    sourceTexts.set(key(source.path), { text, lines, offsets });
  }
  if (sourceTexts.size !== texts.size) fail("source membership differs from the captured snapshot.");
  const location = (value, expected) => {
    keys(value, ["path", "range"]);
    keys(value.range, ["start", "end"]);
    for (const pos of [value.range.start, value.range.end]) {
      keys(pos, ["line", "character"]);
      if (![pos.line, pos.character].every((n) => Number.isInteger(n) && n >= 0 && n <= 0x7FFFFFFF)) fail("invalid UTF-16 position.");
    }
    const { start, end } = value.range;
    const source = typeof value.path === "string" && sourceTexts.get(key(value.path));
    const lines = source?.lines;
    if (!lines || !path.isAbsolute(value.path) || start.line > end.line || end.line >= lines.length
      || start.character > lines[start.line].length || end.character > lines[end.line].length
      || start.line === end.line && start.character > end.character) fail("source range outside the captured source.");
    if (expected !== undefined) {
      const found = source.text.slice(source.offsets[start.line] + start.character, source.offsets[end.line] + end.character);
      if (typeof expected === "string" ? found !== expected : !expected.test(found)) fail("source token does not match its declaration.");
    }
  };
  const declarations = new Map();
  const spans = new Set();
  for (const declaration of inventory.declarations) {
    keys(declaration, ["declaringSchema", "declaringPath", "spelling", "nominationSpelling", "type", "list", "optional", "tag", "versions", "declaration", "nomination"]);
    if (!validName(declaration.declaringSchema) || typeof declaration.spelling !== "string"
      || !/^[A-Za-z0-9_][A-Za-z0-9_-]*$/.test(declaration.spelling)
      || !["text", "int", "float", "bool", "enum", "file", "image", "ref", "nested", "group"].includes(declaration.type)
      || [declaration.list, declaration.optional, declaration.tag].some((v) => typeof v !== "boolean")) fail("invalid nominated declaration.");
    segments(declaration.declaringPath);
    if (declaration.spelling.replace(/-/g, "_").toLowerCase() !== declaration.declaringPath.at(-1)) fail("field spelling does not match its normalized identity.");
    if (!contains(project, window(declaration.versions))) fail("declaration versions escape the project.");
    location(declaration.declaration, declaration.spelling);
    if (!unicode(declaration.nominationSpelling) || !declaration.nominationSpelling.startsWith("@")
      || !declaration.nominationSpelling.endsWith("public")) fail("invalid nomination spelling.");
    location(declaration.nomination, declaration.nominationSpelling);
    if (key(declaration.declaration.path) !== key(declaration.nomination.path)) fail("nomination and declaration have different sources.");
    const id = tuple(declaration.declaringSchema, declaration.declaringPath);
    const span = tuple(key(declaration.nomination.path), declaration.nomination.range);
    if (declarations.has(id) || spans.has(span)) fail("duplicate nominated declaration.");
    declarations.set(id, declaration); spans.add(span);
  }
  const diagnosticLocation = (value) => {
    if (value.displayPath !== undefined && !unicode(value.displayPath)) fail("invalid diagnostic display path.");
    if (value.path !== undefined && (typeof value.path !== "string" || !path.isAbsolute(value.path)
      || !sourceTexts.has(key(value.path)))) fail("diagnostic source is outside the captured project.");
    if (value.range !== undefined) location({ path: value.path, range: value.range });
  };
  for (const diagnostic of inventory.exportDiagnostics) {
    keys(diagnostic, ["code", "severity", "message", "notes"], ["path", "range", "displayPath"]);
    if (!/^E70[23]$/.test(diagnostic.code) || diagnostic.severity !== "error" || !unicode(diagnostic.message)) fail("invalid export diagnostic.");
    diagnosticLocation(diagnostic);
    for (const note of array(diagnostic.notes, 8)) {
      keys(note, ["message"], ["path", "range", "displayPath"]);
      if (!unicode(note.message)) fail("invalid diagnostic note.");
      diagnosticLocation(note);
    }
  }
  if (inventory.status === "export-rejected") {
    if (!inventory.exportDiagnostics.length || inventory.fragment !== undefined || inventory.roots.length) fail("rejected export contains admitted results.");
    return inventory;
  }
  if (inventory.exportDiagnostics.length) fail("admitted export contains errors.");
  const fragment = inventory.fragment;
  keys(fragment, ["formatVersion", "dependencyProfile", "complete", "bindingStatus", "entries"]);
  if (fragment.formatVersion !== 1 || fragment.dependencyProfile !== PROFILE || fragment.complete !== true
    || fragment.bindingStatus !== "unbound") fail("unsupported or runtime-bound fragment.");
  const roots = new Map();
  for (const root of inventory.roots) {
    keys(root, ["rootSchema", "rootInstanceId", "declaration"]);
    if (!validName(root.rootSchema) || !identifier(root.rootInstanceId)) fail("invalid root identity.");
    const id = tuple(root.rootSchema, root.rootInstanceId);
    if (roots.has(id)) fail("duplicate root identity.");
    location(root.declaration, root.rootSchema);
    roots.set(id, root);
  }
  const usedRoots = new Set();
  const targets = new Set();
  let variants = 0;
  for (const entry of array(fragment.entries, LIMITS.targets)) {
    keys(entry, ["rootSchema", "rootInstanceId", "path", "declaringSchema", "declaringPath", "declarationVersions", "domain", "variants"]);
    segments(entry.path); segments(entry.declaringPath);
    const root = tuple(entry.rootSchema, entry.rootInstanceId);
    const declared = declarations.get(tuple(entry.declaringSchema, entry.declaringPath));
    if (!roots.has(root) || !declared || !declared.versions || declared.list || declared.tag
      || entry.declaringPath.length > entry.path.length
      || entry.declaringPath.some((part, i) => part !== entry.path[entry.path.length - entry.declaringPath.length + i])) fail("target has no exact declaration/root binding.");
    usedRoots.add(root);
    const id = tuple(entry.rootSchema, entry.rootInstanceId, entry.path);
    if (targets.has(id)) fail("duplicate target identity.");
    targets.add(id);
    if (!contains(declared.versions, window(entry.declarationVersions))) fail("effective window escapes its declaration.");
    domain(entry.domain);
    if (entry.domain.type !== declared.type) fail("target type differs from its declaration.");
    const rows = array(entry.variants, LIMITS.versions);
    if (!rows.length || (variants += rows.length) > LIMITS.variants) fail("target/variant budget.");
    let next = entry.declarationVersions.min;
    for (const variant of rows) {
      const present = variant.presence === "present";
      keys(variant, present ? ["versions", "presence", "writable", "default"] : ["versions", "presence", "writable"]);
      if (!present && variant.presence !== "absent" || variant.writable !== present) fail("invalid presence/writability.");
      if (window(variant.versions).min !== next || !contains(entry.declarationVersions, variant.versions)) fail("variant intervals do not partition the target window.");
      next = variant.versions.max + 1;
      if (present) scalar(variant.default, entry.domain.type);
    }
    if (next !== entry.declarationVersions.max + 1) fail("variant coverage is incomplete.");
  }
  if (usedRoots.size !== roots.size) fail("root inventory is not the exact target closure.");
  return inventory;
}

function encoded(value, type) {
  if (type === "float") return `${Object.is(float(value), -0) ? "-0.0" : String(float(value))} (f64 ${value})`;
  return type === "text" ? JSON.stringify(value) : String(value);
}
function brief(value, limit = 320) {
  return value.length <= limit ? value : `${value.slice(0, limit)}… [${value.length} UTF-16 units; value abbreviated]`;
}
function domainText(domain) {
  if (domain.type === "bool") return "bool";
  const parts = domain.type === "enum" ? domain.members : domain.ranges;
  let text = `${domain.type}(`;
  for (let i = 0; i < parts.length; i += 1) {
    if (text.length >= 240) { text += `… [${parts.length - i} more alternatives]`; break; }
    const part = parts[i];
    text += (i ? ", " : "") + (domain.type === "enum" ? brief(part, 160)
      : `${encoded(part.min, domain.type === "text" ? "int" : domain.type)}..${encoded(part.max, domain.type === "text" ? "int" : domain.type)}`);
  }
  return `${text})${domain.type === "text" ? " · Unicode scalars" : ""}`;
}
function items(inventory) {
  if (inventory.status === "unavailable") return [];
  const out = [];
  let textBytes = 0;
  const entries = inventory.fragment?.entries || [];
  const active = new Set(entries.map((e) => tuple(e.declaringSchema, e.declaringPath)));
  const push = (row) => {
    textBytes += Buffer.byteLength(row.label) + Buffer.byteLength(row.description) + Buffer.byteLength(row.detail);
    if (out.length >= LIMITS.rows || textBytes > LIMITS.presentationBytes) fail("selection list exceeds the 32,768-row / 4 MiB presentation limit; no partial list is shown.");
    out.push(row);
  };
  for (const declaration of inventory.declarations) {
    const used = active.has(tuple(declaration.declaringSchema, declaration.declaringPath));
    const status = inventory.status === "export-rejected" ? "Nominated — export rejected" : used ? "Nominated" : "No active occurrence";
    push({ label: `${declaration.declaringSchema}.${declaration.declaringPath.join(".")}`,
      description: status, detail: `${declaration.type}${declaration.list ? " list" : ""} · own declaration window ${declaration.versions.min}..${declaration.versions.max} · Go to @public declaration`, location: declaration.nomination, kind: "nomination" });
  }
  const roots = new Map(inventory.roots.map((r) => [tuple(r.rootSchema, r.rootInstanceId), r]));
  for (const entry of entries) {
    const summary = domainText(entry.domain);
    for (const variant of entry.variants) {
    push({ label: `${entry.rootSchema} [${entry.rootInstanceId}].${entry.path.join(".")}`,
      description: `Export admitted — unbound · v${variant.versions.min}..${variant.versions.max}`,
      detail: `${summary} · ${variant.presence === "absent" ? "Absent; not writable" : `Default ${brief(encoded(variant.default, entry.domain.type))}; writable`} · Go to root instance`,
      location: roots.get(tuple(entry.rootSchema, entry.rootInstanceId)).declaration, kind: "occurrence" });
    }
  }
  return out;
}

module.exports = { PROFILE, LIMITS, readInventory, items };
