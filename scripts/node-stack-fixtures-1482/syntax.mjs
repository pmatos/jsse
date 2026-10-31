// V8's wording for a token the grammar has no place for (issue #1484), as
// reported by `vm.Script` and by a module that does not parse.
import vm from "node:vm";

const sources = [
  "const = 1;",
  "let x = 1 y;",
  "let x = 1 2;",
  "let x = 1 'a';",
  "x = ;",
  "class {",
  "if (x) {",
  "foo(",
  "var 1;",
  "let [ = 1;",
  "var {a: } = 1;",
  "a b",
  "1 +",
  "for (;;",
  "x = {a: 1 b: 2}",
  "x = [1 2]",
  "({a b})",
  "var x = `a` `b` 1",
  "if",
  "}",
  ")",
  "x ? y",
  "var a, ;",
  "class A { x y }",
  "var x = 1; var x = ;",
  "x = (1, 2",
  "x = 1 +* 2",
  "async function f() { await }",
  "a => {",
  "switch (x) { foo }",
  "do x while (y)",
];

for (const source of sources) {
  try {
    new vm.Script(source);
    console.log(JSON.stringify(source), "parses");
  } catch (e) {
    console.log(JSON.stringify(source), e.name + ": " + e.message);
  }
}

try {
  await import("./syntax-error.mjs");
} catch (e) {
  console.log("module", e.name + ": " + e.message);
}
