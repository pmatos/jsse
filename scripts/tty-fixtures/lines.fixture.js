// MODES: pipe file
// INPUT: a\nb b\n\nlast
'use strict';
const readline = require('readline');
const rl = readline.createInterface({ input: process.stdin });
let n = 0;
rl.on('line', (line) => console.log('line', ++n, JSON.stringify(line)));
rl.on('close', () => console.log('close', n));
