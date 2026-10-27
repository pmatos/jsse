// MODES: pipe pty
// INPUT: data\n
'use strict';
process.stdin.resume();
process.stdin.once('data', () => {
  console.log('exiting while stdin is open');
  process.exit(5);
});
