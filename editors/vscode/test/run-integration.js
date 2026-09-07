const path = require("path");
const { runTests } = require("@vscode/test-electron");
const { temporaryWorkspace } = require("./test-workspace");

async function main() {
  const workspace = await temporaryWorkspace();
  const compiler = process.env.ABSTRACT_COMPILER_PATH || path.resolve(__dirname, "../../../target/debug", process.platform === "win32" ? "abstract.exe" : "abstract");
  try {
    await workspace.write("one/data/schema.abt", "schema Owner {\nteam: text\n}\nschema Product {\nowner: $(Owner)\nstatus: enum(draft, active)\n}\n");
    await workspace.write("one/data/product.ab", "Product :: @id.x\nowner {\nteam: Example\n}\nstatus: draft\n");
    await workspace.write("two/data/schema.abt", "schema Other {\nflag: bool\n}\n");
    await workspace.write("two/data/other.ab", "Other :: @id.other\nflag: invalid\n");
    const file = await workspace.write("test.code-workspace", JSON.stringify({
      folders: [{ path: "one", name: "one" }, { path: "two", name: "two" }],
      settings: { "abstract.compilerPath": compiler, "security.workspace.trust.enabled": false }
    }));
    await runTests({
      version: "1.92.2",
      vscodeExecutablePath: process.env.ABSTRACT_VSCODE_PATH,
      extensionDevelopmentPath: path.resolve(__dirname, ".."),
      extensionTestsPath: path.join(__dirname, "integration.js"),
      extensionTestsEnv: { ABSTRACT_TEST_ROOT: workspace.root },
      launchArgs: [file, "--disable-extensions", "--disable-workspace-trust", "--skip-welcome", "--skip-release-notes",
        "--user-data-dir", path.join(workspace.root, "vscode-user"), "--extensions-dir", path.join(workspace.root, "vscode-extensions")]
    });
  } finally { await workspace.dispose(); }
}
main().catch((error) => { console.error(error); process.exitCode = 1; });
