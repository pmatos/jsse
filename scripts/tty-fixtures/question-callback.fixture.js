// MODES: pipe pty
// INPUT: 42\r\n
'use strict';
const readline = require('readline');
const rl = readline.createInterface({ input: process.stdin, output: process.stdout, terminal: false });
rl.question('age? ', (answer) => {
  console.log(`age ${answer}`);
  rl.close();
});
rl.on('close', () => console.log('closed'));
