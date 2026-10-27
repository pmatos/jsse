// RUN: 0
// context: rebuild after an edit, watch with an onEnd plugin, cancel and dispose; the process
// must exit by itself once the context is disposed.
const fs = require('fs');
const { esbuild, show, run, text } = require('./lib');

run(async () => {
  fs.writeFileSync('entry.js', 'console.log("one")');
  const ctx = await esbuild.context({ entryPoints: ['entry.js'], bundle: true, outfile: 'out/entry.js', logLevel: 'silent', metafile: true });
  const first = await ctx.rebuild();
  show('first', [Object.keys(first.metafile.outputs), text('out/entry.js').includes('"one"')]);
  fs.writeFileSync('entry.js', 'console.log("two")');
  await ctx.rebuild();
  show('second', text('out/entry.js').includes('"two"'));
  const [a, b] = await Promise.all([ctx.rebuild(), ctx.rebuild()]);
  show('parallel', [a.errors.length, b.errors.length]);
  await ctx.dispose();
  await ctx.dispose().then(() => show('dispose twice', 'ok'), (e) => show('dispose twice', e.message));
  await ctx.rebuild().then(() => show('after dispose', 'ok'), (e) => show('after dispose', e.message));

  let builds = 0;
  let nextBuild;
  let waiter = new Promise((resolve) => { nextBuild = resolve; });
  const watcher = await esbuild.context({
    entryPoints: ['entry.js'], bundle: true, outfile: 'out/watched.js', logLevel: 'silent',
    plugins: [{ name: 'count', setup(build) { build.onEnd(() => { builds += 1; nextBuild(); }); } }],
  });
  await watcher.watch();
  await waiter;
  waiter = new Promise((resolve) => { nextBuild = resolve; });
  // the watcher polls the file's mtime and size
  for (let i = 0; i < 100 && builds < 2; i++) {
    fs.writeFileSync('entry.js', `console.log("edit ${i}")`);
    await Promise.race([waiter, new Promise((resolve) => setTimeout(resolve, 100))]);
  }
  show('rebuilt by watch', [builds >= 2, text('out/watched.js').includes('console.log("edit ')]);
  await watcher.cancel();
  await watcher.dispose();

  let loading;
  const started = new Promise((resolve) => { loading = resolve; });
  const slow = await esbuild.context({
    entryPoints: ['entry.js'], bundle: true, write: false, logLevel: 'silent',
    plugins: [{ name: 'slow', setup(build) { build.onLoad({ filter: /entry\.js$/ }, async () => { loading(); await new Promise((r) => setTimeout(r, 200)); return null; }); } }],
  });
  const pending = slow.rebuild().then(() => 'built', (e) => e.errors.map((x) => x.text));
  await started;
  await slow.cancel();
  show('canceled', await pending);
  await slow.dispose();
  await esbuild.stop();
  show('stopped', true);
});
