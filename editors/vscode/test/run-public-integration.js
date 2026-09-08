const path = require("path");
const fs = require("fs/promises");
const crypto = require("crypto");
const { downloadAndUnzipVSCode } = require("@vscode/test-electron");
const { startProcess } = require("../src/analysis-client");
const { temporaryWorkspace } = require("./test-workspace");

async function main() {
  const workspace = await temporaryWorkspace();
  const compiler = process.env.ABSTRACT_COMPILER_PATH || path.resolve(__dirname, "../../../target/release/abstract.exe");
  const version = process.env.ABSTRACT_TEST_VSCODE_VERSION || "1.92.0";
  const restricted = process.env.ABSTRACT_TEST_RESTRICTED === "1";
  const evidence = process.env.ABSTRACT_PUBLIC_TEST_EVIDENCE;
  const fixtures = {
    "public/data/model.abt": "schema ASettings {\r\n// 😀 before nomination\r\npreview: bool @public = true\r\nresult: bool = false\r\n}\r\nschema ZUnused {\r\nunused: bool @public = false\r\n}\r\n",
    "public/data/item.ab": "ASettings :: @id.main\n",
    "other/data/model.abt": "schema Other {\nvalue: bool\n}\n",
    "other/data/item.ab": "Other :: @id.other\nvalue: invalid\n",
    "empty/data/model.abt": "schema Plain {\nvalue: bool = true\n}\n",
    "empty/data/item.ab": "Plain :: @id.main\n",
    "shared/model.abt": "schema AShared {\nvisible: bool @public = true\n}\n",
    "linked/data/item.ab": "AShared :: @id.main\n",
  };
  try {
    for (const [name, text] of Object.entries(fixtures)) await workspace.write(name, text);
    await fs.symlink(path.join(workspace.root, "shared"), path.join(workspace.root, "linked/data/shared"),
      process.platform === "win32" ? "junction" : "dir");
    await workspace.write("vscode-user/User/settings.json", JSON.stringify({
      "security.workspace.trust.enabled": restricted,
      "security.workspace.trust.startupPrompt": "never",
      "telemetry.telemetryLevel": "off", "update.mode": "none",
    }));
    const file = await workspace.write("test.code-workspace", JSON.stringify({
      folders: ["public", "other", "empty", "linked"].map((name) => ({ name, path: name })),
      settings: { "abstract.compilerPath": compiler },
    }));
    const executable = process.env.ABSTRACT_VSCODE_PATH || await downloadAndUnzipVSCode({
      version, cachePath: path.resolve(__dirname, "../.vscode-test"),
    });
    process.env.ABSTRACT_TEST_ROOT = workspace.root;
    if (evidence) {
      await fs.mkdir(path.join(evidence, "fixtures"), { recursive: true });
      for (const [name, text] of Object.entries(fixtures)) {
        const target = path.join(evidence, "fixtures", name);
        await fs.mkdir(path.dirname(target), { recursive: true });
        await fs.writeFile(target, text, "utf8");
      }
      const hash = (bytes) => crypto.createHash("sha256").update(bytes).digest("hex");
      await fs.writeFile(path.join(evidence, "host.json"), JSON.stringify({
        executable, requestedVersion: version, restricted, compiler,
        executableSha256: hash(await fs.readFile(executable)),
        compilerSha256: hash(await fs.readFile(compiler)),
        fixtureHashes: Object.fromEntries(Object.entries(fixtures).map(([name, text]) => [name, hash(Buffer.from(text))])),
        limits: "Real isolated Extension Host and QuickPick command behavior; no pixel/rendering or whole-host isolation claim.",
      }, null, 2) + "\n");
    }
    const result = await startProcess(executable, [file, "--disable-extensions",
      ...(!restricted ? ["--disable-workspace-trust"] : []), "--no-sandbox", "--disable-gpu-sandbox",
      "--disable-updates", "--skip-welcome", "--skip-release-notes",
      `--extensionDevelopmentPath=${path.resolve(__dirname, "..")}`,
      `--extensionTestsPath=${path.join(__dirname, "public-inventory-integration.js")}`,
      "--user-data-dir", path.join(workspace.root, "vscode-user"),
      "--extensions-dir", path.join(workspace.root, "vscode-extensions"),
    ], { timeout: 90000 }).promise;
    process.stdout.write(result.stdout); process.stderr.write(result.stderr);
    if (result.error) throw result.error;
  } finally { await workspace.dispose(); }
}
main().catch((error) => { console.error(error); process.exitCode = 1; });
