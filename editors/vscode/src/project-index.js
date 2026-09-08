const fs = require("fs/promises");
const path = require("path");
const { createIndex } = require("./language-model");

const isSource = (file) => /\.abt?$/i.test(file);
const ignored = (name) => name.startsWith(".") || ["node_modules", "target", "build", "out"].includes(name.toLowerCase());

async function* directoryEntries(directory, limits, visited) {
  if (!limits) { yield* await fs.readdir(directory, { withFileTypes: true }); return; }
  const handle = await fs.opendir(directory, { bufferSize: 32 });
  // The async iterator closes its directory handle on completion or rejection.
  for await (const entry of handle) {
    limits.check?.();
    if (++visited.entries > limits.maxEntries) throw new Error("Schema discovery exceeds its directory-entry admission limit.");
    yield entry;
  }
}

async function discoveryRoot(target, limits) {
  const absolute = await fs.realpath(target);
  const stat = await fs.stat(absolute);
  const directory = stat.isDirectory() ? absolute : path.dirname(absolute);
  for (let current = directory; ; current = path.dirname(current)) {
    if (path.basename(current).toLowerCase() === "data") return current;
    if (path.dirname(current) === current) break;
  }
  if (stat.isDirectory()) {
    const data = [];
    for await (const entry of directoryEntries(directory, limits, { entries: 0 })) {
      if (entry.name.toLowerCase() !== "data") continue;
      const full = path.join(directory, entry.name);
      if ((await fs.stat(full)).isDirectory()) data.push(full);
    }
    if (data.length > 1) throw new Error("Project contains more than one 'data' directory (SPEC 2.3).");
    if (data.length) return data[0];
  }
  return directory;
}

async function discover(root, limits) {
  const directories = new Set();
  const files = new Set();
  const visited = { entries: 0 };
  async function walk(directory, depth = 0) {
    limits?.check?.();
    if (limits && depth > limits.maxDepth) throw new Error("Schema discovery exceeds its directory-depth admission limit.");
    const canonical = await fs.realpath(directory);
    if (directories.has(canonical)) return;
    directories.add(canonical);
    if (limits && directories.size > limits.maxDirectories) throw new Error("Schema discovery exceeds its directory-count admission limit.");
    for await (const entry of directoryEntries(directory, limits, visited)) {
      const full = path.join(directory, entry.name);
      const stat = entry.isSymbolicLink() ? await fs.stat(full) : entry;
      if (stat.isDirectory()) {
        if (!ignored(entry.name)) await walk(full, depth + 1);
      } else if (stat.isFile() && isSource(entry.name)) {
        files.add(await fs.realpath(full));
        if (limits && files.size > limits.maxSources) throw new Error("Schema project exceeds the 1,024-source admission limit.");
      }
    }
  }
  await walk(root);
  return [...files].sort();
}

class ProjectIndex {
  constructor() { this.projects = new Map(); }
  invalidate() { this.projects.clear(); }
  async get(target, openSources = [], token) {
    let entry = this.projects.get(target);
    // Watchers invalidate immediately inside the workspace. A bounded cache
    // lifetime also refreshes configured projects outside watcher coverage.
    if (!entry || entry.expires < Date.now()) {
      entry = { disk: this.load(target), expires: Date.now() + 2000 };
      this.projects.set(target, entry);
      entry.disk.catch(() => { if (this.projects.get(target) === entry) this.projects.delete(target); });
    }
    const { root, sources, parsed } = await entry.disk;
    if (token?.isCancellationRequested) return undefined;
    const merged = new Map(sources.map((s) => [s.file, s]));
    // Unsaved buffers are the authoring source of truth. realpath also makes
    // an open alias/junction replace, rather than duplicate, its disk source.
    for (const source of openSources) {
      const canonical = await fs.realpath(source.file).catch(() => source.file);
      const relative = path.relative(root, source.file);
      const inside = relative && !relative.startsWith(`..${path.sep}`) && relative !== ".." && !path.isAbsolute(relative);
      if (merged.has(canonical) || (inside && isSource(source.file) && !relative.split(path.sep).slice(0, -1).some(ignored))) {
        merged.set(canonical, { file: canonical, text: source.text });
      }
    }
    return createIndex([...merged.values()], parsed);
  }
  async load(target) {
    const root = await discoveryRoot(target);
    const files = await discover(root);
    const sources = [];
    // Bound in-flight reads so a large project cannot exhaust file handles.
    for (let i = 0; i < files.length; i += 32) {
      sources.push(...await Promise.all(files.slice(i, i + 32).map(async (file) => ({ file, text: await fs.readFile(file, "utf8") }))));
    }
    return { root, sources, parsed: new Map() };
  }
}

module.exports = { ProjectIndex, discoveryRoot, discover, isSource };
