// MODES: pty
// INPUT:
// SIZE: 40x132
'use strict';
const tty = require('tty');
const ws = new tty.WriteStream(1);
const rs = new tty.ReadStream(0);
console.log(ws.isTTY, ws.columns, ws.rows, ws.getWindowSize(), rs.isTTY, rs.isRaw, rs.rawMode);
console.log(typeof tty.WriteStream.prototype.cursorTo, rs instanceof require('stream').Readable);
for (const bad of [-1, 1.5, '1']) {
  try { new tty.WriteStream(bad); } catch (e) { console.log(e.name, e.code, e.message); }
}
try { rs.setRawMode('bogus'); } catch (e) { console.log(e.name, e.code, e.message); }
console.log(rs.setRawMode(true) === rs, rs.isRaw, rs.rawMode);
console.log(rs.setRawMode(false) === rs, rs.isRaw, rs.rawMode);
console.log(rs.setRawMode('io').rawMode, rs.setRawMode(false).isRaw);
