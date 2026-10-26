// RUN: 0
const cp = require('child_process');
const { selfArgs, md5, err, show } = require('./lib');

function summary(r) {
  return {
    keys: Object.keys(r), status: r.status, signal: r.signal,
    stdout: r.stdout === undefined ? undefined : r.stdout && String(r.stdout),
    stderr: r.stderr === undefined ? undefined : r.stderr && String(r.stderr),
    output: r.output && r.output.map((b) => (b === null ? null : String(b))),
    pidType: typeof r.pid, error: err(r.error),
  };
}

show('echo', summary(cp.spawnSync('echo', ['hello', 'world'])));
show('encoding', summary(cp.spawnSync('echo', ['hi'], { encoding: 'utf8' })));
show('cat input', summary(cp.spawnSync('cat', { input: 'from input' })));
show('cat buffer input', summary(cp.spawnSync('cat', { input: Buffer.from([104, 105, 0, 255]) })));
show('cat dataview', summary(cp.spawnSync('cat', { input: new DataView(new Uint8Array([65, 66]).buffer) })));
show('exit code', summary(cp.spawnSync('sh', ['-c', 'echo out; echo err 1>&2; exit 7'])));
show('signal', summary(cp.spawnSync('sh', ['-c', 'kill -TERM $$'])));
show('signal kill', summary(cp.spawnSync('sh', ['-c', 'kill -9 $$'])));
show('enoent', summary(cp.spawnSync('no-such-command-xyz', ['a', 'b'])));
show('enoent cwd', summary(cp.spawnSync('echo', ['x'], { cwd: '/no/such/dir' })));
show('shell', summary(cp.spawnSync('echo $((1+2)) | tr 1-9 a-i', { shell: true })));
show('shell path', summary(cp.spawnSync('echo "$0"', { shell: '/bin/sh' })));
show('cwd', summary(cp.spawnSync('pwd', { cwd: '/usr' })));
show('env', summary(cp.spawnSync('sh', ['-c', 'echo "$FOO:$BAR:${HOME-unset}"'], { env: { FOO: 'a', BAR: 'b' } })));
process.env.PROP_TEST = 'propagated';
show('process.env', summary(cp.spawnSync('sh', ['-c', 'echo $PROP_TEST'])));
show('argv0', summary(cp.spawnSync('sh', ['-c', 'echo $0'], { argv0: 'custom0' })));
show('stdio ignore', summary(cp.spawnSync('echo', ['x'], { stdio: 'ignore' })));
show('stdio pipe/ignore', summary(cp.spawnSync('sh', ['-c', 'echo a; echo b 1>&2'], { stdio: ['pipe', 'ignore', 'pipe'] })));
show('stdio fd3 out', summary(cp.spawnSync('sh', ['-c', 'echo to-fd3 1>&3; echo to-out'], { stdio: ['pipe', 'pipe', 'pipe', 'pipe'] })));
show('timeout', summary(cp.spawnSync('sleep', ['5'], { timeout: 100 })));
show('timeout killSignal', summary(cp.spawnSync('sh', ['-c', 'trap "" TERM; sleep 5'], { timeout: 100, killSignal: 'SIGKILL' })));
show('maxBuffer', summary(cp.spawnSync('sh', ['-c', 'head -c 100000 /dev/zero | tr "\\0" a'], { maxBuffer: 1000 })).error);
const big = cp.spawnSync('sh', ['-c', 'head -c 3000000 /dev/zero | tr "\\0" b'], { maxBuffer: Infinity });
show('big out', { status: big.status, len: big.stdout.length, md5: md5(big.stdout) });
const payload = Buffer.alloc(4 * 1024 * 1024, 'xyz');
const echo = cp.spawnSync('cat', { input: payload, maxBuffer: Infinity });
show('big in/out', { status: echo.status, len: echo.stdout.length, md5: md5(echo.stdout), same: md5(payload) });
const both = cp.spawnSync('sh', ['-c', 'cat; head -c 2000000 /dev/zero 1>&2'], { input: payload, maxBuffer: Infinity });
show('big in/out/err', { len: both.stdout.length, errlen: both.stderr.length });
show('stdin closed', summary(cp.spawnSync('sh', ['-c', 'cat | wc -c'])));
show('self', summary(cp.spawnSync(process.execPath, selfArgs('-e', 'console.log(process.argv.length, JSON.stringify(process.argv.slice(2))); process.exit(5)')), 'x'));
show('self stdin', summary(cp.spawnSync(process.execPath, selfArgs('-e', 'console.log(require("fs").readFileSync(0, "utf8").toUpperCase())'), { input: 'shout' })));
show('self args', summary(cp.spawnSync(process.execPath, selfArgs('-e', 'console.log(JSON.stringify(process.argv.slice(1)))', 'a', '--b', 'c d'))));
for (const bad of [
  () => cp.spawnSync(''), () => cp.spawnSync(1), () => cp.spawnSync('x', 5), () => cp.spawnSync('x', [], { timeout: -1 }),
  () => cp.spawnSync('x', [], { killSignal: 'SIGNOPE' }), () => cp.spawnSync('x', [], { stdio: 'bogus' }),
  () => cp.spawnSync('x\0y'), () => cp.spawnSync('x', ['a\0b']), () => cp.spawnSync('x', [], { maxBuffer: -1 }),
  () => cp.spawnSync('x', [], { cwd: 5 }), () => cp.spawnSync('x', [], { shell: 5 }),
]) {
  try { bad(); console.log('no throw'); } catch (e) { show('throws', err(e)); }
}
