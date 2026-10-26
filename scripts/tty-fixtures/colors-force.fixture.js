// MODES: pipe file pty
// INPUT:
// ENV: FORCE_COLOR=1
'use strict';
console.log({ a: 1, s: 'str' });
console.error([true, null]);
console.log(process.stdout.isTTY, process.stdout.getColorDepth && process.stdout.getColorDepth());
