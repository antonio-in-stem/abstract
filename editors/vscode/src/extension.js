const vscode = require("vscode");
const cp = require("child_process");
const path = require("path");
const fs = require("fs");

const COMPLETIONS = [
  "schema",
  "logic",
  "versions 1..1",
  "text",
  "int",
  "float",
  "bool",
  "enum",
  "file",
  "image",
  "ref",
  "$(Schema)",
  "@optional",
  "@tag",
  "@since(2)",
  "@removed(2)",
  "Product :: @id.",
  "copy(key, value): (en_us, Welcome), (es_*, Bienvenido)",
  "tags: [core, public, ai_ready]",
  "owner.team: Knowledge Systems",
  "capabilities: [#search, #sync(availability: beta)]"
];

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

  const runLint = (document) => lintDocument(document, diagnostics);

  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument((document) => {
      if (document.languageId === "abstract") runLint(document);
    }),
    vscode.workspace.onDidSaveTextDocument((document) => {
      if (document.languageId === "abstract") runLint(document);
    }),
    vscode.languages.registerCompletionItemProvider("abstract", {
      provideCompletionItems() {
        return COMPLETIONS.map((label) => {
          const item = new vscode.CompletionItem(label, vscode.CompletionItemKind.Snippet);
          item.insertText = label;
          return item;
        });
      }
    }),
    vscode.languages.registerDocumentSemanticTokensProvider(
      { language: "abstract" },
      { provideDocumentSemanticTokens },
      SEMANTIC_LEGEND
    ),
    vscode.commands.registerCommand("abstract.lintCurrentProject", async () => {
      const document = vscode.window.activeTextEditor?.document;
      if (!document) return;
      await runCliCommand(document, "lint");
      await lintDocument(document, diagnostics);
    }),
    vscode.commands.registerCommand("abstract.compileCurrentProject", async () => {
      const document = vscode.window.activeTextEditor?.document;
      if (!document) return;
      await runCliCommand(document, "compile");
      await lintDocument(document, diagnostics);
    })
  );

  for (const document of vscode.workspace.textDocuments) {
    if (document.languageId === "abstract") runLint(document);
  }
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

    const blockStart = line.match(/\b(schema|logic)\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{/);
    if (!blockKind && blockStart) {
      blockKind = blockStart[1];
      blockDepth = 0;
    }

    if (blockStart) {
      addToken(tokens, blockStart.index, blockStart[1].length,
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
      addRegexTokens(tokens, line, /^\s*([A-Za-z_][A-Za-z0-9_]*)(?:\[\])?(?=\s*[:{])/g, "abstractSchemaField", 1);
    } else if (blockKind === "logic") {
      addRegexTokens(tokens, line, /\b(if|require|else|throw|for|in|derive)\b/g, "abstractLogicKeyword");
      addRegexTokens(tokens, line, /\b(contains|exists|length|and|or|not|in|version)\b|&&|\|\||==|!=|>=|<=|>|</g, "abstractLogicOperator");
      addRegexTokens(tokens, line, /\.[A-Za-z_][A-Za-z0-9_]*(?:\.(?:[A-Za-z_][A-Za-z0-9_]*|\$[A-Za-z_][A-Za-z0-9_]*))*/g, "abstractLogicPath");
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
  let output = "";
  let inString = false;
  for (let index = 0; index < line.length; index += 1) {
    const ch = line[index];
    const next = line[index + 1];
    if (!inString && ch === "/" && next === "/") {
      output += " ".repeat(line.length - index);
      break;
    }
    if (ch === "\"" && line[index - 1] !== "\\") {
      inString = !inString;
      output += " ";
      continue;
    }
    output += inString ? " " : ch;
  }
  return output;
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
function runCompiler(compiler, args, cwd) {
  return new Promise((resolve) => {
    cp.execFile(
      compiler,
      args,
      { cwd, windowsHide: true, shell: false },
      (error, stdout, stderr) => resolve({ error, stdout, stderr })
    );
  });
}

async function lintDocument(document, diagnostics) {
  const compiler = vscode.workspace.getConfiguration("abstract").get("compilerPath", "abstract");
  const target = resolveProjectPath(document);
  if (!target) {
    // A file with no resolvable project is not linted at all: a single .ab
    // file out of project context has no schemas, so every diagnostic it
    // produced would be noise (SPEC Appendix D.3 item 13).
    diagnostics.clear();
    return;
  }

  const { error, stdout, stderr } = await runCompiler(compiler, ["lint", target], workspaceRoot(document));
  diagnostics.clear();
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
    diagnostics.set(document.uri, [diagnostic]);
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
    diagnostics.set(uri, items);
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
  let current;

  for (const raw of output.split(/\r?\n/)) {
    const match = raw.match(DIAGNOSTIC_LINE);
    if (match) {
      const { path: reported, line, col, id, message } = match.groups;
      const lineIndex = Math.max(0, Number(line || 1) - 1);
      const columnIndex = Math.max(0, Number(col || 1) - 1);
      const diagnostic = new vscode.Diagnostic(
        new vscode.Range(lineIndex, columnIndex, lineIndex, columnIndex + 1),
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
        const lineIndex = Math.max(0, Number(line) - 1);
        const columnIndex = Math.max(0, Number(col || 1) - 1);
        const location = new vscode.Location(
          vscode.Uri.file(resolveReportedPath(reported, projectRoot)),
          new vscode.Range(lineIndex, columnIndex, lineIndex, columnIndex + 1)
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
  const compiler = vscode.workspace.getConfiguration("abstract").get("compilerPath", "abstract");
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
    return;
  }

  const format = vscode.workspace.getConfiguration("abstract").get("outputFormat", "JSON");
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
  const configured = vscode.workspace.getConfiguration("abstract").get("projectPath", "");
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
  return value
    .replace(/\$\{workspaceFolder\}/g, workspaceRoot(document))
    .replace(/\$\{workspaceFolder:[^}]+\}/g, workspaceRoot(document));
}

function deactivate() {}

module.exports = { activate, deactivate };
