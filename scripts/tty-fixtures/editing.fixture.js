// MODES: pty
// INPUT: ab\x7fc\rxyz\x01\x0bq\r\x04
// READY: raw
// SIZE: 24x80
'use strict';
const readline = require('readline');
const rl = readline.createInterface({ input: process.stdin, output: process.stdout, prompt: '> ' });
rl.prompt();
rl.on('line', (line) => {
  console.log('got', JSON.stringify(line), rl.line.length, rl.cursor);
  rl.prompt();
});
rl.on('close', () => console.log('bye'));
