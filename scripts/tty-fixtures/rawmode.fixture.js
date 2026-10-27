// MODES: pty
// INPUT: ab\x1b[A\x03
'use strict';
const stdin = process.stdin;
stdin.setRawMode(true);
stdin.resume();
const seen = [];
stdin.on('data', (chunk) => {
  seen.push(chunk.toString('hex'));
  if (chunk.includes(3)) {
    stdin.setRawMode(false);
    stdin.pause();
    console.log(JSON.stringify(seen), stdin.isRaw);
  }
});
