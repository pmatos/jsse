// RUN: 0
// After the last request the service no longer keeps the process alive: no stop() and no dispose(),
// yet the process ends by itself with its exit code.
const { esbuild, show, run } = require('./lib');

run(async () => {
  show('first', (await esbuild.transform('1 + 1')).code);
  show('second', (await esbuild.build({ stdin: { contents: '2' }, write: false })).outputFiles[0].text);
  process.on('exit', (code) => console.log('exit', code));
});
