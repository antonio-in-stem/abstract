const path = require("path");
const fs = require("fs/promises");
const { downloadAndUnzipVSCode } = require("@vscode/test-electron");
const { startProcess } = require("../src/analysis-client");
const { temporaryWorkspace } = require("./test-workspace");

async function main() {
  const workspace = await temporaryWorkspace();
  const compiler = process.env.ABSTRACT_COMPILER_PATH || path.resolve(__dirname, "../../../target/debug", process.platform === "win32" ? "abstract.exe" : "abstract");
  const restricted = process.env.ABSTRACT_TEST_RESTRICTED === "1";
  try {
    await workspace.write("one/data/schema.abt", "schema Owner {\nteam: text\n}\nschema Product {\nowner: $(Owner)\nstatus: enum(draft, active)\n}\n");
    await workspace.write("one/data/product.ab", "Product :: @id.x\nowner {\nteam: Example\n}\nstatus: draft\n");
    await workspace.write("two/data/schema.abt", "schema Other {\nflag: bool\n}\n");
    await workspace.write("two/data/other.ab", "Other :: @id.other\nflag: invalid\n");
    await workspace.write("one/data/live.abt", "schema Asset {\nlabel: text\nicon: file(txt)\n}\n");
    await workspace.write("one/data/live.ab", "Asset :: @id.asset\nlabel: Disk\nicon: ./note.txt\n");
    await workspace.write("one/assets/note.txt", "actual project asset");
    await workspace.write("unicode/data/schema.abt", "\uFEFFschema Unicode { 😀: text }\r\n");
    await workspace.write("shared/schema.abt", "schema Shared {\nvalue: int\n}\n");
    for (const project of ["one", "two"]) {
      await workspace.write(`${project}/data/shared-instance.ab`, `Shared :: @id.${project}\nvalue: 7\n`);
      await fs.symlink(path.join(workspace.root, "shared"), path.join(workspace.root, project, "data/shared"), process.platform === "win32" ? "junction" : "dir");
    }
    if (restricted) await workspace.write("vscode-user/User/settings.json", JSON.stringify({
      "security.workspace.trust.enabled": true, "security.workspace.trust.startupPrompt": "never"
    }));
    const file = await workspace.write("test.code-workspace", JSON.stringify({
      folders: [{ path: "one", name: "one" }, { path: "two", name: "two" }],
      settings: { "abstract.compilerPath": compiler }
    }));
    const executable = process.env.ABSTRACT_VSCODE_PATH || await downloadAndUnzipVSCode({ version: "1.92.0", cachePath: path.resolve(__dirname, "../.vscode-test") });
    // vscode-test's runTests adds --disable-workspace-trust unconditionally.
    // Launch the official test host directly so Restricted Mode is exercised;
    // explicit profiles and argv also avoid a Windows shell around test paths.
    process.env.ABSTRACT_TEST_ROOT = workspace.root;
    const result = await startProcess(executable, [file, "--disable-extensions", ...(!restricted ? ["--disable-workspace-trust"] : []),
      "--no-sandbox", "--disable-gpu-sandbox", "--disable-updates", "--skip-welcome", "--skip-release-notes",
      `--extensionDevelopmentPath=${path.resolve(__dirname, "..")}`, `--extensionTestsPath=${path.join(__dirname, "integration.js")}`,
      "--user-data-dir", path.join(workspace.root, "vscode-user"), "--extensions-dir", path.join(workspace.root, "vscode-extensions")
    ], { timeout: 60000 }).promise;
    process.stdout.write(result.stdout);
    process.stderr.write(result.stderr);
    if (result.error) throw result.error;
  } finally { await workspace.dispose(); }
}
main().catch((error) => { console.error(error); process.exitCode = 1; });
