// V8's wording of early errors (issues #1484, #1523): the SyntaxError message of
// each source through import() of a module that does not parse, and of eval and new Function,
// which must not repeat the SyntaxError prefix.
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { pathToFileURL } from "node:url";

const modules = [
 "export { x };",
 "let a; export { a }; export { a };",
 "export var a; export var a;",
 "import { a, a } from './lib.mjs';",
 "import a from './lib.mjs'; let a;",
 "import a from './lib.mjs'; var a;",
 "import a from './lib.mjs'; function a() {}",
 "var await;",
 "await = 1;",
 "function f() { var await; }",
 "with (a) {}",
 "delete x;",
 "010",
 "var eval;",
 "export { a as 'b' }; var a; export { a as 'b' }",
 "import { 'a' } from './lib.mjs';",
 "import { 'a' as } from './lib.mjs';",
 "export {",
 "export { a",
 "export { a as }",
 "export { a b }",
 "export default",
 "export * from",
 "export * as from './lib.mjs'",
 "export var",
 "export let",
 "export const a;",
 "export function",
 "export class",
 "import",
 "import x",
 "import x from",
 "import x from 1",
 "import {",
 "import { a",
 "import { a as }",
 "import { a b } from 'x'",
 "import * from 'x'",
 "import * as",
 "import * as x",
 "import a, * as b, c from 'x'",
 "import {a,,} from 'x'",
 "if (1) import x from './lib.mjs';",
 "{ export var a; }",
 "function f() { export var a; }",
 "function f() { import x from './lib.mjs'; }",
 "import.meta = 1",
 "import.meta++",
 "new.target",
 "return 1",
 "yield 1",
 "let yield;",
 "var arguments = 1; (function() { 'use strict'; var arguments; })",
 "export { a as default }; export default 1; var a;",
 "import 'x' with { type: 'json', type: 'json' }",
 "import 'x' with { type: 1 }",
 "import 'x' with { 'a': 'b', a: 'c' }",
 "import 'x' with { a }",
 "import 'x' with",
 "export * as ns from",
 "class A {} export { A }; export { A }",
 "export { default } from './lib.mjs'; export { default } from './lib.mjs'",
 "label: { import x from 'y' }",
 "await 1; function f() { await 1 }",
 "for await (x of y) ;; async function f() { for await (x;;) ; }",
 "async function f() { await = 1 }",
 "class await {}",
 "function await() {}",
 "var x = await;",
 "(await) => 1",
 "async function f(a = await 1) {}",
 "arguments = 1"
];

const dir = fs.mkdtempSync(path.join(os.tmpdir(), "jsse-syntax-messages-"));
fs.writeFileSync(path.join(dir, "lib.mjs"), "export const a = 1; export default 2;\n");
let n = 0;
for (const source of modules) {
  const file = path.join(dir, `m${n++}.mjs`);
  fs.writeFileSync(file, source + "\n");
  try {
    await import(pathToFileURL(file).href);
    console.log("module", JSON.stringify(source), "loads");
  } catch (e) {
    console.log("module", JSON.stringify(source), e.name + ": " + e.message.split(dir).join(""));
  }
}
fs.rmSync(dir, { recursive: true });

for (const source of ["x = ;", "function () {}", "const = 1", "let let = 1", "'use strict'; with (a) {}"]) {
  for (const [label, run] of [
    ["eval", () => eval(source)],
    ["indirect eval", () => (0, eval)(source)],
    ["new Function", () => new Function(source)],
  ]) {
    try {
      run();
    } catch (e) {
      console.log(label, JSON.stringify(source), e.name + ": " + e.message);
    }
  }
}
