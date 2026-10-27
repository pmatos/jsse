// RUN: 0
// Plugin callbacks in every phase, plugin data, build.resolve, watchFiles, plugin errors,
// virtual modules, the TypeScript build's onLoad remap, onDispose and build.initialOptions.
const fs = require('fs');
const { esbuild, show, run } = require('./lib');

run(async () => {
  const events = [];
  const virtual = {
    name: 'virtual',
    setup(build) {
      events.push(['setup', Object.keys(build.initialOptions).sort(), typeof build.esbuild.build]);
      build.onStart(async () => { events.push(['start']); return { warnings: [{ text: 'from start' }] }; });
      build.onResolve({ filter: /^virt:/ }, (args) => {
        events.push(['resolve', args.path, args.kind, args.namespace, args.importer.replace(process.cwd(), '<cwd>')]);
        return { path: args.path.slice(5), namespace: 'virt', pluginData: { from: args.importer ? 'import' : 'entry' } };
      });
      build.onLoad({ filter: /.*/, namespace: 'virt' }, async (args) => {
        events.push(['load', args.path, args.namespace, args.pluginData]);
        if (args.path === 'main') {
          const resolved = await build.resolve('./src/util.ts', { resolveDir: process.cwd(), kind: 'import-statement' });
          return { contents: `import {greet} from ${JSON.stringify('./' + require('path').relative(process.cwd(), resolved.path))}; import "virt:dep"; console.log(greet("p"))`, loader: 'js', resolveDir: process.cwd(), watchFiles: ['src/util.ts'], watchDirs: ['src'] };
        }
        return { contents: 'export const dep = "é\u{1f600}"; console.log(dep)', loader: 'js', warnings: [{ text: 'load warning', location: { file: 'dep.js', line: 1, column: 0, length: 3, lineText: 'abc' } }] };
      });
      build.onEnd((result) => {
        events.push(['end', result.errors.length, result.warnings.map((w) => w.text), Object.keys(result.metafile || {}), result.outputFiles.length]);
        result.warnings.push({ id: 'x', pluginName: 'virtual', text: 'added in end', location: null, notes: [], detail: 1 });
      });
      build.onDispose(() => events.push(['dispose']));
    },
  };
  const remap = {
    name: 'remap',
    setup(build) {
      build.onLoad({ filter: /src[\\/]lazy\.js$/ }, () => ({ contents: 'export * from "./util.ts"', resolveDir: 'src', loader: 'ts' }));
    },
  };
  const result = await esbuild.build({
    entryPoints: ['virt:main'], bundle: true, write: false, outdir: 'out', metafile: true, logLevel: 'silent',
    plugins: [virtual, remap],
  });
  show('events', events);
  show('result', { errors: result.errors, warnings: result.warnings, out: result.outputFiles.map((f) => [f.path.replace(process.cwd(), '<cwd>'), f.text]), meta: result.metafile });

  const failing = await esbuild.build({
    entryPoints: ['src/lazy.js'], bundle: true, write: false, logLevel: 'silent',
    plugins: [{
      name: 'boom',
      setup(build) {
        build.onLoad({ filter: /lazy\.js$/ }, () => { throw new Error('plugin threw'); });
      },
    }],
  }).catch((e) => e);
  show('thrown', { errors: failing.errors.map((e) => [e.text, e.pluginName, e.detail && e.detail.message]) });

  const rejected = await esbuild.build({
    entryPoints: ['src/lazy.js'], bundle: true, write: false, logLevel: 'silent',
    plugins: [{ name: 'rej', setup(build) { build.onResolve({ filter: /./ }, () => ({ errors: [{ text: 'returned error', location: { file: 'x.js', line: 3, column: 1, length: 2, lineText: 'abcd' }, notes: [{ text: 'note' }] }] })); } }],
  }).catch((e) => e);
  show('returned', { message: rejected.message, errors: rejected.errors });

  const sync = (() => { try { esbuild.buildSync({ entryPoints: ['src/util.ts'], plugins: [{ name: 'p', setup() {} }] }); } catch (e) { return e.message; } })();
  show('sync plugins', sync);

  const temp = 'stage';
  fs.mkdirSync(temp);
  fs.writeFileSync(`${temp}/entry.js`, 'import "x-external"; export default 1');
  const external = await esbuild.build({
    entryPoints: [`${temp}/entry.js`], bundle: true, write: false, format: 'cjs', logLevel: 'silent',
    plugins: [{ name: 'ext', setup(build) { build.onResolve({ filter: /^x-external$/ }, (args) => ({ path: args.path, external: true })); } }],
  });
  show('external', external.outputFiles[0].text);
});
