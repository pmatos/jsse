// MODES: pipe
// INPUT: abc\ndef\n
'use strict';
let total = '';
process.stdin.on('readable', () => {
  let chunk;
  while ((chunk = process.stdin.read()) !== null) total += chunk.toString('latin1');
});
process.stdin.on('end', () => console.log('end', JSON.stringify(total)));
