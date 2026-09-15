const { performance } = require("perf_hooks");
const os = require("os");
const path = require("path");
const fs = require("fs/promises");
const cp = require("child_process");
const { createHash } = require("crypto");
const protocol = require("../src/analysis-client");
const { discoveryRoot, discover } = require("../src/project-index");
const { temporaryWorkspace } = require("./test-workspace");

async function main() {
  const compiler = process.env.ABSTRACT_COMPILER_PATH || path.resolve(__dirname, "../../target/release", process.platform === "win32" ? "abstract.exe" : "abstract");
  const compilerSha256 = createHash("sha256").update(await fs.readFile(compiler)).digest("hex");
  const git = (args) => cp.execFileSync("git", args, { cwd: path.resolve(__dirname, "../.."), encoding: "utf8", shell: false, windowsHide: true }).trim();
  const revision = git(["rev-parse", "HEAD"]);
  const workingTreeDirty = git(["status", "--porcelain"]).length > 0;
  const workspace = await temporaryWorkspace();
  try {
    await workspace.write("data/schema.abt", "schema Item {\nlabel: int\nicon: file(txt)\n}\n");
    await workspace.write("assets/note.txt", "asset checked at its original path");
    for (let i = 0; i < 500; i += 1) await workspace.write(`data/item_${i}.ab`, `Item :: @id.item_${i}\nlabel: ${i}\nicon: ./note.txt\n`);
    const file = path.join(workspace.root, "data/item_0.ab");
    const measurements = { clientPreparationMs: [], compilerRoundTripMs: [], completeRequestMs: [] };
    for (let i = 0; i < 25; i += 1) {
      const preparationStart = performance.now();
      const members = await discover(await discoveryRoot(workspace.root));
      if (members.length !== 501) throw new Error("Analysis benchmark membership discovery failed.");
      const input = protocol.encodeRequest(i, [{ path: file, text: `Item :: @id.item_0\nlabel: ${i}\nicon: ./note.txt\n` }]);
      const start = performance.now();
      const result = await protocol.startProcess(compiler, ["analyze", workspace.root, "--stdio"], { input }).promise;
      const response = protocol.response(result, i);
      if (!response.analyzed || response.diagnostics.length) throw new Error("Analysis benchmark fixture failed validation.");
      const end = performance.now();
      if (i >= 5) {
        measurements.clientPreparationMs.push(start - preparationStart);
        measurements.compilerRoundTripMs.push(end - start);
        measurements.completeRequestMs.push(end - preparationStart);
      }
    }
    const summary = (observations) => {
      const times = [...observations].sort((a, b) => a - b);
      const percentile = (fraction) => +times[Math.ceil(times.length * fraction) - 1].toFixed(3);
      return { p50: percentile(0.5), p95: percentile(0.95), max: percentile(1) };
    };
    console.log(JSON.stringify({ node: process.version, platform: `${os.platform()} ${os.release()} ${os.arch()}`, cpu: os.cpus()[0].model.trim(),
      compiler, compilerSha256, revision, workingTreeDirty, recordedAt: new Date().toISOString(),
      fixture: { sources: 501, instances: 500, assets: 1, overlays: 1 }, warmup: 5, samples: measurements.completeRequestMs.length,
      ...Object.fromEntries(Object.entries(measurements).map(([name, times]) => [name, summary(times)])),
      rawSamplesMs: measurements.completeRequestMs.map((_, index) => ({ sample: index + 1,
        ...Object.fromEntries(Object.entries(measurements).map(([name, times]) => [name, times[index]])) })),
      scope: "Client canonical membership discovery/encoding + release compiler startup/full discovery/compile/assets + response parsing; warm OS cache. Excludes 250 ms debounce, capability negotiation, VS Code document snapshot/RPC/rendering. Percentiles use nearest rank. Sequential observations are not independent trials or a population estimate." }, null, 2));
  } finally { await workspace.dispose(); }
}
main().catch((error) => { console.error(error); process.exitCode = 1; });
