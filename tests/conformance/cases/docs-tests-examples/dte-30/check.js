const fs = require("fs");
const src = fs.readFileSync("data/templates/Item.abt", "utf8");

// 1) vscode/src/extension.js:103 - schema type regex actually shipped
const shipped = /\b(text|int|enum|file)\b(?=\s*\()/g;
// what 0.2 actually has (docs/language.md:67-79)
const complete = /\b(text|int|float|enum|file|image)\b(?=\s*\()/g;
const got = [...src.matchAll(shipped)].map(m => m[1]);
const want = [...src.matchAll(complete)].map(m => m[1]);
console.log("extension.js semantic-token schema types matched :", got.join(", "));
console.log("actual 0.2 schema types present in the file       :", want.join(", "));
console.log("MISSED by the shipped regex                      :",
  want.filter((t,i) => !got.includes(t)).filter((v,i,a)=>a.indexOf(v)===i).join(", "));

// 2) vscode/src/extension.js:107 - logic keyword regex
const logicShipped = /\b(if|require|else|throw|for|in|derive)\b/g;
console.log("\nlogic keywords matched  :", [...src.matchAll(logicShipped)].map(m=>m[1]).join(", "));
console.log("'not' matched?           :", /\bnot\b/.test(src) ? "present in source, NOT in the regex" : "n/a");
console.log("'derive?' matched as     :", (src.match(/derive\?/) ? "'derive' only - the '?' is left unhighlighted" : "n/a"));

// 3) vscode/src/extension.js:6-20 - static completion list
const COMPLETIONS = ["schema","logic","text","int","enum","file","@optional","@tag"];
console.log("\ncompletion list missing  :",
  ["float","bool","image","derive","derive?","if","else","require","throw","for","not"]
    .filter(k => !COMPLETIONS.includes(k)).join(", "));

// 4) syntaxes/abstract.tmLanguage.json - schemaBlock end pattern
const g = JSON.parse(fs.readFileSync("../../../project/vscode/syntaxes/abstract.tmLanguage.json","utf8"));
console.log("\nschemaBlock.end =", JSON.stringify(g.repository.schemaBlock.end),
  "-> the first '}' of the nested 'stats { ... }' group at line",
  src.split("\n").findIndex(l => l.trim() === "}") + 1,
  "closes the whole schema scope");
console.log("dead grammar rule       :",
  JSON.stringify(g.repository.constants.patterns[1]),
  "-> Abstract has no 'default' keyword");
