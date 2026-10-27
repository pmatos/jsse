// RUN: 0
// The service child dies under a live context: later calls fail instead of hanging.
const cp = require('child_process');
const { esbuild, show, run } = require('./lib');

run(async () => {
  const ctx = await esbuild.context({ stdin: { contents: '1' }, write: false });
  const pids = cp.execFileSync('pgrep', ['-P', String(process.pid)]).toString().trim().split('\n').map(Number);
  show('children', pids.length);
  process.kill(pids[0], 'SIGKILL');
  await new Promise((resolve) => setTimeout(resolve, 300));
  show('rebuild', await ctx.rebuild().then(() => 'ok', (e) => e.message));
  show('transform', await esbuild.transform('1').then(() => 'ok', (e) => e.message));
  show('transform again', await esbuild.transform('1').then(() => 'ok', (e) => e.message));
});
