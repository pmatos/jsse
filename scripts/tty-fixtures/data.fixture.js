// MODES: pipe file
// INPUT: x\ny\r\nz\n
'use strict';
const chunks = [];
process.stdin.on('data', (c) => chunks.push(c));
process.stdin.on('end', () => {
  const all = Buffer.concat(chunks);
  console.log('end', chunks.length > 0, Buffer.isBuffer(chunks[0]), JSON.stringify(all.toString()));
});
process.stdin.on('close', () => console.log('close'));
