// RUN: 0
const cp = require('child_process');
const path = require('path');
const { err, show, finish, forkOpts } = require('./lib');

const child = path.join(__dirname, 'fork-child.js');

// Collect a forked child's messages and lifecycle events up to 'close'.
function run(c, onMessage, until = 'close') {
  const messages = [];
  const events = [];
  c.on('message', (m) => { messages.push(m); if (onMessage) onMessage(m); });
  c.on('error', (e) => events.push('error:' + e.code));
  c.on('disconnect', () => events.push('disconnect'));
  c.on('exit', (code, signal) => events.push(`exit:${code}:${signal}`));
  c.on('close', (code, signal) => events.push(`close:${code}:${signal}`));
  // Node never emits 'close' for a child the parent disconnected itself.
  return new Promise((resolve) => c.on(until, () => setImmediate(() => {
    events.sort();
    resolve({ messages, events });
  })));
}

async function main() {
  let c = cp.fork(child, ['info', 'a', 'b'], forkOpts({ silent: true }));
  show('shape', {
    connected: c.connected, send: typeof c.send, disconnect: typeof c.disconnect,
    channel: c.channel && c.channel.constructor.name, pid: typeof c.pid,
    stdio: c.stdio.map((s) => s === null ? null : typeof s),     stdin: c.stdin !== null, killed: c.killed, spawnargs: c.spawnargs.slice(-2).length,
  });
  show('info', await run(c));
  show('after', { connected: c.connected, channel: c.channel, exitCode: c.exitCode });

  const values = [null, 0, -1.5, 'text', true, '', 'multi\nline x', 'héllo 😀', [], [1, [2, [3]]],
    {}, { a: { b: [1, 'x', null] }, 'k y': 'v' }, { cmd: 'NODE_internal' }, { cmd: 'NODE' }, { cmd: 5 }, { toJSON() { return 'json'; } }];
  c = cp.fork(child, ['echo'], forkOpts());
  const results = [];
  const done = run(c, (m) => { results.push(m); if (results.length === values.length - 2) c.send({ cmd: 'quit' }); });
  const returns = values.map((v) => c.send(v, (e) => { if (e) show('send error', err(e)); }));
  show('returns', returns);
  const echoed = await done;
  show('echo', echoed);

  c = cp.fork(child, ['echo'], forkOpts());
  const bad = (label, fn) => { try { show(label, fn()); } catch (e) { show(label, err(e)); } };
  bad('send()', () => c.send());
  bad('send(undefined)', () => c.send(undefined));
  bad('send(fn)', () => c.send(() => 1));
  bad('send(symbol)', () => c.send(Symbol('s')));
  bad('send(bigint)', () => c.send(1n));
  bad('send(handle)', () => c.send('x', {}));
  bad('send bad options', () => c.send('x', undefined, 5));
  bad('send circular', () => { const o = {}; o.o = o; return c.send(o); });
  bad('send callback', () => c.send('ok', undefined, {}, () => show('callback ran', true)));
  c.send({ cmd: 'quit' });
  await run(c);
  c = cp.fork(child, ['echo'], forkOpts());
  c.on('error', (e) => show('error event', err(e)));
  c.disconnect();
  show('connected after disconnect', c.connected);
  c.disconnect();
  show('send after disconnect', c.send('x', (e) => show('send cb', err(e))));
  c.send('y');
  await new Promise((resolve) => c.on('exit', resolve));

  c = cp.fork(child, ['selfdisconnect'], forkOpts({ silent: true }));
  let printed = '';
  c.stdout.on('data', (d) => { printed += d; });
  show('selfdisconnect', { ...(await run(c)), printed });

  c = cp.fork(child, ['exit'], forkOpts());
  c.send(7);
  show('exit 7', await run(c));
  c = cp.fork(child, ['hold'], forkOpts());
  c.once('message', () => c.kill('SIGKILL'));
  show('sigkill', await run(c));
  c = cp.fork(child, ['noop'], forkOpts());
  show('noop child exits by itself', await run(c));
  c = cp.fork(child, ['addremove'], forkOpts());
  show('listener removed', await run(c));
  c = cp.fork(child, ['once'], forkOpts());
  c.send('first');
  c.send('second');
  show('once', await run(c, () => c.disconnect(), 'exit'));
  c = cp.fork(child, ['early'], forkOpts());
  c.on('message', (m) => {
    if (m === 'armed') { c.send('one'); c.send('two'); c.send('last'); }
    if (m === 'last') c.send({ cmd: 'quit' });
  });
  show('early messages', await run(c));
  c = cp.fork(child, ['hold'], forkOpts());
  c.once('message', () => c.disconnect());
  show('parent disconnect', await run(c, undefined, 'exit'));
}

main().then(async () => {
  const exec = process.execPath;
  const o = (x) => forkOpts(x);
  let c = cp.fork(child, ['info'], o({ stdio: ['pipe', 'pipe', 'pipe', 'ipc'] }));
  show('explicit stdio', { stdio: c.stdio.map((s) => s === null ? null : typeof s), ...(await run(c)) });
  c = cp.fork(child, ['info'], o({ silent: true }));
  c.stdout.on('data', () => {});
  await run(c);
  for (const [label, fn] of [
    ['no ipc', () => cp.fork(child, [], { stdio: ['pipe', 'pipe', 'pipe'] })],
    ['two ipc', () => cp.fork(child, [], { stdio: ['ipc', 'ipc'] })],
    ['bad modulePath', () => cp.fork(5)],
    ['url modulePath', () => cp.fork(require('url').pathToFileURL(child), ['noop'], o({ silent: true })).kill() && 'forked'],
    ['bad args', () => cp.fork(child, 5)],
    ['bad serialization', () => cp.fork(child, [], o({ serialization: 'binary' }))],
    ['bad options', () => cp.fork(child, [], 'x')],
  ]) {
    try { fn(); show(label, 'no error'); } catch (e) { show(label, err(e)); }
  }
  c = cp.fork(child, ['info'], { execPath: '/no/such/node', stdio: 'ignore' });
  c.on('error', (e) => show('missing execPath', err(e)));
  show('missing execPath close', await new Promise((resolve) => c.on('close', (code) => resolve(code))));
  void exec;
});
