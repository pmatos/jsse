import * as prettier from './node_modules/prettier/standalone.mjs';
import * as babel from './node_modules/prettier/plugins/babel.mjs';
import * as estree from './node_modules/prettier/plugins/estree.mjs';

let passed = 0;
let failed = 0;

async function test(name, input, expected, options = {}) {
  try {
    const result = await prettier.format(input, {
      parser: 'babel',
      plugins: [babel, estree],
      ...options,
    });
    if (result === expected) {
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

await test('simple variable', 'const x=1;', 'const x = 1;\n');
await test('function declaration', 'function foo(a,b){return a+b}', 'function foo(a, b) {\n  return a + b;\n}\n');
await test('arrow function', 'const f=(x)=>x*2', 'const f = (x) => x * 2;\n');

console.log('\n' + passed + '/' + (passed + failed) + ' tests passed');
if (failed > 0) process.exit(1);
