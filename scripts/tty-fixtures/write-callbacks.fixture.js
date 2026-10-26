// MODES: pipe file pty
// INPUT:
'use strict';
const log = [];
const mark = (s) => log.push(s);
const ok = process.stdout.write('a\n', (err) => mark('write cb ' + String(err)));
mark('write returned ' + ok);
process.nextTick(() => mark('tick'));
Promise.resolve().then(() => mark('microtask'));
setImmediate(() => mark('immediate'));
process.stdout.write('b\n', 'utf8', () => mark('write cb2'));
process.on('exit', () => {
  process.stdout.write(log.join('\n') + '\n');
});
