// MODES: pipe
// INPUT:
'use strict';
const tty = require('tty');
const rs = new tty.ReadStream(0);
rs.on('error', (err) => console.log(err.message, err.code, err.errno, err.syscall));
rs.setRawMode(true);
