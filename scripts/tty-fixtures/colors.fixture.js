// MODES: pipe pty
// INPUT:
'use strict';
const util = require('util');
console.log({ a: 1, s: 'str', n: null, u: undefined, arr: [1, 2, 3], f() {}, d: new Date(0), sym: Symbol('x') });
console.log('%o', { x: 1 });
console.log('plain string', 5, true);
console.error({ err: [1, { deep: { deeper: { deepest: 1 } } }] });
console.dir({ dir: 1, nested: { a: [1] } }, { depth: 0 });
console.log(util.inspect({ explicit: 1 }));
console.log(util.inspect({ explicit: 1 }, { colors: true }));
const out = process.stdout;
console.log(out.isTTY, out.getColorDepth && out.getColorDepth(), out.hasColors && out.hasColors(256));
