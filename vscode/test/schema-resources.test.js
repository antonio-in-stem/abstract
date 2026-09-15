const { test } = require("node:test");
const assert = require("node:assert/strict");
const fs = require("fs/promises");
const path = require("path");
const { temporaryWorkspace } = require("./test-workspace");
const { LIMITS, budget, textBytes, readHandle, readSource } = require("../src/schema-resources");
const { discoveryRoot, discover } = require("../src/project-index");

test("bounded reads admit exact bytes, UTF-8 and EOF; aggregate budget includes every source", async () => {
  const workspace = await temporaryWorkspace();
  try {
    const file = await workspace.write("utf8.ab", "😀ab");
    const state = budget({ sourceBytes: 6, totalBytes: 8 });
    assert.deepEqual(await readSource(file, state), { text: "😀ab", bytes: 6 });
    assert.equal(state.remaining, 2);
    assert.equal(textBytes("ab", state), 2);
    await assert.rejects(readSource(file, state), /aggregate source admission/);
    assert.throws(() => textBytes("😀", state), /aggregate source admission/);
  } finally { await workspace.dispose(); }
});

test("growth after fstat is rejected by actual bytes with a one-byte sentinel", async () => {
  const workspace = await temporaryWorkspace();
  try {
    const file = await workspace.write("growing.ab", "a");
    for (const limits of [{ sourceBytes: 8, totalBytes: 16 }, { sourceBytes: 16, totalBytes: 8 }]) {
      await fs.writeFile(file, "a");
      const handle = await fs.open(file, "r"); let readBytes = 0;
      const wrapper = {
        async stat() { const before = await handle.stat(); await fs.appendFile(file, "b".repeat(16)); return before; },
        async read(...args) { const result = await handle.read(...args); readBytes += result.bytesRead; return result; }
      };
      try { await assert.rejects(readHandle(wrapper, budget(limits)), /admission limit/); }
      finally { await handle.close(); }
      assert.equal(readBytes, 9);
    }
  } finally { await workspace.dispose(); }
});

test("readSource closes handles after size, invalid UTF-8 and cancellation errors", async () => {
  const workspace = await temporaryWorkspace();
  const realOpen = fs.open; const handles = [];
  try {
    fs.open = async (...args) => { const handle = await realOpen(...args); handles.push(handle); return handle; };
    const file = await workspace.write("source.ab", "abc");
    await assert.rejects(readSource(file, budget({ sourceBytes: 2, totalBytes: 8 })), /per-source admission/);
    await fs.writeFile(file, Buffer.from([0xff]));
    await assert.rejects(readSource(file, budget()), /encoded data was not valid/);
    await assert.rejects(readSource(file, budget(), () => { throw new Error("cancelled"); }), /cancelled/);
    assert.equal(handles.length, 3);
    for (const handle of handles) await assert.rejects(handle.stat(), /closed|EBADF/);
  } finally { fs.open = realOpen; await workspace.dispose(); }
});

test("bounded discovery rejects full membership and traversal limits, default remains complete", async () => {
  const workspace = await temporaryWorkspace();
  try {
    await workspace.write("data/a.ab", ""); await workspace.write("data/nested/b.abt", "");
    const root = await discoveryRoot(workspace.root, LIMITS);
    assert.equal((await discover(root)).length, 2);
    assert.deepEqual(await discover(root, LIMITS), await discover(root));
    await assert.rejects(discover(root, { ...LIMITS, maxSources: 1 }), /source admission/);
    await assert.rejects(discover(root, { ...LIMITS, maxDirectories: 1 }), /directory-count/);
    await assert.rejects(discover(root, { ...LIMITS, maxDepth: 0 }), /directory-depth/);
    await assert.rejects(discover(root, { ...LIMITS, maxEntries: 1 }), /directory-entry/);
    await workspace.write("irrelevant.txt", "");
    await assert.rejects(discoveryRoot(workspace.root, { ...LIMITS, maxEntries: 1 }), /directory-entry/);
    // A rejected iterator must close handles: the workspace remains movable.
    const moved = path.join(workspace.root, "renamed"); await fs.rename(root, moved);
    assert.equal((await discover(moved)).length, 2);
  } finally { await workspace.dispose(); }
});
