// MODES: pipe file
// INPUT:
'use strict';
const line = 'x'.repeat(1023) + '\n';
let sent = 0;
for (let i = 0; i < 2048; i++) { process.stdout.write(line); sent++; }
console.error('sent', sent);
process.exitCode = 3;
process.exit();
