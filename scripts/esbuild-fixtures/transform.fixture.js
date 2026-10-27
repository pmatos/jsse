// RUN: 0
// transform and the synchronous API (which spawns the service once per call), formatMessages,
// analyzeMetafile and the version.
const { esbuild, show, run, text } = require('./lib');

run(async () => {
  show('version', esbuild.version);
  const ts = 'enum E { A = 1, B } export class C<T> { constructor(public x: T) {} }\nexport const e: E = E.B;';
  for (const options of [
    { loader: 'ts' },
    { loader: 'ts', minify: true, format: 'cjs', target: 'es2015' },
    { loader: 'ts', sourcemap: 'external', sourcefile: 'in.ts', sourcesContent: false },
    { loader: 'ts', sourcemap: 'inline', define: { E: 'Other' } },
  ]) show('transform', await esbuild.transform(ts, options));

  show('tsx', await esbuild.transform('const a = <div id="x">{1}</div>', { loader: 'tsx', jsxFactory: 'h' }));
  show('css', await esbuild.transform('a { color: #ff0000; &:hover { color: blue } }', { loader: 'css', minify: true }));
  show('json', await esbuild.transform('{"a": [1, 2]}', { loader: 'json' }));
  show('unicode', await esbuild.transform('export default "é世界😀"', { loader: 'js' }));
  show('sync', esbuild.transformSync('let x: number = 1', { loader: 'ts' }));
  show('buildSync', esbuild.buildSync({ entryPoints: ['src/util.ts'], bundle: true, write: false, format: 'esm', logLevel: 'silent' }).outputFiles.map((f) => f.text));
  show('warning', await esbuild.transform('if (a == -0) {}', { loader: 'js', logLevel: 'silent' }));

  try { await esbuild.transform('let x = ;', { loader: 'js', logLevel: 'silent' }); } catch (e) {
    show('syntax error', { message: e.message, errors: e.errors, warnings: e.warnings });
  }
  try { esbuild.transformSync('let = = 1', { loader: 'js', logLevel: 'silent' }); } catch (e) {
    show('sync error', { message: e.message, errors: e.errors.map((x) => x.text) });
  }
  try { await esbuild.transform('1', { loader: 'nope' }); } catch (e) { show('bad option', e.message); }

  const messages = [{ text: 'boom', location: { file: 'a.js', line: 1, column: 4, length: 3, lineText: 'let abc = 1' }, notes: [{ text: 'a note' }] }];
  show('format error', await esbuild.formatMessages(messages, { kind: 'error', color: false }));
  show('format warning', await esbuild.formatMessages(messages, { kind: 'warning', color: false, terminalWidth: 40 }));
  show('format color', await esbuild.formatMessages(messages, { kind: 'error', color: true }));

  const meta = (await esbuild.build({ entryPoints: ['src/main.ts'], bundle: true, write: false, metafile: true, external: ['fs'], define: { BUILD_ID: '1' }, outdir: 'o' })).metafile;
  console.log(await esbuild.analyzeMetafile(meta));
  console.log(await esbuild.analyzeMetafile(JSON.stringify(meta), { verbose: true }));
  console.log(text('src/lazy.js').length);
});
