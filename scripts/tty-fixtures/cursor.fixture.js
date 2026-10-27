// MODES: pipe pty
// INPUT:
// SIZE: 24x80
'use strict';
const readline = require('readline');
const rlp = require('readline/promises');
const out = process.stdout;
const log = [];
out.write('start\n');
log.push(readline.cursorTo(out, 5));
log.push(readline.cursorTo(out, 5, 2));
log.push(readline.moveCursor(out, -2, 3));
log.push(readline.moveCursor(out, 0, 0));
log.push(readline.clearLine(out, 0));
log.push(readline.clearLine(out, -1));
log.push(readline.clearLine(out, 1));
log.push(readline.clearScreenDown(out));
log.push(readline.cursorTo(out, 1, undefined, () => out.write('[cb]')));
try { readline.cursorTo(out, 'a'); } catch (e) { log.push(e.code); }
try { readline.cursorTo(out, undefined, 3); } catch (e) { log.push(e.code); }
const r = new rlp.Readline(out);
r.cursorTo(0, 1).moveCursor(1, 1).clearLine(0).clearScreenDown();
r.commit().then(() => {
  out.write('\n' + JSON.stringify(log) + '\n');
  if (out.isTTY) {
    out.write([typeof out.cursorTo, typeof out.moveCursor, typeof out.clearLine, typeof out.clearScreenDown].join() + '\n');
    out.cursorTo(3); out.moveCursor(1, 0); out.clearLine(1); out.clearScreenDown();
    out.write('|\n');
  }
});
