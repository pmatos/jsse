// RUN: 3
// process.exit while a build is in flight: the exit code wins, the buffered stdout is flushed
// and the service child is released.
const { esbuild, run } = require('./lib');

run(async () => {
  esbuild.build({ entryPoints: ['src/main.ts'], bundle: true, write: false, external: ['fs'] });
  console.log('building');
  setTimeout(() => { console.log('exiting'); process.exit(3); }, 0);
});
