const vscode = require("vscode");
const { findSyntaxHelp } = require("./syntax-help");

function registerSyntaxHover(context) {
  context.subscriptions.push(vscode.languages.registerHoverProvider({ language: "abstract" }, {
    provideHover(document, position) {
      const match = findSyntaxHelp(document.getText(), document.offsetAt(position));
      if (!match) return undefined;
      return new vscode.Hover(
        new vscode.MarkdownString(match.markdown),
        new vscode.Range(document.positionAt(match.start), document.positionAt(match.end))
      );
    }
  }));
}

module.exports = { registerSyntaxHover };
