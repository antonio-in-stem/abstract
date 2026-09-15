const { ExactNumber } = require("./protocol-json");

const own = (value, key) => Object.prototype.hasOwnProperty.call(value, key);
const record = (value) => value && typeof value === "object" && !Array.isArray(value) && !(value instanceof ExactNumber);
const u32 = (value) => {
  const source = value instanceof ExactNumber ? value.source : String(value);
  if (!/^(?:0|[1-9][0-9]*)$/.test(source)) throw new Error("Invalid compiled version range.");
  const number = Number(source);
  if (!Number.isSafeInteger(number) || number > 0xFFFFFFFF) throw new Error("Invalid compiled version range.");
  return number;
};

function compiledValues(response) {
  const values = response.values;
  if (!values || values.version !== 1 || typeof values.complete !== "boolean") throw new Error("Invalid compiler value projection.");
  if (!values.complete) throw new Error(typeof values.reason === "string" && values.reason ? values.reason : "Compiler values are unavailable.");
  if (!record(values.document) || !record(values.document.abstract) || !record(values.document.abstract.versions)
      || !Array.isArray(values.document.data) || !Array.isArray(values.document.overlays)
      || values.document.data.length > 65536 || values.document.overlays.length > 65536) {
    throw new Error("Invalid compiled value document.");
  }
  const min = u32(values.document.abstract.versions.min); const max = u32(values.document.abstract.versions.max);
  if (min > max) throw new Error("Invalid compiled version range.");
  const checkObject = (entry) => {
    if (!record(entry) || typeof entry.id !== "string" || typeof entry.template !== "string") throw new Error("Invalid compiled instance object.");
  };
  values.document.data.forEach(checkObject);
  const ids = new Set(values.document.data.map((entry) => entry.id));
  if (ids.size !== values.document.data.length) throw new Error("Duplicate compiled instance identity.");
  for (const overlay of values.document.overlays) {
    if (!record(overlay) || !record(overlay.versions) || !Array.isArray(overlay.data) || !Array.isArray(overlay.removed)
        || overlay.data.length > 65536 || overlay.removed.length > 65536) throw new Error("Invalid compiled overlay.");
    const lo = u32(overlay.versions.min); const hi = u32(overlay.versions.max);
    if (lo < min || hi > max || lo > hi) throw new Error("Invalid compiled overlay range.");
    overlay.data.forEach(checkObject);
    if (overlay.removed.some((id) => typeof id !== "string")) throw new Error("Invalid compiled removal identity.");
    const dataIds = new Set(overlay.data.map((entry) => entry.id));
    const removedIds = new Set(overlay.removed);
    if (dataIds.size !== overlay.data.length || removedIds.size !== overlay.removed.length
        || [...dataIds].some((id) => removedIds.has(id))) throw new Error("Ambiguous compiled overlay identity.");
  }
  return { document: values.document, min, max };
}

function exactJson(value, depth = 0) {
  if (depth > 32) return "<value nesting exceeds hover limit>";
  if (value instanceof ExactNumber) return value.source;
  if (typeof value === "string") return JSON.stringify(value);
  if (typeof value === "boolean") return String(value);
  if (value === null) return "null";
  if (Array.isArray(value)) return `[${value.map((entry) => exactJson(entry, depth + 1)).join(", ")}]`;
  if (record(value)) return `{ ${Object.entries(value).map(([key, entry]) => `${JSON.stringify(key)}: ${exactJson(entry, depth + 1)}`).join(", ")} }`;
  throw new Error("Invalid compiled value.");
}

function atPath(instance, path) {
  if (!instance) return { kind: "instance-absent" };
  let value = instance;
  for (const segment of path) {
    if (!record(value) || !own(value, segment)) return { kind: "field-absent" };
    value = value[segment];
  }
  return { kind: "value", value, key: exactJson(value) };
}

function project(compiled, id, path) {
  const base = compiled.document.data.find((entry) => entry.id === id);
  const changes = [];
  for (const overlay of compiled.document.overlays) {
    const replacement = overlay.data.find((entry) => entry.id === id);
    const removed = overlay.removed.includes(id);
    if (!replacement && !removed) continue;
    if (replacement && removed) throw new Error("A compiled overlay both replaces and removes one instance.");
    changes.push({ min: u32(overlay.versions.min), max: u32(overlay.versions.max), state: atPath(replacement, path) });
  }
  changes.sort((a, b) => a.min - b.min || a.max - b.max);
  const runs = []; let cursor = compiled.min;
  const add = (min, max, state) => {
    if (min > max) return;
    const previous = runs.at(-1);
    const key = state.kind === "value" ? `value:${state.key}` : state.kind;
    if (previous && previous.max + 1 === min && previous.key === key) previous.max = max;
    else runs.push({ min, max, state, key });
  };
  for (const change of changes) {
    if (change.min < cursor) throw new Error("Overlapping compiled overlays for one instance.");
    add(cursor, change.min - 1, atPath(base, path)); add(change.min, change.max, change.state); cursor = change.max + 1;
  }
  add(cursor, compiled.max, atPath(base, path));
  return runs;
}

function markdown(compiled, id, path) {
  const runs = project(compiled, id, path);
  const lines = ["**Effective compiled value**"];
  for (const run of runs) {
    const versions = run.min === run.max ? `version ${run.min}` : `versions ${run.min}–${run.max}`;
    const rendered = run.state.kind === "value" ? run.state.key
      : run.state.kind === "instance-absent" ? "instance absent" : "field absent";
    lines.push(`- ${versions}: \`${rendered.replace(/`/g, "\\`")}\``);
  }
  const output = lines.join("\n");
  return output.length <= 32768 ? output : "**Effective compiled value**\n\nValue is too large to display in a hover.";
}

module.exports = { compiledValues, exactJson, project, markdown };
