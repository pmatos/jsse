// RUN: 0
// NEEDS: string_decoder
const cp = require('child_process');
const util = require('util');
const { selfArgs, md5, err, show } = require('./lib');

function run(label, fn) {
  return new Promise((resolve) => {
    let child;
    const cb = (error, stdout, stderr) => {
      show(label, {
        error: error && { ...err(error), killed: error.killed, signal: error.signal, cmd: error.cmd, status: error.status },
        stdout: Buffer.isBuffer(stdout) ? ['buf', String(stdout)] : stdout,
        stderr: Buffer.isBuffer(stderr) ? ['buf', String(stderr)] : stderr,
      });
      resolve(child);
    };
    child = fn(cb);
  });
}

async function main() {
  await run('exec ok', (cb) => cp.exec('echo out; echo err 1>&2', cb));
  await run('exec fail', (cb) => cp.exec('echo before; echo oops 1>&2; exit 9', cb));
  await run('exec signal', (cb) => cp.exec('kill -TERM $$', cb));
  await run('exec enoent', (cb) => cp.exec('no-such-command-xyz', cb));
  await run('exec options', (cb) => cp.exec('echo $X; pwd', { cwd: '/usr', env: { X: 'envx' } }, cb));
  await run('exec buffer', (cb) => cp.exec('printf "\\377abc"', { encoding: 'buffer' }, cb));
  await run('exec latin1', (cb) => cp.exec('printf "\\377abc"', { encoding: 'latin1' }, cb));
  await run('exec shell', (cb) => cp.exec('echo $0', { shell: '/bin/sh' }, cb));
  await run('exec timeout', (cb) => cp.exec('sleep 5', { timeout: 50 }, cb));
  await run('exec timeout kill', (cb) => cp.exec('sleep 5', { timeout: 50, killSignal: 'SIGKILL' }, cb));
  await run('exec maxBuffer', (cb) => cp.exec('head -c 5000 /dev/zero | tr "\\0" a', { maxBuffer: 100 }, cb));
  await run('exec maxBuffer err', (cb) => cp.exec('head -c 5000 /dev/zero | tr "\\0" a 1>&2', { maxBuffer: 100 }, cb));
  await run('exec big', (cb) => cp.exec('head -c 4000000 /dev/zero | tr "\\0" z', { maxBuffer: 5e6 }, (e, out, er) => cb(e, md5(out), er)));
  await run('exec nocb', () => { const c = cp.exec('echo x'); c.on('close', () => show('exec nocb', 'closed')); return c; }).catch(() => {});
  await run('execFile ok', (cb) => cp.execFile('echo', ['a', 'b c'], cb));
  await run('execFile fail', (cb) => cp.execFile('sh', ['-c', 'exit 2'], cb));
  await run('execFile enoent', (cb) => cp.execFile('no-such-command-xyz', ['q'], cb));
  await run('execFile options', (cb) => cp.execFile('pwd', { cwd: '/usr' }, cb));
  await run('execFile abort', (cb) => {
    const ac = new AbortController();
    setTimeout(() => ac.abort(), 30);
    return cp.execFile('sleep', ['5'], { signal: ac.signal }, cb);
  });
  await run('execFile self', (cb) => cp.execFile(process.execPath, selfArgs('-e', 'console.log(1+1)'), cb));
  const p = util.promisify(cp.exec)('echo promised');
  show('promisify', { out: await p, child: typeof p.child.pid });
  try { await util.promisify(cp.exec)('echo e; exit 3'); } catch (e) { show('promisify reject', { code: e.code, stdout: e.stdout, stderr: e.stderr }); }
  const pf = await util.promisify(cp.execFile)('echo', ['pf']);
  show('promisify execFile', pf);
  for (const bad of [
    () => cp.exec(), () => cp.exec(5), () => cp.exec('x', 5, 5), () => cp.execFile(), () => cp.execFile('x', [], { timeout: -1 }),
    () => cp.execFile('x', [], { maxBuffer: 'a' }), () => cp.execFile('x', [], { killSignal: 'NOPE' }), () => cp.execFile('x', {}, 5),
  ]) {
    try { bad(); console.log('no throw'); } catch (e) { show('throws', err(e)); }
  }
}
main().then(() => console.log('done'));
