'use strict';
// Sends one large string to the parent before reading anything, then answers
// the parent's large string with its length and ends. argv[2] is the size.
const size = Number(process.argv[2]);
process.send('0123456789'.repeat(size / 10) + 'END!');
process.on('message', (m) => {
  if (m === 'done') process.disconnect();
  else process.send({ length: m.length, head: m.slice(0, 4), tail: m.slice(-4) });
});
