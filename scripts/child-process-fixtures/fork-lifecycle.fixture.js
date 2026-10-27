// RUN: 0
// Process lifetimes around an IPC channel: a grandchild whose parent is
// killed sees 'disconnect' and ends, a channel does not keep a child alive
// without listeners, and a ref'd channel keeps the parent alive until the
// child is gone.
const cp = require('child_process');
const path = require('path');
const { show, forkOpts } = require('./lib');

const child = path.join(__dirname, 'fork-child.js');

async function main() {
  // The middle process forks a leaf that shares the middle's stdout pipe; the
  // pipe only reaches EOF when the leaf has exited too.
  const middle = cp.fork(child, ['middle'], forkOpts({ silent: true }));
  let printed = '';
  middle.stdout.on('data', (d) => { printed += d; });
  const ended = new Promise((resolve) => middle.stdout.on('end', resolve));
  await new Promise((resolve) => middle.once('message', resolve));
  middle.kill('SIGKILL');
  await ended;
  show('leaf after parent death', printed);

  // unref'd channel: the parent exits without waiting for a quiet child
  const quiet = cp.fork(child, ['hold'], forkOpts());
  await new Promise((resolve) => quiet.once('message', resolve));
  quiet.channel.unref();
  quiet.unref();
  show('unref', [quiet.connected, typeof quiet.channel.ref]);
  const done = new Promise((resolve) => process.once('exit', resolve));
  process.on('exit', () => quiet.kill('SIGKILL'));
  await Promise.resolve();
  show('main done', true);
  void done;
}

main();
