// MODES: pty
// INPUT: bob\r
// READY: raw
'use strict';
const rlp = require('node:readline/promises');
(async () => {
  const rl = rlp.createInterface({ input: process.stdin, output: process.stdout });
  const answer = await rl.question('name? ');
  console.log(`hello ${answer}`);
  rl.close();
})();
