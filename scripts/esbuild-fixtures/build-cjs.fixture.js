// RUN: 0
// The TypeScript build's bundler options: node/cjs, linked sourcemap, banner and footer,
// a define, externals and a post-processing onEnd plugin that rewrites the output file.
const fs = require('fs');
const { esbuild, show, tree, text, run } = require('./lib');

run(async () => {
  const outfile = 'out/main.js';
  const fakeName = 'QQQQQQQ';
  const options = {
    entryPoints: ['./src/main.ts'],
    banner: { js: '// banner\nvar ts = {}; ((module) => {' },
    footer: { js: '})({ get exports() { return ts; }, set exports(v) { ts = v; } })' },
    bundle: true,
    outfile,
    platform: 'node',
    target: ['es2020', 'node14.17'],
    format: 'cjs',
    sourcemap: 'linked',
    sourcesContent: false,
    packages: 'external',
    logLevel: 'warning',
    metafile: true,
    define: { BUILD_ID: '"b1"', require: fakeName },
    external: ['fs'],
    plugins: [{
      name: 'post-process',
      setup(build) {
        build.onEnd(async () => {
          const contents = await fs.promises.readFile(outfile, 'utf-8');
          await fs.promises.writeFile(outfile, contents.replace(new RegExp(fakeName, 'g'), 'require'));
        });
      },
    }],
  };
  const result = await esbuild.build(options);
  show('errors', [result.errors.length, result.warnings.length]);
  show('files', tree('out'));
  console.log(text(outfile));
  show('metafile', result.metafile);
});
