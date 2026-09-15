const { scanLine } = require("./language-model");

const SCHEMA = "[A-Za-z][A-Za-z0-9_]*";
const ID = "[A-Za-z0-9_][A-Za-z0-9_-]*";
const PATH = `${ID}(?:\\.${ID})*`;
const BLOCK_OPEN = new RegExp(
  `^(?:(?:schema|logic)\\s+${SCHEMA}|${PATH}(?:\\[[^\\]]*\\])?(?:\\s+@(?:optional|public|tag|since\\(\\d+\\)|removed\\(\\d+\\)))*|(?:if|for)\\b[\\s\\S]*|(?:}\\s*)?else\\b[\\s\\S]*)\\s*\\{\\s*$`
);
const HEADER = new RegExp(`^${SCHEMA}\\s*::`);

// Formatting changes indentation only. Abstract declares indentation
// insignificant, so strings, comments, values and token spelling remain exact.
function formatDocument(text, options = {}) {
  const eol = text.includes("\r\n") ? "\r\n" : "\n";
  const finalEol = /\r?\n$/.test(text);
  const lines = text.split(/\r?\n/);
  if (finalEol) lines.pop();
  const unit = options.insertSpaces === false ? "\t" : " ".repeat(Math.max(1, options.tabSize || 4));
  const output = [];
  let explicitDepth = 0;
  let continuationDepth = 0;
  let headerContinuation = false;
  let instance = false;

  for (const raw of lines) {
    const content = raw.trimStart();
    if (!content) { output.push(""); continue; }
    const masked = scanLine(content).masked;
    const closesBlock = continuationDepth === 0 && /^}/.test(masked) ? 1 : 0;
    const logicalDepth = Math.max(0, explicitDepth - closesBlock);
    const top = logicalDepth === 0 && continuationDepth === 0 && !headerContinuation;
    const beginsHeader = top && HEADER.test(masked);
    const beginsDeclaration = top && /^(?:schema|logic|versions)\b/.test(masked);

    const closesContinuation = continuationDepth > 0 && /^[\])}]/.test(masked);
    const depth = logicalDepth + (instance && !beginsHeader && !beginsDeclaration ? 1 : 0)
      + (continuationDepth > 0 && !closesContinuation ? 1 : 0);
    output.push(unit.repeat(depth) + content);

    const opensBlock = BLOCK_OPEN.test(masked) ? 1 : 0;
    explicitDepth = logicalDepth + opensBlock;
    if (!explicitDepth && !instance && /^}/.test(masked)) explicitDepth = 0;

    const blockOpenIndex = opensBlock ? masked.lastIndexOf("{") : -1;
    const blockCloseIndex = closesBlock ? masked.indexOf("}") : -1;
    for (let index = 0; index < masked.length; index += 1) {
      const char = masked[index];
      if (char === "(" || char === "[") continuationDepth += 1;
      else if (char === ")" || char === "]") continuationDepth = Math.max(0, continuationDepth - 1);
      else if (char === "{" && index !== blockOpenIndex) continuationDepth += 1;
      else if (char === "}" && index !== blockCloseIndex) continuationDepth = Math.max(0, continuationDepth - 1);
    }
    if (beginsHeader) instance = true;
    else if (beginsDeclaration) instance = false;
    const headerStart = HEADER.test(masked);
    headerContinuation = continuationDepth === 0 && (headerStart || headerContinuation) && /,\s*$/.test(masked);
  }
  return output.join(eol) + (finalEol ? eol : "");
}

module.exports = { formatDocument };
