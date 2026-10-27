// MODES: pty
// INPUT:
// ENV: TERM=xterm COLORTERM=truecolor
'use strict';
const out = process.stdout;
console.log(out.getColorDepth(), out.hasColors(2 ** 24), out.hasColors(16), out.getColorDepth({ TERM: 'dumb' }), out.getColorDepth({ TERM: 'xterm' }), out.hasColors({ TERM: 'dumb' }));
console.log({ k: 1 });
