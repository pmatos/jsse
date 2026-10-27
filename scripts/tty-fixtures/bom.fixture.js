// MODES: pipe file
// INPUT: \xef\xbb\xbfhello\nworld\n
'use strict';
const readline = require('readline');
const rl = readline.createInterface({ input: process.stdin });
rl.on('line', (line) => console.log(JSON.stringify(line), line.length));
