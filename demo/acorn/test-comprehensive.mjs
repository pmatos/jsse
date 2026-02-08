import { parse } from "./node_modules/acorn/dist/acorn.mjs";

var tests = [
  ["1 + 2", "simple expression"],
  ["function hello(name) { return 'Hello, ' + name; }", "function declaration"],
  ["const greet = ({ name, age }) => `${name} is ${age}`;", "arrow + destructuring + template"],
  ["class Foo extends Bar { constructor() { super(); this.x = 1; } get y() { return 2; } }", "class declaration"],
  ["async function fetchData() { const res = await fetch('/api'); return res.json(); }", "async/await"],
  ["for (let [key, value] of map) { console.log(key, value); }", "for-of with destructuring"],
  ["const { a, b: c, ...rest } = obj;", "object destructuring with rest"],
  ["const [x, , z, ...tail] = arr;", "array destructuring with holes and rest"],
  ["a?.b?.c ?? 'default'", "optional chaining + nullish coalescing"],
  ["let x = condition ? a : b;", "ternary expression"],
  ["switch(x) { case 1: break; default: break; }", "switch statement"],
  ["try { throw new Error('test'); } catch(e) { } finally { }", "try/catch/finally"],
  ["/(?:abc|def)/gi.test(str)", "regex literal"],
  ["var obj = { get x() { return 1; }, set x(v) { }, ['computed']: true, method() {} };", "object with getters/setters/computed/methods"],
  ["import.meta.url", "import.meta"],
  ["(function*() { yield 1; yield* [2, 3]; })", "generator function"],
  ["0b1010 + 0o17 + 0xDEAD", "binary/octal/hex literals"],
  ["tag`hello ${world}`", "tagged template literal"],
  ["({ ...a, b: 1 })", "object spread"],
];

var passed = 0;
var failed = 0;

for (var i = 0; i < tests.length; i++) {
  var code = tests[i][0];
  var desc = tests[i][1];
  try {
    var ast = parse(code, { ecmaVersion: 2020, sourceType: "module" });
    if (ast && ast.type === "Program") {
      passed++;
      console.log("PASS:", desc);
    } else {
      failed++;
      console.log("FAIL:", desc, "- no AST returned");
    }
  } catch(e) {
    failed++;
    console.log("FAIL:", desc, "-", e.message);
  }
}

console.log("\n" + passed + "/" + (passed + failed) + " tests passed");
