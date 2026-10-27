// MODES: pty
// INPUT:
// ENV: NO_COLOR=1
'use strict';
console.log({ a: 1, s: 'str' });
console.log(process.stdout.getColorDepth(), process.stdout.hasColors());
