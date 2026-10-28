// RUN: 1
// An unhandled build failure ends the process with status 1 and the service does not hold it open.
const { esbuild, show } = require('./lib');

show('start', esbuild.version);
esbuild.build({ entryPoints: ['nonexistent.js'], write: false, logLevel: 'silent' });
