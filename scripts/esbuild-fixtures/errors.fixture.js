// RUN: 0
// STDERR: same
// Failure shapes: errors and warnings with locations and notes, logLimit, a missing entry, a bad
// tsconfig, and the log output the native binary writes straight to the inherited stderr.
const { esbuild, show, run } = require('./lib');
const fs = require('fs');

run(async () => {
  const fail = (options) => esbuild.build({ logLevel: 'silent', write: false, ...options }).then((r) => ['ok', r.warnings], (e) => [e.message, e.errors, e.warnings]);
  show('missing entry', await fail({ entryPoints: ['nope.ts'] }));
  show('unresolved', await fail({ stdin: { contents: 'import "./missing"', sourcefile: 'in.js', resolveDir: '.' }, bundle: true }));
  show('syntax', await fail({ stdin: { contents: 'let a = 1;\nlet a = 2;', sourcefile: 'dup.js' } }));
  show('bad format', await fail({ stdin: { contents: '1' }, format: 'umd' }));
  fs.writeFileSync('bad.json', '{ "compilerOptions": { "target": ');
  show('bad tsconfig', await fail({ entryPoints: ['src/util.ts'], tsconfig: 'bad.json', bundle: true }));
  show('log limit', await fail({ stdin: { contents: 'a b c d e f', sourcefile: 'l.js' }, logLimit: 2 }));
  show('warnings', await fail({ stdin: { contents: 'if (x === -0) {}\nlet y = typeof x == "nope"', sourcefile: 'w.js' } }));
  show('verbose', await esbuild.build({ stdin: { contents: 'if (x === -0) {}', sourcefile: 'w2.js' }, write: false, logLevel: 'warning', color: false }).then((r) => r.warnings.length));
  show('error to stderr', await esbuild.build({ stdin: { contents: 'let = =', sourcefile: 'e.js' }, write: false, logLevel: 'error', color: false }).catch((e) => e.errors.length));
  show('info to stderr', await esbuild.build({ entryPoints: ['src/util.ts'], bundle: true, outfile: 'out/u.js', logLevel: 'info', color: false }).then((r) => r.errors.length));
  show('transform failure', await esbuild.transform('x = ', { loader: 'js', logLevel: 'silent' }).catch((e) => [e.message, e.errors]));
  show('invalid call', [
    await esbuild.build({ entryPoints: 'src/util.ts', logLevel: 'silent' }).catch((e) => e.message),
    await esbuild.build({ nope: 1, logLevel: 'silent' }).catch((e) => e.message),
    await esbuild.transform(1).catch((e) => e.message),
    await esbuild.context({ entryPoints: ['src/util.ts'], plugins: [{ name: 1 }], logLevel: 'silent' }).catch((e) => e.message),
  ]);
});
