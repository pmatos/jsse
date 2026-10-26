// RUN: 0
const cp = require('child_process');
const { selfArgs, md5, err, show } = require('./lib');

function attempt(label, fn) {
  try {
    const r = fn();
    show(label, { ok: Buffer.isBuffer(r) ? ['buf', String(r)] : r });
  } catch (e) {
    show(label, {
      err: err(e), status: e.status, signal: e.signal, pidType: typeof e.pid, cmd: e.cmd,
      stdout: e.stdout && String(e.stdout), stderr: e.stderr && String(e.stderr),
      output: e.output && e.output.map((b) => (b === null ? null : String(b))),
      keys: Object.keys(e),
    });
  }
}

attempt('execSync', () => cp.execSync('echo hi'));
attempt('execSync encoding', () => cp.execSync('echo hi', { encoding: 'utf8' }));
attempt('execSync stdio pipe', () => cp.execSync('echo hi; echo err 1>&2', { stdio: 'pipe' }));
attempt('execSync input', () => cp.execSync('cat', { input: 'fed', encoding: 'utf8' }));
attempt('execSync fail', () => cp.execSync('echo out; echo bad 1>&2; exit 4', { stdio: 'pipe' }));
attempt('execSync signal', () => cp.execSync('kill -9 $$', { stdio: 'pipe' }));
attempt('execSync enoent', () => cp.execSync('no-such-command-xyz', { stdio: 'pipe' }));
attempt('execSync cwd/env', () => cp.execSync('pwd; echo $E', { cwd: '/usr', env: { E: 'v' }, encoding: 'utf8' }));
attempt('execSync timeout', () => cp.execSync('sleep 5', { timeout: 50, stdio: 'pipe' }));
attempt('execSync maxBuffer', () => cp.execSync('head -c 5000 /dev/zero', { maxBuffer: 100, stdio: 'pipe' }));
attempt('execSync big', () => md5(cp.execSync('head -c 4000000 /dev/zero | tr "\\0" k', { maxBuffer: 1e7 })));
attempt('execFileSync', () => cp.execFileSync('echo', ['a', 'b']));
attempt('execFileSync input', () => cp.execFileSync('cat', { input: Buffer.from('xyz'), encoding: 'utf8' }));
attempt('execFileSync fail', () => cp.execFileSync('sh', ['-c', 'exit 5'], { stdio: 'pipe' }));
attempt('execFileSync enoent', () => cp.execFileSync('no-such-command-xyz', ['x'], { stdio: 'pipe' }));
attempt('execFileSync self', () => cp.execFileSync(process.execPath, selfArgs('-e', 'console.log("sync child")'), { encoding: 'utf8' }));
attempt('execFileSync stdin roundtrip', () => {
  const payload = Buffer.alloc(3 * 1024 * 1024, 'q');
  const out = cp.execFileSync('cat', { input: payload, maxBuffer: 1e8 });
  return { same: md5(out) === md5(payload), len: out.length };
});
attempt('execSync stderr inherit', () => cp.execSync('echo to-stderr 1>&2; echo to-stdout', { encoding: 'utf8', stdio: ['pipe', 'pipe', 'inherit'] }));
attempt('execSync bad', () => cp.execSync(5));
attempt('execFileSync bad', () => cp.execFileSync());
