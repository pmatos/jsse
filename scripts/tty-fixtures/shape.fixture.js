// MODES: pipe file pty
// INPUT: hello\n
// SIZE: 30x100
'use strict';
const stream = require('stream');
const tty = require('tty');

function describe(name, s) {
  const named = s.constructor.name;
  return [
    name, named, s._type, s.fd, s.isTTY, s.readable, s.writable, s.columns,
    s.rows, typeof s.getColorDepth, typeof s.hasColors, typeof s.setRawMode,
    s instanceof stream.Writable, s instanceof stream.Readable,
    s instanceof stream.Duplex,
    Object.getOwnPropertyDescriptor(process, name).enumerable,
    Object.getOwnPropertyDescriptor(process, name).configurable,
  ].join(' ');
}

const lines = [];
for (const name of ['stdin', 'stdout', 'stderr']) lines.push(describe(name, process[name]));
lines.push([0, 1, 2, 3, -1, 1.5, '1'].map((fd) => tty.isatty(fd)).join(' '));
lines.push(process.stdout === process.stdout, process.stdin === process.stdin);
lines.push(Object.keys(tty).sort().join());
process.stdout.write(lines.join('\n') + '\n');
