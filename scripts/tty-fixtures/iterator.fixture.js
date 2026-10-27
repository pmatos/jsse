// MODES: pipe file
// INPUT: one\ntwo\nthree\n
'use strict';
const readline = require('readline');
(async () => {
  const rl = readline.createInterface({ input: process.stdin, crlfDelay: Infinity });
  const seen = [];
  for await (const line of rl) {
    seen.push(line);
    console.log('line', line);
  }
  console.log('done', seen.length);
})();
