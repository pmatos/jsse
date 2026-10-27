// RUN: 0
// Ping-pong of 100k messages, many sends in flight at once, and large
// messages in both directions at the same time (partial reads and writes).
const cp = require('child_process');
const path = require('path');
const { show, forkOpts } = require('./lib');

const child = path.join(__dirname, 'fork-child.js');
const COUNT = Number(process.env.FORK_COUNT || 100000);
// 'advanced' frames are engine-private, so the cross-engine passes set this.
const ADVANCED = process.env.FORK_JSON_ONLY === undefined;

function pingPong(serialization) {
  const c = cp.fork(child, ['bulk'], forkOpts({ serialization }));
  let next = 0;
  let wrong = 0;
  return new Promise((resolve) => {
    c.on('message', (m) => {
      if (m.n !== next || m.echo !== 'x'.repeat(next % 50)) wrong++;
      next++;
      if (next === COUNT) c.send('done');
      else c.send({ n: next });
    });
    c.on('close', (code, signal) => resolve({ received: next, wrong, code, signal }));
    c.send({ n: 0 });
  });
}

function burst(serialization, count) {
  const c = cp.fork(child, ['bulk'], forkOpts({ serialization }));
  const got = [];
  return new Promise((resolve) => {
    c.on('message', (m) => { got.push(m.n); if (got.length === count) c.send('done'); });
    c.on('close', () => resolve({ ordered: got.every((n, i) => n === i), received: got.length }));
    for (let i = 0; i < count; i++) c.send({ n: i });
  });
}

// Both ends write a large message before reading anything.
function simultaneous(serialization, size) {
  const c = cp.fork(path.join(__dirname, 'fork-big-child.js'), [String(size)], forkOpts({ serialization }));
  const got = [];
  return new Promise((resolve) => {
    c.on('message', (m) => {
      got.push(typeof m === 'string' ? { length: m.length, head: m.slice(0, 10), tail: m.slice(-4) } : m);
      if (got.length === 2) c.send('done');
    });
    c.on('close', (code) => resolve({ code, got }));
    c.send('abcdefghij'.repeat(size / 10) + 'TAIL');
  });
}

async function main() {
  show('pingpong json', await pingPong('json'));
  if (ADVANCED) show('pingpong advanced', await pingPong('advanced'));
  show('burst json', await burst('json', 20000));
  if (ADVANCED) show('burst advanced', await burst('advanced', 20000));
  show('large json', await simultaneous('json', 16 * 1024 * 1024));
  if (ADVANCED) show('large advanced', await simultaneous('advanced', 16 * 1024 * 1024));
}

main();
