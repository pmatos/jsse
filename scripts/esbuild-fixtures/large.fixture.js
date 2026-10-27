// RUN: 0
// Messages far above a pipe buffer in both directions: a ~600 KB transform input, a stdin build, many
// concurrent builds sharing the one service, and an output of several MB.
const { esbuild, show, sha, run } = require('./lib');

run(async () => {
  const big = Array.from({ length: 20000 }, (_, i) => `export const v${i} = ${i} + "é${i}";`).join('\n');
  const transformed = await esbuild.transform(big, { loader: 'js', minify: true });
  show('transform', [big.length, transformed.code.length, sha(transformed.code)]);
  const built = await esbuild.build({ stdin: { contents: big, loader: 'js' }, bundle: true, write: false, format: 'esm', minify: true, metafile: true });
  show('build', [built.outputFiles[0].contents.length, sha(built.outputFiles[0].contents), Object.keys(built.metafile.inputs)]);
  const huge = 'export const s = "' + 'x'.repeat(4 * 1024 * 1024) + '";';
  const out = await esbuild.transform(huge, { loader: 'js' });
  show('huge', [out.code.length, sha(out.code)]);
  const many = await Promise.all(Array.from({ length: 40 }, (_, i) => esbuild.build({
    stdin: { contents: `console.log(${i}, "${'y'.repeat(i * 100)}")` }, write: false,
    plugins: [{ name: 'p', setup(build) { build.onEnd(() => {}); } }],
  })));
  show('concurrent', sha(many.map((r) => r.outputFiles[0].text).join('')));
});
