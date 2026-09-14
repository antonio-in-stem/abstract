const cp = require("child_process");
const path = require("path");
const { parseProtocolJson } = require("./protocol-json");

const LIMITS = Object.freeze({ request: 16 * 1024 * 1024, text: 4 * 1024 * 1024, path: 32768, overlays: 128, response: 4 * 1024 * 1024 });
function unicode(text) {
  for (const c of text) {
    const value = c.codePointAt(0);
    if (value >= 0xD800 && value <= 0xDFFF) throw new Error("An unsaved buffer contains an unpaired UTF-16 surrogate; analysis cannot encode it as UTF-8.");
  }
}

function encodeRequest(id, overlays) {
  if (!Number.isInteger(id) || id < 0 || id > 0xFFFFFFFF || overlays.length > LIMITS.overlays) throw new Error("Analysis request id/overlay count exceeds protocol limits.");
  const header = Buffer.alloc(16);
  header.write("ABANLZ01", "ascii");
  header.writeUInt32BE(id, 8);
  header.writeUInt32BE(overlays.length, 12);
  const parts = [header];
  let size = header.length;
  for (const overlay of overlays) {
    if (typeof overlay.text !== "string" || typeof overlay.path !== "string" || !path.isAbsolute(overlay.path)) throw new Error("Analysis overlays require absolute paths and source text.");
    unicode(overlay.text); unicode(overlay.path);
    const pathLength = Buffer.byteLength(overlay.path);
    const textLength = Buffer.byteLength(overlay.text);
    size += 8 + pathLength + textLength;
    if (!pathLength || pathLength > LIMITS.path || textLength > LIMITS.text || size > LIMITS.request) throw new Error("Unsaved sources exceed the analysis protocol limits (128 buffers, 4 MiB per buffer, 16 MiB total).");
    const lengths = Buffer.alloc(8);
    lengths.writeUInt32BE(pathLength, 0); lengths.writeUInt32BE(textLength, 4);
    parts.push(lengths, Buffer.from(overlay.path), Buffer.from(overlay.text));
  }
  return Buffer.concat(parts, size);
}

// A process is the cancellation boundary. Native compiler work and its worker
// thread terminate together; there is no shell or shared long-lived server.
function startProcess(compiler, args, { input, cwd, timeout = 30000 } = {}) {
  let child;
  let cancelled = false;
  const promise = new Promise((resolve) => {
    child = cp.execFile(compiler, args, { cwd, shell: false, windowsHide: true, timeout, maxBuffer: LIMITS.response },
      (error, stdout, stderr) => resolve({ error, stdout, stderr, cancelled }));
    child.stdin?.on("error", () => {}); // Early rejection/exit closes the pipe; callback owns the result.
    child.stdin?.end(input);
  });
  return { promise, cancel() { cancelled = true; if (child?.exitCode === null) child.kill("SIGKILL"); } };
}

function capability(result) {
  if (result.cancelled) return undefined;
  if (result.error) {
    if (result.error.code === 2 && /error\[E801\]: Unknown command 'analyze';/.test(result.stderr)) {
      return { supported: false, reason: "This compiler does not support analysis overlays. Diagnostics use saved files; save all Abstract buffers to validate." };
    }
    throw new Error(result.stderr.trim() || result.error.message);
  }
  let value;
  try { value = JSON.parse(result.stdout); } catch { throw new Error("The compiler returned an invalid analysis capability response."); }
  if (value.protocol !== "abstract-analysis" || value.version !== 1 || value.positionEncoding !== "utf-16") {
    return { supported: false, reason: "This compiler's analysis protocol is unsupported. Diagnostics use saved files; save all Abstract buffers to validate." };
  }
  return { supported: true, compiler: value.compiler, schemaBindings: value.schemaBindings === 1,
    publicInventory: value.publicInventory === 1, values: value.values === 1 };
}

function response(result, expectedId) {
  if (result.cancelled) return undefined;
  if (result.error) throw new Error(result.stderr.trim() || result.error.message);
  let value;
  try { value = parseProtocolJson(result.stdout); } catch { throw new Error("The compiler returned invalid analysis JSON."); }
  if (value.protocol !== "abstract-analysis" || value.version !== 1 || value.requestId !== expectedId || value.positionEncoding !== "utf-16"
      || typeof value.analyzed !== "boolean" || typeof value.truncated !== "boolean" || !Array.isArray(value.diagnostics) || value.diagnostics.length > 100) {
    throw new Error("The compiler returned an incompatible or stale analysis response.");
  }
  const locationValid = (item) => {
    if (item.path !== undefined && (typeof item.path !== "string" || !path.isAbsolute(item.path))) return false;
    if (item.range === undefined) return true;
    const { start, end } = item.range || {};
    const position = (p) => p && Number.isInteger(p.line) && Number.isInteger(p.character) && p.line >= 0 && p.character >= 0 && p.line <= 0x7FFFFFFF && p.character <= 0x7FFFFFFF;
    return position(start) && position(end) && (end.line > start.line || (end.line === start.line && end.character >= start.character));
  };
  for (const item of value.diagnostics) {
    if (!/^E[1-8]\d\d$/.test(item.code) || item.severity !== "error" || typeof item.message !== "string" || !locationValid(item)
        || !Array.isArray(item.notes) || item.notes.length > 8 || item.notes.some((note) => typeof note.message !== "string" || !locationValid(note))) {
      throw new Error("The compiler returned a malformed analysis diagnostic.");
    }
  }
  return value;
}

module.exports = { LIMITS, encodeRequest, startProcess, capability, response };
