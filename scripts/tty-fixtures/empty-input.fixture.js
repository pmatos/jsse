// MODES: pipe file
// INPUT:
'use strict';
process.stdin.on('data', () => console.log('data?'));
process.stdin.on('end', () => console.log('end'));
process.stdin.resume();
