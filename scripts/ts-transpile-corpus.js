"use strict";
// Driver of scripts/ts_transpile_corpus.py (issue #1287): run ts.transpileModule / ts.transpileDeclaration of a built
// TypeScript checkout over a list of files and several option sets, and print one line per case:
//   <file>\t<option set>\t<sha256 of output, source map and diagnostics>\t<output length>
// Usage: <node|jsse --node> ts-transpile-corpus.js <TypeScript checkout> <file with one test-case path per line>
const fs = require("fs");
const path = require("path");
const crypto = require("crypto");

const [, , tsDir, listFile] = process.argv;
const ts = require(path.join(tsDir, "built", "local", "typescript.js"));

const OPTION_SETS = {
  "es2022-esnext": { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext },
  "es5-commonjs-map": { target: ts.ScriptTarget.ES5, module: ts.ModuleKind.CommonJS, sourceMap: true },
  "isolated-jsx": { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext, isolatedModules: true, jsx: ts.JsxEmit.Preserve },
  "react-umd-inline": { target: ts.ScriptTarget.ES2017, module: ts.ModuleKind.UMD, jsx: ts.JsxEmit.React, inlineSourceMap: true, inlineSources: true },
  "node16-verbatim": { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.Node16, verbatimModuleSyntax: true, jsx: ts.JsxEmit.ReactJSX },
  "decorators": { target: ts.ScriptTarget.ES2015, module: ts.ModuleKind.AMD, experimentalDecorators: true, emitDecoratorMetadata: true, importHelpers: true, noEmitHelpers: false },
  "declaration": { target: ts.ScriptTarget.ES2020, declaration: true, declarationMap: true },
};
const DECLARATION_SETS = new Set(["declaration"]);

function digest(result) {
  const h = crypto.createHash("sha256");
  h.update(result.outputText || "");
  h.update("\u0000" + (result.sourceMapText || ""));
  for (const d of result.diagnostics || []) {
    h.update(`\u0000${d.code}:${d.start}:${d.length}:${ts.flattenDiagnosticMessageText(d.messageText, "\n")}`);
  }
  return h.digest("hex");
}

for (const file of fs.readFileSync(listFile, "utf8").split("\n").filter(Boolean)) {
  const text = fs.readFileSync(path.join(tsDir, file), "utf8");
  for (const [name, compilerOptions] of Object.entries(OPTION_SETS)) {
    let line;
    try {
      const run = DECLARATION_SETS.has(name) ? ts.transpileDeclaration : ts.transpileModule;
      const result = run(text, { compilerOptions: { ...compilerOptions }, fileName: path.basename(file), reportDiagnostics: true });
      line = `${digest(result)}\t${(result.outputText || "").length}`;
    } catch (e) {
      line = `throws ${e && e.name}: ${String(e && e.message).split("\n")[0]}\t0`;
    }
    console.log(`${file}\t${name}\t${line}`);
  }
}
