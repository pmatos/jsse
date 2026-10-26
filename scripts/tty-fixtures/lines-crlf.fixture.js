// MODES: pipe pty
// INPUT: x\r\ny\rz\n\x04
'use strict';
const readline = require('readline');
const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
rl.on('line', (line) => console.log('line', JSON.stringify(line)));
rl.on('close', () => console.log('close'));
