// MODES: pipe pty
// INPUT: first\nsecond\n
'use strict';
process.stdin.once('data', (chunk) => {
  console.log('got', chunk.length > 0);
  process.stdin.pause();
  console.log('paused', process.stdin.isPaused());
});
process.stdin.on('end', () => console.log('end'));
process.on('exit', (code) => console.log('exit', code));
