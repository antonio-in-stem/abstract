const fs = require("fs/promises");

// Provider admission, not a bound on the compiler heap or the tolerant index.
const LIMITS = Object.freeze({ sourceBytes: 4 * 1024 * 1024, totalBytes: 16 * 1024 * 1024,
  maxSources: 1024, maxEntries: 32768, maxDirectories: 4096, maxDepth: 128 });
function budget(limits = LIMITS) { return { remaining: limits.totalBytes, limits }; }
function allowance(state) { return Math.min(state.limits.sourceBytes, state.remaining); }
function admit(bytes, state) {
  if (bytes > state.limits.sourceBytes) throw new Error("Schema source exceeds the 4 MiB per-source admission limit.");
  if (bytes > state.remaining) throw new Error("Schema project exceeds the 16 MiB aggregate source admission limit.");
}
function textBytes(text, state) {
  // UTF-8 has at least as many bytes as UTF-16 code units. Avoid encoding first.
  admit(text.length, state);
  const size = Buffer.byteLength(text, "utf8");
  admit(size, state);
  state.remaining -= size;
  return size;
}
function bytes(size, state) { admit(size, state); state.remaining -= size; }
async function readHandle(handle, state, check = () => {}) {
  check();
  const stat = await handle.stat();
  if (!stat.isFile()) throw new Error("Schema source is not a regular file.");
  admit(stat.size, state);
  const limit = allowance(state);
  // A size check alone is insufficient: the file can grow after fstat. Read
  // at most the remaining allowance plus one sentinel byte, in fixed chunks.
  const chunks = []; let size = 0;
  while (true) {
    check();
    const chunk = Buffer.allocUnsafe(Math.min(65536, limit + 1 - size));
    const { bytesRead } = await handle.read(chunk, 0, chunk.length, null);
    size += bytesRead;
    admit(size, state);
    if (!bytesRead) break;
    chunks.push(chunk.subarray(0, bytesRead));
  }
  check();
  const text = new TextDecoder("utf-8", { fatal: true }).decode(Buffer.concat(chunks, size));
  state.remaining -= size;
  return { text, bytes: size };
}
async function readSource(file, state, check) {
  const handle = await fs.open(file, "r");
  try { return await readHandle(handle, state, check); }
  finally { await handle.close(); }
}
function sourceCount(count) {
  if (count > LIMITS.maxSources) throw new Error("Schema project exceeds the 1,024-source admission limit.");
}

module.exports = { LIMITS, budget, admit, bytes, textBytes, readHandle, readSource, sourceCount };
