const fs = require("fs/promises");
const os = require("os");
const path = require("path");

async function temporaryWorkspace() {
  const root = await fs.mkdtemp(path.join(os.tmpdir(), "abstract-vscode-"));
  return {
    root,
    async write(relative, content) {
      const file = path.resolve(root, relative);
      if (!file.startsWith(root + path.sep)) throw new Error("Fixture path escaped its temporary workspace.");
      await fs.mkdir(path.dirname(file), { recursive: true });
      await fs.writeFile(file, content, "utf8");
      return file;
    },
    async dispose() {
      // Check the absolute deletion target before recursively cleaning fixtures.
      const resolved = path.resolve(root);
      if (path.dirname(resolved) !== path.resolve(os.tmpdir()) || !path.basename(resolved).startsWith("abstract-vscode-")) {
        throw new Error("Refusing cleanup outside the named temporary workspace.");
      }
      await fs.rm(resolved, { recursive: true, force: true, maxRetries: 10, retryDelay: 250 });
    }
  };
}

module.exports = { temporaryWorkspace };
