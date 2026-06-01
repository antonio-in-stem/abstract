const vscode = require("vscode");
const cp = require("child_process");
const path = require("path");
const fs = require("fs");

const COMPLETIONS = [
  "schema",
  "logic",
  "text",
  "int",
  "enum",
  "file",
  "@optional",
  "@tag",
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
      addRegexTokens(tokens, line, /\b(text|int|enum|file)\b(?=\s*\()/g, "abstractSchemaType");
      addRegexTokens(tokens, line, /@(optional|tag)\b/g, "abstractSchemaModifier");
      addRegexTokens(tokens, line, /^\s*([A-Za-z_][A-Za-z0-9_]*)(?:\[\])?(?=\s*[:{])/g, "abstractSchemaField", 1);
    } else if (blockKind === "logic") {
      addRegexTokens(tokens, line, /\b(if|require|else|throw|for|in|derive)\b/g, "abstractLogicKeyword");
      addRegexTokens(tokens, line, /\b(contains|exists|length|and|or)\b|&&|\|\||==|!=|>=|<=|>|</g, "abstractLogicOperator");
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

function lintDocument(document, diagnostics) {
  const compiler = vscode.workspace.getConfiguration("abstract").get("compilerPath", "abstract");
  const target = resolveProjectPath(document);
  const command = `"${compiler}" lint "${target}"`;

  cp.exec(command, { cwd: workspaceRoot(document), windowsHide: true }, (error, stdout, stderr) => {
    const output = `${stderr || ""}\n${stdout || ""}`.trim();
    if (!error) {
      diagnostics.clear();
      return;
    }

    const message = output || String(error);
    const parsed = parseCompilerError(message, target);
    const uri = parsed.file ? vscode.Uri.file(parsed.file) : document.uri;
    const targetDocument = findOpenDocument(uri) || document;
    const range = rangeForDiagnostic(targetDocument, parsed.detail || message);
    const diagnostic = new vscode.Diagnostic(
      range,
      parsed.detail || message,
      vscode.DiagnosticSeverity.Error
    );
    diagnostic.source = "abstract";
    if (parsed.file) {
      diagnostic.relatedInformation = [
        new vscode.DiagnosticRelatedInformation(
          new vscode.Location(uri, range),
          `Reported by: ${path.basename(compiler)} lint`
        )
      ];
    }
    diagnostics.clear();
    diagnostics.set(uri, [diagnostic]);
  });
}

function parseCompilerError(message, target) {
  const clean = message.replace(/^abstract:\s*/i, "").trim();
  const match = clean.match(/^([^:\r\n]+\.ab(?:t|raw)?):\s*(.+)$/);
  if (!match) return { file: "", detail: clean };

  const reportedPath = match[1];
  const detail = match[2];
  const base = fs.existsSync(target) && fs.statSync(target).isDirectory()
    ? target
    : path.dirname(target);
  const file = path.isAbsolute(reportedPath)
    ? reportedPath
    : path.resolve(base, reportedPath);
  return { file, detail };
}

function findOpenDocument(uri) {
  return vscode.workspace.textDocuments.find((document) => document.uri.fsPath === uri.fsPath);
}

function rangeForDiagnostic(document, message) {
  const field = fieldFromMessage(message);
  const needle = valueFromMessage(message);
  const fallback = new vscode.Range(0, 0, 0, Math.max(1, document.lineAt(0).text.length));

  for (let index = 0; index < document.lineCount; index += 1) {
    const line = document.lineAt(index).text;
    if (field && lineMatchesField(line, field)) {
      return tokenRange(document, index, field) || document.lineAt(index).range;
    }
    if (needle && line.includes(needle)) {
      return tokenRange(document, index, needle) || document.lineAt(index).range;
    }
  }
  return fallback;
}

function fieldFromMessage(message) {
  const match = message.match(/\b[A-Z][A-Za-z0-9_]*\.([A-Za-z0-9_]+)\b/);
  return match ? match[1] : "";
}

function valueFromMessage(message) {
  const match = message.match(/received '([^']+)'|unknown template '([^']+)'|clone cycle detected for '([^']+)'/);
  return match ? (match[1] || match[2] || match[3] || "") : "";
}

function lineMatchesField(line, field) {
  return line.includes(`${field}:`)
    || line.includes(`@${field}.`)
    || line.includes(`.${field}:`)
    || line.includes(`${field}[]`);
}

function tokenRange(document, lineIndex, token) {
  const line = document.lineAt(lineIndex).text;
  const start = line.indexOf(token);
  if (start < 0) return undefined;
  return new vscode.Range(lineIndex, start, lineIndex, start + token.length);
}

function runCliCommand(document, commandName) {
  const compiler = vscode.workspace.getConfiguration("abstract").get("compilerPath", "abstract");
  const target = resolveProjectPath(document);
  const format = vscode.workspace.getConfiguration("abstract").get("outputFormat", "JSON");
  const formatArg = commandName === "compile" ? ` ${format}` : "";
  const command = `"${compiler}" ${commandName} "${target}"${formatArg}`;

  return new Promise((resolve) => {
    cp.exec(command, { cwd: workspaceRoot(document), windowsHide: true }, (error, stdout, stderr) => {
      const channel = getOutputChannel();
      channel.clear();
      channel.appendLine(`> ${command}`);
      if (stdout) channel.appendLine(stdout.trimEnd());
      if (stderr) channel.appendLine(stderr.trimEnd());
      if (error) {
        vscode.window.showErrorMessage(`Abstract ${commandName} failed. See Abstract output.`);
      } else {
        vscode.window.showInformationMessage(`Abstract ${commandName} completed.`);
      }
      channel.show(true);
      resolve();
    });
  });
}

let outputChannel;

function getOutputChannel() {
  if (!outputChannel) {
    outputChannel = vscode.window.createOutputChannel("Abstract");
  }
  return outputChannel;
}

function resolveProjectPath(document) {
  const configured = vscode.workspace.getConfiguration("abstract").get("projectPath", "");
  if (configured) {
    return expandWorkspacePath(document, configured);
  }

  const root = workspaceRoot(document);
  const abstractFolder = path.join(root, ".abstract");
  if (fs.existsSync(abstractFolder)) return abstractFolder;
  const abstractExampleFolder = path.join(root, "example");
  if (fs.existsSync(path.join(root, "Cargo.toml")) && fs.existsSync(abstractExampleFolder)) {
    return abstractExampleFolder;
  }
  return document.uri.fsPath;
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
