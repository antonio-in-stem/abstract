const { performance } = require("perf_hooks");
const os = require("os");
const path = require("path");
const { ProjectIndex } = require("../src/project-index");
const { completions } = require("../src/language-model");
const { temporaryWorkspace } = require("./test-workspace");

function summary(samples) {
  const sorted = [...samples].sort((a, b) => a - b);
  return { p50Ms: +sorted[Math.floor(sorted.length * 0.5)].toFixed(3), p95Ms: +sorted[Math.floor(sorted.length * 0.95)].toFixed(3), maxMs: +sorted.at(-1).toFixed(3) };
}

async function main() {
  const workspace = await temporaryWorkspace();
  try {
    const schema = Array.from({ length: 50 }, (_, i) => `schema Shape${i} {\n${Array.from({ length: 20 }, (_, j) => `field_${j}: enum(first, second, third)`).join("\n")}\n}\n`).join("");
    await workspace.write("data/schema.abt", schema);
    for (let i = 0; i < 500; i += 1) await workspace.write(`data/item_${i}.ab`, `Shape${i % 50} :: @id.item_${i}\nfield_0: first\n`);
    const projects = new ProjectIndex();
    const before = performance.now();
    await projects.get(workspace.root);
    const cold = performance.now() - before;
    const samples = [];
    const file = path.join(workspace.root, "data/item_0.ab");
    for (let i = 0; i < 100; i += 1) {
      const text = `Shape0 :: @id.item_0\n${i % 2 ? "field" : "fie"}`;
      const start = performance.now();
      const index = await projects.get(workspace.root, [{ file, text }]);
      const proposals = completions(index, await require("fs/promises").realpath(file), text.length);
      if (proposals.length !== 20) throw new Error("Benchmark lost schema completions.");
      if (i >= 10) samples.push(performance.now() - start);
    }
    console.log(JSON.stringify({ node: process.version, platform: `${os.platform()} ${os.release()} ${os.arch()}`, cpu: os.cpus()[0].model,
      fixture: { files: 501, schemas: 50, fields: 1000, instances: 500 },
      coldIndexMs: +cold.toFixed(3), warmup: 10, samples: samples.length,
      changedBufferSnapshotAndCompletion: summary(samples), heapUsedMiB: +(process.memoryUsage().heapUsed / 1024 / 1024).toFixed(2),
      scope: "Node process, filesystem discovery + model; excludes VS Code RPC/rendering and compiler lint. One cold run is not a percentile." }, null, 2));
  } finally { await workspace.dispose(); }
}
main().catch((error) => { console.error(error); process.exitCode = 1; });
