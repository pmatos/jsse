// MODES: pty
// INPUT: a\x1b[A\x1b[B\x7f\r\x03
// READY: raw
'use strict';
const readline = require('readline');
readline.emitKeypressEvents(process.stdin);
process.stdin.setRawMode(true);
const names = [];
process.stdin.on('keypress', (str, key) => {
  names.push(key.ctrl ? 'ctrl+' + key.name : key.name + (str ? '(' + JSON.stringify(str) + ')' : ''));
  if (key.ctrl && key.name === 'c') {
    process.stdin.setRawMode(false);
    process.stdin.pause();
    console.log(names.join(' '));
  }
});
