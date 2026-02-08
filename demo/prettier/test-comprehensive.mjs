import * as prettier from './node_modules/prettier/standalone.mjs';
import * as babel from './node_modules/prettier/plugins/babel.mjs';
import * as estree from './node_modules/prettier/plugins/estree.mjs';

let passed = 0;
let failed = 0;

async function test(name, input, expected, options = {}) {
  try {
    const result = await prettier.format(input + '\n', {
      parser: 'babel',
      plugins: [babel, estree],
      ...options,
    });
    if (result === expected) {
      console.log('PASS: ' + name);
      passed++;
    } else {
      console.log('FAIL: ' + name);
      console.log('  Expected: ' + JSON.stringify(expected));
      console.log('  Got:      ' + JSON.stringify(result));
      failed++;
    }
  } catch (e) {
    console.log('FAIL: ' + name + ' (threw: ' + e.message + ')');
    failed++;
  }
}

// Category 1: Variable declarations (5 tests)
await test('const declaration', 'const x = 1;', 'const x = 1;\n');
await test('let with extra spaces', 'let   y  =  2;', 'let y = 2;\n');
await test('var multiple declarators', 'var z=3,w=4;', 'var z = 3,\n  w = 4;\n');
await test('destructuring object', 'const {a, b} = obj;', 'const { a, b } = obj;\n');
await test('destructuring array with rest', 'const [x, ...rest] = arr;', 'const [x, ...rest] = arr;\n');

// Category 2: Functions (5 tests)
await test('function declaration', 'function foo(a,b){return a+b}', 'function foo(a, b) {\n  return a + b;\n}\n');
await test('arrow function expression', 'const f=(x)=>x*2', 'const f = (x) => x * 2;\n');
await test('arrow function with block body', 'const g = (a,b) => { return a+b; }', 'const g = (a, b) => {\n  return a + b;\n};\n');
await test('async function', 'async function fetchData(url){return await fetch(url)}', 'async function fetchData(url) {\n  return await fetch(url);\n}\n');
await test('generator function', 'function* gen(){yield 1;yield 2}', 'function* gen() {\n  yield 1;\n  yield 2;\n}\n');

// Category 3: Classes (2 tests)
await test('class with constructor and getter', 'class Foo{constructor(x){this.x=x}get val(){return this.x}}', 'class Foo {\n  constructor(x) {\n    this.x = x;\n  }\n  get val() {\n    return this.x;\n  }\n}\n');
await test('class extends with super', 'class Bar extends Foo{method(){super.method()}}', 'class Bar extends Foo {\n  method() {\n    super.method();\n  }\n}\n');

// Category 4: Control flow (5 tests)
await test('if-else', 'if(x){a()}else{b()}', 'if (x) {\n  a();\n} else {\n  b();\n}\n');
await test('for loop', 'for(let i=0;i<10;i++){console.log(i)}', 'for (let i = 0; i < 10; i++) {\n  console.log(i);\n}\n');
await test('while loop', 'while(true){break}', 'while (true) {\n  break;\n}\n');
await test('try-catch', 'try{a()}catch(e){b()}', 'try {\n  a();\n} catch (e) {\n  b();\n}\n');
await test('try-finally', 'try{a()}finally{c()}', 'try {\n  a();\n} finally {\n  c();\n}\n');

// Category 5: Expressions (12 tests)
await test('addition', '1+2;', '1 + 2;\n');
await test('string literal', '"hello";', '"hello";\n');
await test('object literal', 'const x = {a:1,b:2,c:3};', 'const x = { a: 1, b: 2, c: 3 };\n');
await test('array literal', 'const arr = [1,2,3,4,5];', 'const arr = [1, 2, 3, 4, 5];\n');
await test('ternary', 'const result = condition ? a : b;', 'const result = condition ? a : b;\n');
await test('nullish coalescing', 'const x = a ?? b;', 'const x = a ?? b;\n');
await test('optional chaining', 'const y = a?.b?.c;', 'const y = a?.b?.c;\n');
await test('spread in object', 'const z = {...obj, a: 1};', 'const z = { ...obj, a: 1 };\n');
await test('spread in array', 'const w = [...arr, 4, 5];', 'const w = [...arr, 4, 5];\n');
await test('template literal (simple)', 'const x = `hello world`;', 'const x = `hello world`;\n');
await test('logical operator precedence', 'const x = a || b && c;', 'const x = a || (b && c);\n');
await test('function with array arg', 'const x = foo([a,b,c]);', 'const x = foo([a, b, c]);\n');

// Category 6: Async (1 test)
await test('async function with await', 'async function foo(){const x = await bar();return x}', 'async function foo() {\n  const x = await bar();\n  return x;\n}\n');

// Category 7: Modules (3 tests)
await test('named import', 'import {a,b} from "module";', 'import { a, b } from "module";\n');
await test('export const', 'export const x = 1;', 'export const x = 1;\n');
await test('export default function', 'export default function() {}', 'export default function () {}\n');

// Category 8: Formatting options (6 tests)
await test('no semicolons', 'const x = 1;', 'const x = 1\n', { semi: false });
await test('single quotes', 'const x = "hello";', "const x = 'hello';\n", { singleQuote: true });
await test('tabWidth 4', 'if (true) {\n  x;\n}', 'if (true) {\n    x;\n}\n', { tabWidth: 4 });
await test('trailing comma all', 'const x = [1, 2, 3];', 'const x = [1, 2, 3];\n', { trailingComma: 'all' });
await test('no bracket spacing', 'const x = { a: 1, b: 2 };', 'const x = {a: 1, b: 2};\n', { bracketSpacing: false });
await test('arrow parens avoid', 'const f = (x) => x;', 'const f = x => x;\n', { arrowParens: 'avoid' });

// Category 9: Edge cases (5 tests)
await test('empty input', '', '', { });
await test('whitespace only', '   \n\n   ', '', { });
await test('typeof operator', 'typeof x === "string";', 'typeof x === "string";\n');
await test('void operator', 'void 0;', 'void 0;\n');
await test('long line wrap', 'const veryLongVariableName = someVeryLongFunctionName(argument1, argument2, argument3);', 'const veryLongVariableName =\n  someVeryLongFunctionName(\n    argument1,\n    argument2,\n    argument3,\n  );\n', { printWidth: 40 });

console.log('\n' + passed + '/' + (passed + failed) + ' tests passed');
