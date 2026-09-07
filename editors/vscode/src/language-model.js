// Tolerant editor index for SPEC 1.x. This is not a validator: diagnostics
// remain the compiler's responsibility. All offsets are UTF-16, as in VS Code.
const path = require("path");

const ID = "[A-Za-z0-9_][A-Za-z0-9_-]*";
const SCHEMA = "[A-Za-z][A-Za-z0-9_]*";
const PATH = `${ID}(?:\\.${ID})*`;
const normalize = (name) => name.replace(/[A-Z-]/g, (c) => c === "-" ? "_" : c.toLowerCase());

// Strings end on their physical line. Escapes consume the next character;
// testing only the immediately preceding slash gets even-length runs wrong.
function scanLine(line) {
  let masked = "";
  let inString = false;
  let comment = line.length;
  for (let i = 0; i < line.length; i += 1) {
    const c = line[i];
    if (inString && c === "\\") {
      masked += " ";
      if (i + 1 < line.length) { masked += " "; i += 1; }
    } else if (c === '"') {
      inString = !inString;
      masked += " ";
    } else if (!inString && c === "/" && line[i + 1] === "/" && (i === 0 || /[ \t]/.test(line[i - 1]))) {
      comment = i;
      masked += " ".repeat(line.length - i);
      break;
    } else {
      masked += inString ? " " : c;
    }
  }
  return { masked, code: line.slice(0, comment), comment, inString };
}

function logicalLines(text) {
  const physical = text.split(/\n/);
  const statements = [];
  let start = 0;
  let offset = 0;
  let raw = "";
  let mask = "";
  let depth = 0;
  for (let i = 0; i < physical.length; i += 1) {
    const line = physical[i].replace(/\r$/, "");
    const scan = scanLine(line);
    raw += scan.code.padEnd(physical[i].length, " ");
    mask += scan.masked.padEnd(physical[i].length, " ");
    // A block opener is a last-token brace on a declaration/path line. All
    // other braces are value braces (multipaths, patterns, interpolation).
    const block = new RegExp(`^\\s*(?:(?:schema|logic)\\s+${SCHEMA}|${PATH}(?:\\[[^\\]]*\\])?(?:\\s+@(?:optional|tag|since\\(\\d+\\)|removed\\(\\d+\\)))*)\\s*\\{\\s*$`).test(mask)
      || /^\s*(?:if\b|for\b|(?:}\s*)?else\b)[\s\S]*\{\s*$/.test(mask);
    for (let j = 0; j < scan.masked.length; j += 1) {
      const c = scan.masked[j];
      if (c === "(" || c === "[" || (c === "{" && !block)) depth += 1;
      else if (c === ")" || c === "]" || (c === "}" && depth > 0)) depth = Math.max(0, depth - 1);
    }
    const headerContinues = new RegExp(`^\\s*${SCHEMA}\\s*::`).test(mask) && /,\s*$/.test(mask);
    const end = offset + physical[i].length;
    if ((depth === 0 && !headerContinues) || i === physical.length - 1) {
      statements.push({ start, end, text: raw, mask });
      start = end + 1;
      raw = "";
      mask = "";
    } else { raw += "\n"; mask += "\n"; }
    offset = end + 1;
  }
  return statements;
}

function symbol(name, statement, file, fields = undefined) {
  const local = statement.mask.indexOf(name);
  return { name, key: normalize(name), file, start: statement.start + local,
    end: statement.start + local + name.length, declaration: statement.text.trim(), fields };
}

function parseSource(file, text) {
  const schemas = [];
  const instances = [];
  const contexts = [];
  const blocks = [];
  let mode = "";
  let schema = "";
  let instance;
  let stack = [];
  for (const statement of logicalLines(text)) {
    const prefix = stack.filter((s) => s.kind === "body").flatMap((s) => s.path);
    contexts.push({ ...statement, mode, schema, instance, prefix, schemaDepth: mode === "schema" ? stack.length - 1 : 0 });
    const declaration = statement.mask.match(new RegExp(`^\\s*(schema|logic)\\s+(${SCHEMA})\\s*\\{\\s*$`));
    if (!mode && declaration) {
      mode = declaration[1]; schema = declaration[2];
      const root = symbol(schema, statement, file, []);
      root.start = statement.start + declaration[0].indexOf(schema, declaration[0].indexOf(declaration[1]) + declaration[1].length);
      root.end = root.start + schema.length;
      root.kind = mode;
      stack = [{ kind: mode, symbol: root }];
      if (mode === "schema") schemas.push(root);
      continue;
    }
    if (/^\s*}/.test(statement.mask)) {
      const closed = stack.pop();
      if (closed?.kind === "body") blocks.push({ ...closed, end: statement.end, close: statement });
      if (mode === "logic" && /^\s*}\s*else\b[\s\S]*\{\s*$/.test(statement.mask)) stack.push({ kind: "logicBody" });
      if (!stack.length && mode !== "instance") { mode = ""; schema = ""; }
      continue;
    }
    if (mode === "schema") {
      const field = statement.mask.match(new RegExp(`^\\s*(${ID})(\\[[^\\]]*\\])?\\s*(:[\\s\\S]*|(?:@[^{}]*)?\\{\\s*)$`));
      if (!field) continue;
      const entry = symbol(field[1], statement, file, /\{\s*$/.test(field[3]) ? [] : undefined);
      entry.list = Boolean(field[2]);
      entry.cardinality = field[2] || "";
      entry.optional = /@optional\b/.test(field[3]);
      entry.tag = /@tag\b/.test(field[3]);
      const tail = statement.text.slice(statement.mask.indexOf(field[3]));
      const type = field[3].match(/^:\s*(\$\([A-Za-z][A-Za-z0-9_]*\)|[a-z]+(?:\([^)]*\))?)/);
      entry.type = entry.fields ? "group" : type?.[1]?.replace(/\s+/g, " ") || "unknown";
      entry.target = entry.type.match(/^(?:\$|ref)\(([A-Za-z][A-Za-z0-9_]*)\)$/)?.[1];
      entry.enum = entry.type.match(/^enum\(([^)]*)\)$/)?.[1].split(",").map((v) => normalize(v.trim()));
      entry.default = tail.match(/=([\s\S]*)$/)?.[1].trim();
      entry.since = Number(field[3].match(/@since\((\d+)\)/)?.[1] || 1);
      entry.removed = Number(field[3].match(/@removed\((\d+)\)/)?.[1] || Infinity);
      stack.at(-1)?.symbol?.fields?.push(entry);
      if (entry.fields) stack.push({ kind: "group", symbol: entry });
      continue;
    }
    if (mode === "logic") {
      if (/\{\s*$/.test(statement.mask)) stack.push({ kind: "logicBody" });
      continue;
    }
    const header = statement.mask.match(new RegExp(`^\\s*(${SCHEMA})\\s*::`));
    if (header && !stack.length) {
      schema = header[1]; mode = "instance";
      const id = [...statement.mask.matchAll(new RegExp(`@(${ID})\\.(${ID})`, "g"))].find((m) => normalize(m[1]) === "id");
      const fallback = path.basename(file).replace(/\.abt?$/i, "");
      instance = symbol(id ? id[2] : header[1], statement, file);
      if (id) { instance.start = statement.start + id.index + id[1].length + 2; instance.end = instance.start + id[2].length; }
      instance.name = id ? normalize(id[2]) : normalize(fallback);
      instance.key = instance.name;
      instance.schema = schema;
      instance.declaration = `${schema} :: @id.${instance.name}`;
      instances.push(instance);
      contexts.at(-1).header = header;
      contexts.at(-1).schema = schema;
      contexts.at(-1).instance = instance;
      continue;
    }
    const body = statement.mask.match(new RegExp(`^\\s*(${PATH})\\s*\\{\\s*$`));
    if (mode === "instance" && body) {
      stack.push({ kind: "body", path: body[1].split(".").map(normalize), prefix, statement, schema });
    }
  }
  return { file, text, schemas, instances, contexts, blocks };
}

function createIndex(sources, cache = new Map()) {
  const live = new Set(sources.map((s) => s.file));
  for (const file of cache.keys()) if (!live.has(file)) cache.delete(file);
  const documents = sources.map(({ file, text }) => {
    const previous = cache.get(file);
    if (previous?.text === text) return previous;
    const parsed = parseSource(file, text);
    cache.set(file, parsed);
    return parsed;
  });
  const schemas = new Map();
  for (const document of documents) for (const schema of document.schemas) {
    // An ambiguous schema has no trusted shape; definitions can still expose
    // both declarations while compiler lint explains the collision.
    schemas.set(schema.name, [...(schemas.get(schema.name) || []), schema]);
  }
  return { documents, schemas, instances: documents.flatMap((d) => d.instances) };
}

function schemaFields(index, schema) {
  const matches = index.schemas.get(schema);
  return matches?.length === 1 ? matches[0].fields : [];
}

function children(index, field) {
  return field?.fields || (field?.type.startsWith("$(") ? schemaFields(index, field.target) : []);
}

function resolveField(index, schema, segments) {
  let fields = schemaFields(index, schema);
  let result;
  for (let i = 0; i < segments.length; i += 1) {
    const matches = fields.filter((f) => f.key === normalize(segments[i]));
    if (matches.length !== 1) return undefined;
    result = matches[0];
    if (i < segments.length - 1 && result.list) return undefined;
    fields = children(index, result);
  }
  return result;
}

function contextAt(document, offset) {
  const context = document.contexts.find((s) => offset >= s.start && offset <= s.end);
  if (!context) return undefined;
  const before = document.text.slice(context.start, offset);
  const line = before.slice(before.lastIndexOf("\n") + 1);
  const scan = scanLine(line);
  if (scan.inString || scan.comment < line.length) return undefined;
  return { ...context, before, maskedBefore: before.split("\n").map((s) => scanLine(s).masked).join("\n") };
}

const TYPES = ["text", "int", "float", "bool", "enum", "file", "image", "ref"];
function values(index, field) {
  if (!field) return [];
  if (field.enum) return field.enum.map((label) => ({ label, kind: "EnumMember", detail: field.type }));
  if (field.type === "bool") return ["true", "false"].map((label) => ({ label, kind: "Value", detail: "bool" }));
  if (field.type.startsWith("ref(")) return index.instances.filter((i) => i.schema === field.target)
    .map((i) => ({ label: i.name, kind: "Reference", detail: i.declaration, symbol: i }));
  return [];
}

function completions(index, file, offset) {
  const document = index.documents.find((d) => d.file === file);
  const context = document && contextAt(document, offset);
  if (!context) return [];
  const before = context.maskedBefore;
  const lastWord = before.match(/[A-Za-z0-9_-]*$/)[0];
  const from = offset - lastWord.length;
  const to = offset + document.text.slice(offset).match(/^[A-Za-z0-9_-]*/)[0].length;
  const result = (items, extra = {}) => items.map((item) => {
    let insert = item.insert;
    if (insert && /(?:\.\s*|:\s*)$/.test(insert) && /^\s*[:.]/.test(document.text.slice(to))) insert = item.label;
    return { from, to, ...item, ...(insert ? { insert } : {}), ...extra };
  });
  const schemas = [...index.schemas.keys()].map((label) => ({ label, kind: "Class", detail: `schema ${label}` }));
  if (/(?:\$|ref)\([A-Za-z0-9_]*$/.test(before) || /^\s*logic\s+[A-Za-z0-9_]*$/.test(before)) return result(schemas);
  if (context.mode === "schema") {
    if (new RegExp(`^\\s*${ID}(?:\\[[^\\]]*\\])?\\s*:\\s*[a-z]*$`).test(before)) {
      return result([...TYPES.map((label) => ({ label, kind: "TypeParameter", detail: "Abstract type",
        insert: ["enum", "file", "image", "ref"].includes(label) ? `${label}($1)` : label, snippet: true })),
        ...schemas.map((s) => ({ ...s, label: `$(${s.label})`, insert: `$(${s.label})` }))]);
    }
    if (/@[A-Za-z]*$/.test(before) && !/=/.test(before)) {
      return result(["optional", "since", "removed", ...(context.schemaDepth ? ["tag"] : [])]
        .map((label) => ({ label, kind: "Keyword", insert: ["since", "removed"].includes(label) ? `${label}(\${1:2})` : label, snippet: true })));
    }
    return [];
  }
  const header = before.match(new RegExp(`^\\s*(${SCHEMA})\\s*::`));
  const schema = header?.[1] || context.schema;
  const rootFields = schemaFields(index, schema);
  if (header) {
    const tagValue = before.match(new RegExp(`@(${ID})\\.([A-Za-z0-9_-]*)$`));
    if (tagValue) return result(values(index, rootFields.find((f) => f.key === normalize(tagValue[1]))));
    if (/@[A-Za-z0-9_-]*$/.test(before)) return result([
      { label: "id", insert: "id.${1:name}", snippet: true, kind: "Property", detail: "Project-unique instance identity" },
      ...rootFields.filter((f) => f.key !== "id" && !f.list && f.type !== "group" && !f.type.startsWith("$("))
        .map((f) => ({ label: f.key, insert: f.type === "bool" ? f.key : `${f.key}.`, kind: "Property", detail: f.type, symbol: f }))
    ]);
    return [];
  }
  if (context.mode === "logic") {
    const logicPath = before.match(new RegExp(`\\.(${PATH}\\.?|)$`));
    if (!logicPath) return result(["require", "if", "for", "derive", "derive?", "exists", "length", "contains", "version"]
      .map((label) => ({ label, kind: "Keyword" })));
    return result(fieldCompletions(index, schema, logicPath[1].split(".").slice(0, -1), false));
  }
  if (context.mode === "instance") {
    const clone = before.match(new RegExp(`^\\s*&(${ID})?$`));
    if (clone && !context.prefix.length) return result(index.instances.filter((i) => i.schema === schema && i !== context.instance)
      .map((i) => ({ label: i.name, insert: `${i.name}.*`, kind: "Reference", detail: i.declaration, symbol: i })));
    const assignment = before.match(new RegExp(`^\\s*(${PATH})\\s*:\\s*([\\s\\S]*)$`));
    if (assignment) {
      const field = resolveField(index, schema, [...context.prefix, ...assignment[1].split(".")]);
      const value = assignment[2];
      if (/^(?:[A-Za-z0-9_-]*|\[?\s*(?:[A-Za-z0-9_-]+\s*,\s*)*[A-Za-z0-9_-]*)$/.test(value.replace(/\n/g, " "))) return result(values(index, field));
      if (/#([A-Za-z0-9_-]*)$/.test(value)) return result(values(index, children(index, field).find((f) => f.tag)));
      return [];
    }
    const fieldPath = before.match(new RegExp(`^\\s*(${PATH}\\.?|)$`));
    if (fieldPath) return result(fieldCompletions(index, schema, [...context.prefix, ...fieldPath[1].split(".").slice(0, -1)], true));
    return [];
  }
  if (/^\s*[A-Za-z0-9_]*$/.test(before)) {
    if (/\.abt$/i.test(file)) return result([
      { label: "schema", insert: "schema ${1:Name} {\n\t$0\n}", snippet: true, kind: "Snippet" },
      { label: "logic", insert: "logic ${1:Name} {\n\t$0\n}", snippet: true, kind: "Snippet" },
      { label: "versions", insert: "versions ${1:1}..${2:2}", snippet: true, kind: "Snippet" }
    ]);
    return result(schemas.map((s) => ({ ...s, insert: `${s.label} :: @id.\${1:name}`, snippet: true })));
  }
  return [];
}

function fieldCompletions(index, schema, prefix, assign) {
  const parent = prefix.length ? resolveField(index, schema, prefix) : undefined;
  if (parent?.list) return [];
  const fields = prefix.length ? children(index, parent) : schemaFields(index, schema);
  return fields.filter((f) => prefix.length || !["id", "template"].includes(f.key)).map((f) => ({
    label: f.key, kind: f.fields || f.type.startsWith("$(") ? "Struct" : "Property", detail: f.declaration,
    insert: f.key + (assign ? (!f.list && children(index, f).length ? "." : ": ") : ""), symbol: f
  }));
}

function definitions(index, file, offset) {
  const document = index.documents.find((d) => d.file === file);
  const context = document && contextAt(document, offset);
  if (!context) return [];
  const local = offset - context.start;
  for (const match of context.mask.matchAll(new RegExp(`(?:schema\\s+|logic\\s+|ref\\(|\\$\\()(${SCHEMA})|^\\s*(${SCHEMA})\\s*::`, "g"))) {
    const name = match[1] || match[2];
    const start = match.index + match[0].lastIndexOf(name);
    if (local >= start && local <= start + name.length) return index.schemas.get(name) || [];
  }
  const pathMatch = context.mask.match(new RegExp(`^\\s*(${PATH})\\s*[:{]`));
  if (pathMatch && context.mode === "instance") {
    const start = context.mask.indexOf(pathMatch[1]);
    if (local >= start && local <= start + pathMatch[1].length) {
      const segments = pathMatch[1].split(".");
      let end = start;
      for (let i = 0; i < segments.length; i += 1) {
        end += segments[i].length;
        if (local <= end) {
          const field = resolveField(index, context.schema, [...context.prefix, ...segments.slice(0, i + 1)]);
          return field ? [field] : [];
        }
        end += 1;
      }
    }
    const field = resolveField(index, context.schema, [...context.prefix, ...pathMatch[1].split(".")]);
    if (field?.type.startsWith("ref(")) {
      const word = wordAt(context.mask, local);
      return index.instances.filter((i) => i.schema === field.target && i.name === normalize(word));
    }
  }
  const clone = context.mask.match(new RegExp(`^\\s*&(${ID})`));
  if (clone) return index.instances.filter((i) => i.schema === context.schema && i.name === normalize(clone[1]));
  return [];
}

function wordAt(text, offset) {
  const left = text.slice(0, offset).match(/[A-Za-z0-9_-]*$/)[0];
  const right = text.slice(offset).match(/^[A-Za-z0-9_-]*/)[0];
  return left + right;
}

// SPEC 5.4 gives an exact syntactic equivalence. Keep this deliberately narrow:
// a single scalar/path assignment, no comments removed, no multiline values,
// no list traversal, no annotations on the block, and no other statements.
function abbreviations(index, file) {
  const document = index.documents.find((d) => d.file === file);
  if (!document) return [];
  return document.blocks.flatMap((block) => {
    const body = document.text.slice(block.statement.end + 1, block.close.start);
    const lines = body.split(/\r?\n/).filter((line) => line.trim());
    if (lines.length !== 1) return [];
    const line = lines[0];
    const assignment = scanLine(line).masked.match(new RegExp(`^\\s*(${PATH})\\s*:\\s*.+$`));
    if (!assignment) return [];
    const prefix = [...block.prefix, ...block.path];
    if (!resolveField(index, block.schema, [...prefix, ...assignment[1].split(".")])) return [];
    const openRaw = document.text.slice(block.statement.start, block.statement.end);
    const closeRaw = document.text.slice(block.close.start, block.close.end);
    if (scanLine(openRaw).comment < openRaw.length || scanLine(closeRaw).comment < closeRaw.length) return [];
    const indent = openRaw.match(/^[ \t]*/)[0];
    const replacement = `${indent}${block.path.join(".")}.${line.trimStart()}`;
    return [{ start: block.statement.start, end: block.end, replacement,
      title: "Use a dotted path (same assignment, fewer lines)", detail: "SPEC 5.4: body blocks are path prefixes." }];
  });
}

module.exports = { normalize, scanLine, logicalLines, parseSource, createIndex, schemaFields,
  children, resolveField, contextAt, completions, definitions, abbreviations };
