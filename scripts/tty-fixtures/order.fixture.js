// MODES: pipe file pty
// INPUT:
'use strict';
console.log('1 console.log');
process.stdout.write('2 stdout.write\n');
console.error('3 console.error');
process.stderr.write('4 stderr.write\n');
console.log('5 %s %d', 'fmt', 7);
process.stdout.write('6 no newline');
process.stdout.write(' ... continued\n');
process.stdout.write(Buffer.from('7 buffer\n'));
process.stdout.write(new Uint8Array([56, 32, 117, 56, 10]));
process.stdout.write('9 café ☃\n');
process.stdout.write('10 latin1 \xe9\n', 'latin1');
process.stdout.write('3132206865780a', 'hex');
process.stdout.write('MTMgYmFzZTY0Cg==', 'base64');
console.group('g');
console.log('14 grouped');
console.groupEnd();
console.log('15 last');
