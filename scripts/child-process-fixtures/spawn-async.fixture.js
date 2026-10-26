// RUN: 0
const cp = require('child_process');
const { selfArgs, md5, err, show, finish } = require('./lib');

async function main() {
  let c = cp.spawn('sh', ['-c', 'echo out; echo err 1>&2; exit 3']);
  show('shape', {
    pidType: typeof c.pid, stdio: c.stdio.length, spawnfile: c.spawnfile, spawnargs: c.spawnargs,
    killed: c.killed, exitCode: c.exitCode, signalCode: c.signalCode, connected: c.connected,
    stdin: c.stdin.constructor.name !== '', stdout: c.stdout.readable, stdinWritable: c.stdin.writable,
  });
  show('exit 3', await finish(c));
  show('after', { exitCode: c.exitCode, signalCode: c.signalCode, killed: c.killed, pidType: typeof c.pid });

  c = cp.spawn('no-such-command-xyz', ['arg']);
  show('enoent', await finish(c));
  show('enoent props', { pid: c.pid, exitCode: c.exitCode, spawnfile: c.spawnfile });
  c = cp.spawn('no-such-command-xyz');
  c.removeAllListeners('error');
  await new Promise((resolve) => { c.on('error', (e) => { show('enoent err', err(e)); resolve(); }); });
  await new Promise((resolve) => setTimeout(resolve, 20));

  c = cp.spawn('echo', ['x'], { cwd: '/no/such/dir' });
  show('bad cwd', await finish(c));

  c = cp.spawn('sleep', ['10']);
  c.on('spawn', () => { show('kill ret', c.kill()); show('killed', c.killed); show('kill again', c.kill()); });
  show('sigterm', await finish(c));
  c = cp.spawn('sleep', ['10']);
  setTimeout(() => c.kill('SIGKILL'), 20);
  show('sigkill', await finish(c));
  c = cp.spawn('sh', ['-c', 'trap "echo caught; exit 4" USR1; echo ready; while :; do sleep 0.05; done']);
  c.stdout.once('data', () => c.kill('SIGUSR1'));
  show('usr1', await finish(c));
  c = cp.spawn('sh', ['-c', 'exit 0']);
  await finish(c);
  show('kill after exit', c.kill());
  c = cp.spawn('sleep', ['10']);
  show('kill 0', c.kill(0));
  c.kill();
  await finish(c);
  try { cp.spawn('sleep', ['10']).kill('SIGNOPE'); } catch (e) { show('bad signal', err(e)); }

  c = cp.spawn('cat');
  const done = finish(c);
  c.stdin.write('hello ');
  c.stdin.write(Buffer.from('world'));
  c.stdin.end('!');
  show('cat', await done);
  c = cp.spawn('cat');
  const piped = finish(c);
  c.stdin.end();
  show('cat empty', await piped);

  c = cp.spawn('sh', ['-c', 'echo $FOO:$BAR'], { env: { FOO: 'a', BAR: 'b' } });
  show('env', await finish(c));
  c = cp.spawn('pwd', { cwd: '/usr' });
  show('cwd', await finish(c));
  c = cp.spawn('echo hello | tr a-z A-Z', { shell: true });
  show('shell', await finish(c));
  c = cp.spawn('echo', ['x'], { stdio: 'ignore' });
  show('ignore', { stdin: c.stdin, stdout: c.stdout, stdio: c.stdio, ...(await finish(c)) });
  c = cp.spawn('sh', ['-c', 'echo a; echo b 1>&2'], { stdio: ['ignore', 'pipe', 'inherit'] });
  show('stdio mixed', await finish(c));
  c = cp.spawn('sh', ['-c', 'echo to3 1>&3; echo hello'], { stdio: ['pipe', 'pipe', 'pipe', 'pipe'] });
  const extra = [];
  c.stdio[3].on('data', (d) => extra.push(d));
  show('fd3', { ...(await finish(c)), fd3: Buffer.concat(extra).toString() });
  c = cp.spawn('sh', ['-c', 'cat <&3; echo done'], { stdio: ['ignore', 'pipe', 'pipe', 'pipe'] });
  c.stdio[3].end('through fd3\n');
  show('fd3 in', await finish(c));
  c = cp.spawn('sh', ['-c', 'echo argv0=$0'], { argv0: 'renamed' });
  show('argv0', await finish(c));

  const big = cp.spawn('sh', ['-c', 'head -c 5000000 /dev/zero | tr "\\0" q; head -c 3000000 /dev/zero | tr "\\0" e 1>&2']);
  const chunks = { out: [], err: [] };
  big.stdout.on('data', (d) => chunks.out.push(d));
  big.stderr.on('data', (d) => chunks.err.push(d));
  await new Promise((resolve) => big.on('close', resolve));
  show('big', { out: Buffer.concat(chunks.out).length, err: Buffer.concat(chunks.err).length, md5: md5(Buffer.concat(chunks.out)) });
  const payload = Buffer.alloc(6 * 1024 * 1024, 'abcdefg');
  c = cp.spawn('cat');
  const back = [];
  c.stdout.on('data', (d) => back.push(d));
  c.stdin.end(payload);
  await new Promise((resolve) => c.on('close', resolve));
  show('big echo', { len: Buffer.concat(back).length, same: md5(Buffer.concat(back)) === md5(payload) });
  c = cp.spawn('sh', ['-c', 'head -c 300000 /dev/zero']);
  const paused = [];
  c.on('close', () => show('unread stdout closes', paused.reduce((n, b) => n + b.length, 0)));
  await new Promise((resolve) => setTimeout(resolve, 100));
  c.stdout.on('data', (d) => paused.push(d));
  await new Promise((resolve) => c.on('close', resolve));

  c = cp.spawn(process.execPath, selfArgs('-e', 'console.log("child", process.argv.length); process.stderr.write("oops\\n"); process.exitCode = 6'));
  show('self', await finish(c));
  c = cp.spawn(process.execPath, selfArgs('-e', 'process.kill(process.pid, "SIGTERM"); setTimeout(()=>{}, 5000)'));
  show('self signalled', await finish(c));
  c = cp.spawn(process.execPath, selfArgs('-e', 'console.log(require("fs").readFileSync(0, "utf8").length)'));
  c.stdin.end('x'.repeat(100000));
  show('self reads stdin', await finish(c));

  c = cp.spawn('sleep', ['10'], { timeout: 50 });
  show('timeout', await finish(c));
  c = cp.spawn('sleep', ['10'], { timeout: 50, killSignal: 'SIGKILL' });
  show('timeout kill', await finish(c));
  const ac = new AbortController();
  c = cp.spawn('sleep', ['10'], { signal: ac.signal });
  setTimeout(() => ac.abort(), 20);
  show('abort', await finish(c));
  c = cp.spawn('sleep', ['10'], { signal: AbortSignal.abort() });
  show('abort early', await finish(c));
  c = cp.spawn('sleep', ['0.05'], { detached: true, stdio: 'ignore' });
  c.unref();
  show('detached', { pidType: typeof c.pid });
  c = cp.spawn('sh', ['-c', 'echo $$; ps -o pid=,pgid= -p $$ | tr -s " " | tr " " ":"'], { detached: true });
  const d = await finish(c);
  const [pid, tail] = d.stdout.split('\n');
  show('detached pgid', { sameGroup: tail.split(':').filter(Boolean)[0] === tail.split(':').filter(Boolean)[1] });
  for (const bad of [
    () => cp.spawn(), () => cp.spawn('x', 'y'), () => cp.spawn('x', [], { stdio: 'bogus' }), () => cp.spawn('x', [], { timeout: 'a' }),
    () => cp.spawn('x', [], { signal: 5 }), () => cp.spawn('x', [], { detached: 'a' }),
    () => cp.spawn('x', [], { uid: 'a' }),
  ]) {
    try { bad(); console.log('no throw'); } catch (e) { show('throws', err(e)); }
  }
}
main().then(() => console.log('done'));
