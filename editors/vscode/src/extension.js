const vscode = require("vscode");
const cp = require("child_process");
const path = require("path");
const fs = require("fs");
const { scanLine } = require("./language-model");
const { registerLanguageFeatures } = require("./providers");
const { registerDiagnostics } = require("./live-diagnostics");
const { registerSchemaFeatures } = require("./schema-providers");
const { registerPublicInventory } = require("./public-inventory-provider");
const { registerSyntaxHover } = require("./syntax-hover-provider");

const SEMANTIC_TOKEN_TYPES = [
  "abstractSchemaKeyword",
  "abstractSchemaType",
  "abstractSchemaModifier",
  "abstractSchemaField",
  "abstractLogicKeyword",
  "abstractLogicOperator",
  "abstractLogicPath",
  "abstractInterpolationVariable"
];

const SEMANTIC_LEGEND = new vscode.SemanticTokensLegend(SEMANTIC_TOKEN_TYPES, []);

function activate(context) {
  const diagnostics = vscode.languages.createDiagnosticCollection("abstract");
  context.subscriptions.push(diagnostics);

  registerLanguageFeatures(context, resolveProjectPath, (message) => getOutputChannel().appendLine(message));
  registerSchemaFeatures(context, resolveProjectPath, (message) => getOutputChannel().appendLine(message), workspaceRoot);
  registerPublicInventory(context, resolveProjectPath, (message) => getOutputChannel().appendLine(message), workspaceRoot);
  registerSyntaxHover(context);
  const live = registerDiagnostics(context, {
    diagnostics, resolveProjectPath, workspaceRoot,
    lintSaved: lintDocument,
    report: (message) => getOutputChannel().appendLine(message)
  });
  context.subscriptions.push({ dispose() { outputChannel?.dispose(); outputChannel = undefined; } });

  context.subscriptions.push(
    vscode.commands.registerCommand("abstract.useForWorkspace", async (resource) => {
      const document = resource ? await vscode.workspace.openTextDocument(resource) : vscode.window.activeTextEditor?.document;
      if (!document || document.uri.scheme !== "file" || !/\.abt?$/i.test(document.uri.fsPath) || !resolveProjectPath(document)) return false;
      const configuration = vscode.workspace.getConfiguration("files", document.uri);
      const associations = configuration.inspect("associations")?.workspaceValue || {};
      await configuration.update("associations", { ...associations, "*.ab": "abstract", "*.abt": "abstract" },
        vscode.ConfigurationTarget.Workspace);
      for (const open of vscode.workspace.textDocuments) {
        if (open.uri.scheme === "file" && /\.abt?$/i.test(open.uri.fsPath) && resolveProjectPath(open) && open.languageId !== "abstract") {
          await vscode.languages.setTextDocumentLanguage(open, "abstract");
        }
      }
      return true;
    }),
    vscode.commands.registerCommand("abstract.showDiagnosticStatus", () => {
      const state = live.readiness(vscode.window.activeTextEditor?.document);
      const message = `${state.label}: ${state.detail}`;
      return state.ready ? vscode.window.showInformationMessage(message) : vscode.window.showWarningMessage(message);
    }),
    vscode.languages.registerDocumentSemanticTokensProvider(
      { language: "abstract", scheme: "file" },
      { provideDocumentSemanticTokens },
      SEMANTIC_LEGEND
    ),
    vscode.commands.registerCommand("abstract.lintCurrentProject", async () => {
      const document = vscode.window.activeTextEditor?.document;
      if (!document) return;
      await live.schedule(document, true);
    }),
    vscode.commands.registerCommand("abstract.compileCurrentProject", async () => {
      const document = vscode.window.activeTextEditor?.document;
      if (!document || !vscode.workspace.isTrusted || document.uri.scheme !== "file") return false;
      try {
        if (await live.hasUnsaved(document)) {
          vscode.window.showWarningMessage("Save the Abstract files in this project, including linked schemas, before compiling.");
          return false;
        }
      } catch (error) {
        vscode.window.showWarningMessage(`Cannot verify saved project sources: ${error.message}`);
        return false;
      }
      const ran = await runCliCommand(document, "compile");
      if (ran) await live.schedule(document, true);
      return ran;
    })
  );

  const associationWarnings = new Set();
  const untitledWarnings = new Set();
  function reportUntitled(document) {
    if (!document?.isUntitled || document.languageId !== "abstract") return;
    const key = document.uri.toString();
    if (untitledWarnings.has(key)) return;
    untitledWarnings.add(key);
    vscode.window.showWarningMessage(
      "Save this Abstract document inside a project to enable compiler diagnostics.",
      "Save As…"
    ).then((choice) => {
      if (choice === "Save As…") vscode.commands.executeCommand("workbench.action.files.saveAs");
    });
  }
  function reportLanguageConflict(document) {
    if (document.uri.scheme !== "file" || document.languageId === "abstract" || !/\.abt?$/i.test(document.uri.fsPath)
        || !resolveProjectPath(document)) return;
    const folder = vscode.workspace.getWorkspaceFolder(document.uri);
    const key = folder?.uri.toString() || path.dirname(document.uri.fsPath);
    if (associationWarnings.has(key)) return;
    associationWarnings.add(key);
    vscode.window.showWarningMessage(
      `Abstract cannot provide diagnostics for ${path.basename(document.uri.fsPath)} because VS Code opened it as '${document.languageId}'.`,
      "Use Abstract for This Workspace"
    ).then((choice) => {
      if (choice === "Use Abstract for This Workspace") vscode.commands.executeCommand("abstract.useForWorkspace", document.uri);
    });
  }
  const reportDocument = (document) => { reportLanguageConflict(document); reportUntitled(document); };
  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument(reportDocument),
    vscode.window.onDidChangeActiveTextEditor((editor) => reportDocument(editor?.document)),
    vscode.workspace.onDidCloseTextDocument((document) => untitledWarnings.delete(document.uri.toString()))
  );
  for (const document of vscode.workspace.textDocuments) reportDocument(document);

}

function provideDocumentSemanticTokens(document) {
  const builder = new vscode.SemanticTokensBuilder(SEMANTIC_LEGEND);
  let blockKind = "";
  let blockDepth = 0;

  for (let lineIndex = 0; lineIndex < document.lineCount; lineIndex += 1) {
    const rawLine = document.lineAt(lineIndex).text;
    const line = maskStringsAndComments(rawLine);
    const tokens = [];

    // `versions` is a top-level declaration, not a block, so it is coloured
    // outside the block tracking below.
    addRegexTokens(tokens, line, /^\s*(versions)\b/g, "abstractSchemaKeyword", 1);

    const blockStart = line.match(/^\s*(schema|logic)\s+([A-Za-z][A-Za-z0-9_]*)\s*\{/);
    if (!blockKind && blockStart) {
      blockKind = blockStart[1];
      blockDepth = 0;
    }

    if (blockStart) {
      addToken(tokens, blockStart.index + blockStart[0].indexOf(blockStart[1]), blockStart[1].length,
        blockStart[1] === "schema" ? "abstractSchemaKeyword" : "abstractLogicKeyword");
    }

    if (blockKind === "schema") {
      // Every type keyword of SPEC §4.4: the seven that take arguments, the
      // four that also appear bare after ':', and $(Schema).
      addRegexTokens(
        tokens,
        line,
        /\b(text|int|float|enum|file|image|ref)\b(?=\s*\()|(?<=:\s*)\b(text|int|float|bool)\b(?!\s*\()|\$\([A-Za-z_][A-Za-z0-9_]*\)/g,
        "abstractSchemaType"
      );
      addRegexTokens(tokens, line, /@(optional|tag|since|removed)\b/g, "abstractSchemaModifier");
      const declarationEnd = line.indexOf("=");
      addRegexTokens(tokens, declarationEnd < 0 ? line : line.slice(0, declarationEnd),
        /@public(?![A-Za-z0-9_-])/g, "abstractSchemaModifier");
      addRegexTokens(tokens, line, /^\s*([A-Za-z0-9_][A-Za-z0-9_-]*)(?:\[[^\]]*\])?(?=\s*[:{@])/g, "abstractSchemaField", 1);
    } else if (blockKind === "logic") {
      addRegexTokens(tokens, line, /\b(if|require|else|throw|for|in|derive)\b/g, "abstractLogicKeyword");
      addRegexTokens(tokens, line, /\b(contains|exists|length|and|or|not|in|version)\b|&&|\|\||==|!=|>=|<=|>|</g, "abstractLogicOperator");
      addRegexTokens(tokens, line, /\.[A-Za-z0-9_][A-Za-z0-9_-]*(?:\.(?:[A-Za-z0-9_][A-Za-z0-9_-]*|\$[A-Za-z0-9_][A-Za-z0-9_-]*))*/g, "abstractLogicPath");
    }
    addRegexTokens(tokens, line, /\$[A-Za-z_][A-Za-z0-9_]*/g, "abstractInterpolationVariable");

    tokens
      .sort((left, right) => left.start - right.start || left.length - right.length)
      .forEach((token) => {
        const tokenType = SEMANTIC_TOKEN_TYPES.indexOf(token.type);
        if (tokenType >= 0 && token.length > 0) {
          builder.push(lineIndex, token.start, token.length, tokenType, 0);
        }
      });

    if (blockKind) {
      blockDepth += braceDelta(line);
      if (blockDepth <= 0) {
        blockKind = "";
        blockDepth = 0;
      }
    }
  }

  return builder.build();
}

function maskStringsAndComments(line) {
  return scanLine(line).masked;
}

function braceDelta(line) {
  let delta = 0;
  for (const ch of line) {
    if (ch === "{") delta += 1;
    if (ch === "}") delta -= 1;
  }
  return delta;
}

function addRegexTokens(tokens, line, regex, type, captureGroup = 0) {
  for (const match of line.matchAll(regex)) {
    const value = match[captureGroup];
    if (!value) continue;
    const start = match.index + (captureGroup ? match[0].indexOf(value) : 0);
    addToken(tokens, start, value.length, type);
  }
}

function addToken(tokens, start, length, type) {
  const end = start + length;
  if (tokens.some((token) => start < token.start + token.length && end > token.start)) return;
  tokens.push({ start, length, type });
}

// The compiler is always invoked with an argument vector and never through a
// shell (SPEC Appendix D.3 item 12). Paths from the workspace — the project
// root, the configured compiler path — reach the process as single argv
// entries, so a directory named `; rm -rf ~` is a directory name and not a
// command. There is no quoting to get right because there is no shell to quote
// for.
function runCompiler(compiler, args, cwd, registerOperation) {
  return new Promise((resolve) => {
    const child = cp.execFile(
      compiler,
      args,
      { cwd, windowsHide: true, shell: false, timeout: 30000, maxBuffer: 4 * 1024 * 1024 },
      (error, stdout, stderr) => resolve({ error, stdout, stderr })
    );
    registerOperation?.({ cancel() { if (child.exitCode === null) child.kill("SIGKILL"); } });
  });
}

function hasUnsavedProject(target) {
  return vscode.workspace.textDocuments.some((d) => d.languageId === "abstract" && d.isDirty && resolveProjectPath(d) === target);
}

async function lintDocument(document, state) {
  if (!vscode.workspace.isTrusted || document.uri.scheme !== "file") return;
  const compiler = vscode.workspace.getConfiguration("abstract", document.uri).get("compilerPath", "abstract");
  const target = resolveProjectPath(document);
  if (!target) {
    // A file with no resolvable project is not linted at all: a single .ab
    // file out of project context has no schemas, so every diagnostic it
    // produced would be noise (SPEC Appendix D.3 item 13).
    return;
  }
  if (hasUnsavedProject(target)) return;

  const { error, stdout, stderr } = await runCompiler(compiler, ["lint", target], workspaceRoot(document), state.setOperation);
  // Concurrent runs for one project cannot publish an older result, and a
  // successful project cannot erase diagnostics belonging to another root.
  if (!state.isCurrent() || hasUnsavedProject(target)) return;
  if (!error) return;

  const output = `${stderr || ""}\n${stdout || ""}`.trim();
  const parsed = parseDiagnostics(output, projectRootOf(target));
  if (parsed.length === 0) {
    // The compiler failed but said nothing this parser understands — a
    // missing binary, for instance. Report it once, on the open document,
    // rather than dropping it.
    const diagnostic = new vscode.Diagnostic(
      new vscode.Range(0, 0, 0, 0),
      output || String(error),
      vscode.DiagnosticSeverity.Error
    );
    diagnostic.source = "abstract";
    state.publish(document.uri, [diagnostic]);
    return;
  }

  const byFile = new Map();
  for (const item of parsed) {
    const uri = item.file ? vscode.Uri.file(item.file) : document.uri;
    const key = uri.fsPath;
    if (!byFile.has(key)) byFile.set(key, { uri, items: [] });
    byFile.get(key).items.push(item.diagnostic);
  }
  for (const { uri, items } of byFile.values()) {
    state.publish(uri, items);
  }
}

// SPEC §9.8:
//
//   <path>:<line>:<col>: error[<ID>]: <message>
//     note: <note text>
//
// with the shorter forms `<path>:<line>:`, `<path>:` and `abstract:` when a
// diagnostic has no column, no position, or no file. `--max-errors` means
// there is usually more than one, so every diagnostic in the output is
// reported, not just the first.
// `abstract` comes first so that the no-file form is never read as a path.
const DIAGNOSTIC_LINE =
  /^(?:abstract|(?<path>[^\s:][^:]*?)(?::(?<line>\d+))?(?::(?<col>\d+))?):\s*error\[(?<id>[A-Z]\d+)\]:\s*(?<message>.*)$/;

function parseDiagnostics(output, projectRoot) {
  const results = [];
  const sources = new Map();
  let current;

  function rangeFor(reported, line, col) {
    const lineIndex = Math.max(0, Number(line || 1) - 1);
    const scalarIndex = Math.max(0, Number(col || 1) - 1);
    const file = reported ? resolveReportedPath(reported, projectRoot) : "";
    if (!sources.has(file)) {
      const open = vscode.workspace.textDocuments.find((d) => d.uri.fsPath === file);
      let text = open?.getText();
      if (text === undefined && file) { try { text = fs.readFileSync(file, "utf8"); } catch { /* Missing source: keep the reported position. */ } }
      sources.set(file, text?.split(/\r?\n/));
    }
    const text = sources.get(file)?.[lineIndex];
    if (text === undefined) return new vscode.Range(lineIndex, scalarIndex, lineIndex, scalarIndex + 1);
    // Compiler columns count Unicode scalars (SPEC 9.8); VS Code counts
    // UTF-16 code units. A leading BOM is removed by the compiler (SPEC 2.2).
    const bom = lineIndex === 0 && text.startsWith("\uFEFF") ? 1 : 0;
    const scalars = Array.from(text.slice(bom));
    const columnIndex = bom + scalars.slice(0, scalarIndex).join("").length;
    return new vscode.Range(lineIndex, columnIndex, lineIndex, columnIndex + (scalars[scalarIndex]?.length || 0));
  }

  for (const raw of output.split(/\r?\n/)) {
    const match = raw.match(DIAGNOSTIC_LINE);
    if (match) {
      const { path: reported, line, col, id, message } = match.groups;
      const diagnostic = new vscode.Diagnostic(
        rangeFor(reported, line, col),
        message,
        vscode.DiagnosticSeverity.Error
      );
      diagnostic.source = "abstract";
      diagnostic.code = id;
      current = {
        file: reported ? resolveReportedPath(reported, projectRoot) : "",
        diagnostic
      };
      results.push(current);
      continue;
    }

    // A note belongs to the diagnostic above it. Notes that name a second
    // position become related information, which VS Code turns into a link;
    // the rest are appended to the message.
    const note = raw.match(/^\s+(?:(?<path>[^\s:][^:]*?):(?<line>\d+)(?::(?<col>\d+))?:\s*)?note:\s*(?<text>.*)$/);
    if (note && current) {
      const { path: reported, line, col, text } = note.groups;
      if (reported) {
        const location = new vscode.Location(
          vscode.Uri.file(resolveReportedPath(reported, projectRoot)),
          rangeFor(reported, line, col)
        );
        current.diagnostic.relatedInformation = [
          ...(current.diagnostic.relatedInformation || []),
          new vscode.DiagnosticRelatedInformation(location, text)
        ];
      } else {
        current.diagnostic.message += `\nnote: ${text}`;
      }
    }
  }

  return results;
}

// Diagnostic paths are project-root-relative and use '/' (SPEC §9.8).
function resolveReportedPath(reported, projectRoot) {
  const native = reported.split("/").join(path.sep);
  return path.isAbsolute(native) ? native : path.resolve(projectRoot, native);
}

// Diagnostic paths are relative to the project root, which is the parent of
// the data directory when there is one (SPEC §2.3). `abstract.projectPath` may
// legitimately name either, so both spellings resolve to the same root here.
function projectRootOf(target) {
  let directory;
  try {
    directory = fs.statSync(target).isDirectory() ? target : path.dirname(target);
  } catch (error) {
    directory = path.dirname(target);
  }
  return path.basename(directory).toLowerCase() === "data" ? path.dirname(directory) : directory;
}

async function runCliCommand(document, commandName) {
  if (!vscode.workspace.isTrusted || document.uri.scheme !== "file") return false;
  const compiler = vscode.workspace.getConfiguration("abstract", document.uri).get("compilerPath", "abstract");
  const target = resolveProjectPath(document);
  const channel = getOutputChannel();
  if (!target) {
    channel.clear();
    channel.appendLine(
      `Abstract: ${document.uri.fsPath} is not inside a project. Open a folder whose sources ` +
        "live under a 'data' directory, or set abstract.projectPath."
    );
    channel.show(true);
    vscode.window.showErrorMessage(`Abstract ${commandName} needs a project.`);
    return false;
  }
  if (hasUnsavedProject(target)) {
    vscode.window.showWarningMessage("Save the Abstract files in this project before running the compiler.");
    return false;
  }

  const format = vscode.workspace.getConfiguration("abstract", document.uri).get("outputFormat", "JSON");
  const args = commandName === "compile" ? [commandName, target, format] : [commandName, target];

  const { error, stdout, stderr } = await runCompiler(compiler, args, workspaceRoot(document));
  channel.clear();
  // Shown for the reader, never handed to a shell.
  channel.appendLine(`> ${[compiler, ...args].join(" ")}`);
  if (stdout) channel.appendLine(stdout.trimEnd());
  if (stderr) channel.appendLine(stderr.trimEnd());
  if (error) {
    vscode.window.showErrorMessage(`Abstract ${commandName} failed. See Abstract output.`);
  } else {
    vscode.window.showInformationMessage(`Abstract ${commandName} completed.`);
  }
  channel.show(true);
  return true;
}

let outputChannel;

function getOutputChannel() {
  if (!outputChannel) {
    outputChannel = vscode.window.createOutputChannel("Abstract");
  }
  return outputChannel;
}

// SPEC §2.3, applied to the open document. The extension always lints a
// project, never a lone file (Appendix D.3 item 13), so it resolves the
// project root the way the compiler does and passes that.
//
// Step 1: D is the directory containing the file. Step 2: walk D and its
// ancestors upwards for the first directory whose final component compares
// case-insensitively equal to `data`. Step 4: the project root is that
// directory's parent. The walk is upwards only — there is no search among D's
// children, and step 3 does not apply because it is never applied to the
// directory that contains a named file.
//
// Returns the path to lint, or "" when the document is not in a project.
function resolveProjectPath(document) {
  if (document.uri.scheme !== "file") return "";
  const configured = vscode.workspace.getConfiguration("abstract", document.uri).get("projectPath", "");
  if (configured) {
    return expandWorkspacePath(document, configured);
  }

  const dataDirectory = findDataDirectory(path.dirname(document.uri.fsPath));
  if (dataDirectory) {
    return path.dirname(dataDirectory);
  }

  // No `data` marker anywhere above the file. A project in that layout keeps
  // its sources in one directory, so that directory is the project — but only
  // when it holds a template, since a lone .ab file has no schema to validate
  // against and linting it would report nothing but noise.
  const directory = path.dirname(document.uri.fsPath);
  return directoryHasTemplate(directory) ? directory : "";
}

function findDataDirectory(startDirectory) {
  let current = startDirectory;
  while (true) {
    if (path.basename(current).toLowerCase() === "data") {
      return current;
    }
    const parent = path.dirname(current);
    if (parent === current) return "";
    current = parent;
  }
}

function directoryHasTemplate(directory) {
  try {
    return fs.readdirSync(directory).some((entry) => entry.toLowerCase().endsWith(".abt"));
  } catch (error) {
    return false;
  }
}

function workspaceRoot(document) {
  const folder = vscode.workspace.getWorkspaceFolder(document.uri);
  return folder ? folder.uri.fsPath : path.dirname(document.uri.fsPath);
}

function expandWorkspacePath(document, value) {
  const expanded = value
    .replace(/\$\{workspaceFolder\}/g, workspaceRoot(document))
    .replace(/\$\{workspaceFolder:([^}]+)\}/g, (_, name) =>
      vscode.workspace.workspaceFolders?.find((folder) => folder.name === name)?.uri.fsPath || workspaceRoot(document));
  return path.resolve(workspaceRoot(document), expanded);
}

function deactivate() {}

module.exports = { activate, deactivate };
