// Tolerant editor index for SPEC 1.x. This is not a validator: diagnostics
// remain the compiler's responsibility. All offsets are UTF-16, as in VS Code.
const path = require("path");
const { CALC_FUNCTIONS, calcRegionAt, canStartCalcAt } = require("./arithmetic-context");

const ID = "[A-Za-z0-9_][A-Za-z0-9_-]*";
const SCHEMA = "[A-Za-z][A-Za-z0-9_]*";
const PATH = `${ID}(?:\\.${ID})*`;
const normalize = (name) => name.replace(/[A-Z-]/g, (c) => c === "-" ? "_" : c.toLowerCase());
const PUBLIC_NOMINATION_DETAIL = "Author nomination for public values; requires Abstract 1.1+. Export checks dependencies separately. Runtime bindings are still required.";
const CALC_COMPLETION_ARGUMENTS = { min: "${1:first}, ${2:second}", max: "${1:first}, ${2:second}",
  clamp: "${1:value}, ${2:low}, ${3:high}", div: "${1:dividend}, ${2:divisor}",
  pow: "${1:base}, ${2:integerExponent}" };

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
    const block = new RegExp(`^\\s*(?:(?:schema|logic)\\s+${SCHEMA}|${PATH}(?:\\[[^\\]]*\\])?(?:\\s+@(?:optional|public|tag|since\\(\\d+\\)|removed\\(\\d+\\)))*)\\s*\\{\\s*$`).test(mask)
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
    contexts.push({ ...statement, mode, schema, instance, prefix,
      schemaPrefix: stack.filter((entry) => entry.kind === "group").map((entry) => entry.symbol.key),
      schemaDepth: mode === "schema" ? stack.length - 1 : 0,
      loops: stack.filter((entry) => entry.loop).map((entry) => entry.loop) });
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
      // Tolerant declaration metadata only: no eligibility or dependency claim.
      entry.public = /@public(?![A-Za-z0-9_-])/.test(field[3].split("=", 1)[0]);
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
      const loop = statement.mask.match(new RegExp(`^\\s*for\\s+\\$(${ID})\\s+in\\s+(.+?)\\s*\\{\\s*$`));
      if (/\{\s*$/.test(statement.mask)) stack.push({ kind: "logicBody",
        ...(loop ? { loop: { name: normalize(loop[1]), iterable: loop[2].trim() } } : {}) });
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
  if (/^(?:file|image)\(/.test(field.type)) {
    const allowed = new Set((field.type.match(/^[^(]+\(([^)]*)/)?.[1] || "").split(",")
      .map((part) => part.trim().split(/\s+/)[0].toLowerCase()).filter(Boolean));
    return (index.assets || []).filter((asset) => !allowed.size || allowed.has(path.extname(asset).slice(1).toLowerCase()))
      .map((label) => ({ label, kind: "File", detail: field.type }));
  }
  return [];
}

function fieldsAt(index, rootFields, segments) {
  let fields = rootFields;
  let field;
  for (const segment of segments.filter(Boolean)) {
    const matches = fields.filter((entry) => entry.key === normalize(segment));
    if (matches.length !== 1) return undefined;
    field = matches[0]; fields = children(index, field);
  }
  return { field, fields };
}

function fieldItems(index, fields, prefix, assign) {
  const resolved = fieldsAt(index, fields, prefix);
  if (!resolved || resolved.field?.list) return [];
  return resolved.fields.map((field) => ({
    label: field.key, kind: children(index, field).length ? "Struct" : "Property", detail: field.declaration,
    insert: field.key + (assign ? (!field.list && children(index, field).length ? "." : ": ") : ""), symbol: field
  }));
}

function loopValue(index, schema, loops, name) {
  const loop = [...loops].reverse().find((entry) => entry.name === normalize(name));
  if (!loop) return undefined;
  if (loop.iterable.startsWith(".")) return resolveField(index, schema,
    loop.iterable.slice(1).replace(/\[\d+\]/g, "").split("."));
  const parent = loop.iterable.match(new RegExp(`^\\$(${ID})\\.(.+)$`));
  const base = parent && loopValue(index, schema, loops, parent[1]);
  return base ? fieldsAt(index, children(index, base), parent[2].replace(/\[\d+\]/g, "").split("."))?.field : undefined;
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
      const alreadyPublic = /@public(?![A-Za-z0-9_-])/.test(before.replace(/@[A-Za-z]*$/, ""));
      return result(["optional", ...(!alreadyPublic ? ["public"] : []), "since", "removed", ...(context.schemaDepth ? ["tag"] : [])]
        .map((label) => ({ label, kind: "Keyword", insert: ["since", "removed"].includes(label) ? `${label}(\${1:2})` : label,
          snippet: true, ...(label === "public" ? { detail: PUBLIC_NOMINATION_DETAIL } : {}) })));
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
    const calc = calcRegionAt(context.mask, offset - context.start);
    const calcStart = canStartCalcAt(context.mask, offset - context.start - lastWord.length);
    const loopPath = before.match(new RegExp(`\\$(${ID})(?:\\.(${PATH}\\.?|))?$`));
    if (loopPath) {
      const bound = loopValue(index, schema, context.loops || [], loopPath[1]);
      if (loopPath[2] !== undefined && bound) return result(fieldItems(index, children(index, bound), loopPath[2].split(".").slice(0, -1), false));
      if (loopPath[2] === undefined) return result((context.loops || []).map((entry) => ({ label: entry.name, kind: "Variable", detail: `loop variable $${entry.name}` })));
    }
    const logicPath = before.match(new RegExp(`\\.(${PATH}\\.?|)$`));
    if (calc && !logicPath) return result([
      ...CALC_FUNCTIONS.map((label) => ({ label, kind: "Function", detail: "Abstract numeric function",
        insert: `${label}(${CALC_COMPLETION_ARGUMENTS[label] || "${1:value}"})`, snippet: true })),
      { label: "version", kind: "Variable", detail: "Current project version" }
    ]);
    if (calcStart && !logicPath) return result([
      { label: "calc", kind: "Function", detail: "Explicit Abstract 1.2 numeric expression",
        insert: "calc(${1:expression})", snippet: true }
    ]);
    if (!logicPath) return result(["require", "if", "for", "derive", "derive?", "exists", "length", "contains", "version"]
      .map((label) => ({ label, kind: "Keyword" })));
    return result(fieldItems(index, schemaFields(index, schema), logicPath[1].split(".").slice(0, -1), false));
  }
  if (context.mode === "instance") {
    const clone = before.match(new RegExp(`^\\s*&(${ID})?$`));
    if (clone && !context.prefix.length) return result(index.instances.filter((i) => i.schema === schema && i !== context.instance)
      .map((i) => ({ label: i.name, insert: `${i.name}.*`, kind: "Reference", detail: i.declaration, symbol: i })));
    const tupleColumns = before.match(new RegExp(`^\\s*(${PATH})\\(([^)]*)$`));
    if (tupleColumns) {
      const field = resolveField(index, schema, [...context.prefix, ...tupleColumns[1].split(".")]);
      const current = tupleColumns[2].split(",").at(-1).trim();
      const used = new Set(tupleColumns[2].split(",").slice(0, -1).map((value) => normalize(value.trim())));
      const items = children(index, field).filter((entry) => !used.has(entry.key));
      return result(items.map((entry) => ({ label: entry.key, kind: "Field", detail: entry.declaration })),
        { from: offset - current.length, to: offset + document.text.slice(offset).match(/^[A-Za-z0-9_-]*/)[0].length });
    }
    const assignment = before.match(new RegExp(`^\\s*(${PATH})(?:\\(([^)]*)\\))?\\s*:\\s*([\\s\\S]*)$`));
    if (assignment) {
      const field = resolveField(index, schema, [...context.prefix, ...assignment[1].split(".")]);
      const columns = assignment[2]?.split(",").map((value) => normalize(value.trim())).filter(Boolean);
      const value = assignment[3];
      if (columns) {
        const row = value.match(/\(([^()]*)$/);
        if (row) {
          const cell = row[1].split(",").at(-1).trim();
          const column = children(index, field).find((entry) => entry.key === columns[row[1].split(",").length - 1]);
          return result(values(index, column), { from: offset - cell.length,
            to: offset + document.text.slice(offset).match(/^[A-Za-z0-9_./\\-]*/)[0].length });
        }
      }
      const tag = value.match(new RegExp(`#${ID}\\(([^()]*)$`));
      if (tag) {
        const parts = tag[1].split(",");
        const argument = parts.at(-1).trimStart();
        const used = new Set(parts.slice(0, -1).map((part) => normalize(part.trim().split(/[:.]/)[0])));
        const assigned = argument.match(new RegExp(`^(${PATH})\\s*:\\s*([\\s\\S]*)$`));
        if (assigned) {
          const argumentField = fieldsAt(index, children(index, field), assigned[1].split("."))?.field;
          const token = assigned[2].trimStart();
          return result(values(index, argumentField), { from: offset - token.length,
            to: offset + document.text.slice(offset).match(/^[A-Za-z0-9_./\\-]*/)[0].length });
        }
        const prefix = argument.split("."); const word = prefix.pop() || "";
        return result(fieldItems(index, children(index, field), prefix, true)
          .filter((entry) => prefix.length || (!entry.symbol.tag && !used.has(entry.symbol.key))), { from: offset - word.length,
          to: offset + document.text.slice(offset).match(/^[A-Za-z0-9_-]*/)[0].length });
      }
      if (/^(?:[A-Za-z0-9_-]*|\[?\s*(?:[A-Za-z0-9_-]+\s*,\s*)*[A-Za-z0-9_-]*)$/.test(value.replace(/\n/g, " "))) return result(values(index, field));
      if (/#([A-Za-z0-9_-]*)$/.test(value)) return result(values(index, children(index, field).find((f) => f.tag)));
      if (/^(?:file|image)\(/.test(field?.type || "")) {
        const token = value.match(/[A-Za-z0-9_./\\-]*$/)?.[0] || "";
        return result(values(index, field), { from: offset - token.length,
          to: offset + document.text.slice(offset).match(/^[A-Za-z0-9_./\\-]*/)[0].length });
      }
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
  return fieldItems(index, schemaFields(index, schema), prefix, assign)
    .filter((field) => prefix.length || !["id", "template"].includes(field.label));
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
  if (context.mode === "schema") {
    const declaration = context.mask.match(new RegExp(`^\\s*(${ID})(?:\\[[^\\]]*\\])?\\s*(?::|@|\\{)`));
    if (declaration) {
      const start = context.mask.indexOf(declaration[1]);
      if (local >= start && local <= start + declaration[1].length) {
        const field = resolveField(index, context.schema, [...(context.schemaPrefix || []), declaration[1]]);
        return field ? [field] : [];
      }
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

function instanceFieldTarget(index, file, offset) {
  const document = index.documents.find((entry) => entry.file === file);
  const context = document && contextAt(document, offset);
  if (!context || context.mode !== "instance" || !context.instance) return undefined;
  const match = context.mask.match(new RegExp(`^\\s*(${PATH})\\s*(?:[:{(])`));
  if (!match) return undefined;
  const local = offset - context.start;
  const start = context.mask.indexOf(match[1]);
  if (local < start || local > start + match[1].length) return undefined;
  const segments = match[1].split(".");
  let end = start;
  for (let index = 0; index < segments.length; index += 1) {
    end += segments[index].length;
    if (local <= end) return { id: context.instance.name,
      path: [...context.prefix, ...segments.slice(0, index + 1)].map(normalize) };
    end += 1;
  }
  return undefined;
}

// SPEC 5.4 gives an exact syntactic equivalence. Keep this deliberately narrow:
// direct assignments, no comments on braces, no list traversal and no nested
// block in the selected replacement. SPEC 5.4 defines this path-prefix rewrite.
function abbreviations(index, file) {
  const document = index.documents.find((d) => d.file === file);
  if (!document) return [];
  return document.blocks.flatMap((block) => {
    const prefix = [...block.prefix, ...block.path];
    const openRaw = document.text.slice(block.statement.start, block.statement.end);
    const closeRaw = document.text.slice(block.close.start, block.close.end);
    if (scanLine(openRaw).comment < openRaw.length || scanLine(closeRaw).comment < closeRaw.length) return [];
    const indent = openRaw.match(/^[ \t]*/)[0];
    const eol = document.text.slice(block.statement.end, block.statement.end + 2) === "\r\n" ? "\r\n" : "\n";
    const body = document.text.slice(block.statement.end + eol.length, block.close.start);
    const statements = logicalLines(body);
    let cursor = 0; const rewritten = [];
    for (const statement of statements) {
      const raw = body.slice(cursor, statement.end + (body[statement.end] === "\r" ? 1 : 0));
      cursor = statement.end + 1;
      if (!raw.trim() || /^\s*\/\//.test(raw)) { rewritten.push(raw); continue; }
      const assignment = statement.mask.match(new RegExp(`^\\s*(${PATH})\\s*:\\s*.+$`));
      if (!assignment || !resolveField(index, block.schema, [...prefix, ...assignment[1].split(".")])) return [];
      const leading = raw.match(/^[ \t]*/)[0];
      rewritten.push(`${indent}${block.path.join(".")}.${raw.slice(leading.length)}`);
    }
    if (!rewritten.some((line) => line.trim() && !/^\s*\/\//.test(line))) return [];
    const replacement = rewritten.join(eol).replace(/\r?\n$/, "");
    return [{ start: block.statement.start, end: block.end, replacement,
      title: "Flatten body block to dotted paths", detail: "SPEC 5.4: body blocks are path prefixes." }];
  });
}

module.exports = { normalize, scanLine, logicalLines, parseSource, createIndex, schemaFields, PUBLIC_NOMINATION_DETAIL,
  children, resolveField, contextAt, completions, definitions, instanceFieldTarget, abbreviations, values, loopValue };
