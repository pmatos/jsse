const path = require('path');
const out = [
  typeof require, typeof module, typeof exports, __filename, __dirname,
  module.id, module.filename === path.join(process.cwd(), '[eval]'),
  require.main, module.paths.slice(0, 2).map((p) => path.relative(process.cwd(), p)),
  this === globalThis, Object.keys(globalThis).filter((k) => ['require', 'module', 'exports', '__filename', '__dirname'].includes(k)),
  require('./lib/dir'), require('plain'), require.resolve('./lib/data.json') === path.join(process.cwd(), 'lib/data.json'),
  require.resolve.paths('./x').map((p) => path.relative(process.cwd(), p) || '.'),
];
console.log(JSON.stringify(out, (k, v) => v === undefined ? '[undefined]' : v));
