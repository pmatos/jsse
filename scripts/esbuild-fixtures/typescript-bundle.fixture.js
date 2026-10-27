// RUN: 0
// NEEDS_ENV: TS_SRC_DIR
// Bundles the real TypeScript services entry (src/typescript/typescript.ts) with the options of its
// Herebyfile.mjs createBundler (exportIsTsObject). The output file digests must equal Node's; the
// source paths in the map and the output path in the metafile are replaced before they are digested.
const fs = require('fs');
const os = require('os');
const path = require('path');
const assert = require('assert');
const { esbuild, sha } = require('./lib');

const src = process.env.TS_SRC_DIR;
const out = fs.mkdtempSync(path.join(os.tmpdir(), 'esbuild-ts-'));

(async () => {
  const outfile = path.join(out, 'typescript.js');
  const fakeName = 'Q'.repeat('require'.length);
  const toCommonJs = /var __toCommonJS .*/;
  const result = await esbuild.build({
    absWorkingDir: src,
    entryPoints: ['./src/typescript/typescript.ts'],
    banner: { js: 'var ts = {}; ((module) => {' },
    footer: { js: '})({ get exports() { return ts; }, set exports(v) { ts = v; if (typeof module !== "undefined" && module.exports) { module.exports = v; } } })' },
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
    define: { require: fakeName },
    plugins: [{
      name: 'post-process',
      setup(build) {
        build.onEnd(async () => {
          let contents = await fs.promises.readFile(outfile, 'utf-8');
          contents = contents.replace(new RegExp(fakeName, 'g'), 'require');
          let matches = 0;
          contents = contents.replace(toCommonJs, () => { matches++; return 'var __toCommonJS = (mod) => (__copyProps, mod);'; });
          assert(matches === 1);
          await fs.promises.writeFile(outfile, contents);
        });
      },
    }],
  });
  console.log('warnings', result.warnings.length, 'errors', result.errors.length);
  for (const name of fs.readdirSync(out).sort()) {
    const data = fs.readFileSync(path.join(out, name), 'latin1').split(path.relative(out, src)).join('<src>');
    console.log(name, data.length, sha(data));
  }
  const meta = JSON.stringify(result.metafile).split(path.relative(src, out)).join('<out>');
  console.log('metafile', meta.length, sha(meta));
  fs.rmSync(out, { recursive: true, force: true });
})().catch((e) => { console.log('UNEXPECTED', e && e.stack || e); process.exitCode = 1; });
