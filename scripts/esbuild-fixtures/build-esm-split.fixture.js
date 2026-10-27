// RUN: 0
// Splitting, esm, hashed names, loaders, minify, external sourcemaps and the dynamic import chunk.
const { esbuild, show, tree, text, run } = require('./lib');

run(async () => {
  const result = await esbuild.build({
    entryPoints: { app: 'src/main.ts', page: 'src/view.tsx' },
    bundle: true,
    outdir: 'dist',
    format: 'esm',
    splitting: true,
    platform: 'node',
    external: ['fs'],
    define: { BUILD_ID: '"b2"' },
    chunkNames: 'chunks/[name]-[hash]',
    entryNames: '[dir]/[name]-[hash]',
    assetNames: 'assets/[name]-[hash]',
    loader: { '.txt': 'text', '.bin': 'file', '.css': 'dataurl' },
    sourcemap: 'external',
    minify: true,
    treeShaking: true,
    metafile: true,
    logLevel: 'silent',
  });
  show('result', [result.errors, result.warnings, Object.keys(result).sort()]);
  show('files', tree('dist'));
  show('metafile', result.metafile);

  const unminified = await esbuild.build({
    entryPoints: ['src/main.ts'], bundle: true, write: false, outdir: 'mem', format: 'iife',
    platform: 'browser', globalName: 'App', loader: { '.txt': 'base64' }, external: ['fs'],
    define: { BUILD_ID: '"b3"' }, legalComments: 'none', sourcemap: 'inline', sourcesContent: false,
  });
  for (const file of unminified.outputFiles) {
    console.log(file.path.replace(process.cwd(), '<cwd>'), file.hash, file.contents.length);
    console.log(file.text);
  }
});
