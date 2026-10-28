// Differential driver for require() of ES modules (#1241); see main.mjs.
const path = require('path');
const { pathToFileURL } = require('url');

const ROOT = __dirname;
const ROOT_URL = pathToFileURL(ROOT).href;
const out = [];
const norm = (v) => String(v).split(ROOT_URL).join('<url>').split(ROOT).join('<root>');
const log = (...a) => out.push(a.map((x) => (typeof x === 'string' ? norm(x) : x === undefined ? 'undefined' : JSON.stringify(x, (k, v) => (typeof v === 'string' ? norm(v) : v)))).join(' '));
const err = (e) => `${e && e.name} ${e && e.code} ${e instanceof SyntaxError ? '-' : norm(String(e && e.message).split('\n')[0])}`;
const attempt = (label, fn) => { try { log(label, fn()); } catch (e) { log(label, 'threw', err(e)); } };

(async () => {
  log('features', process.features.require_module);

  const plain = require('./esm/plain.mjs');
  log('plain', Object.keys(plain), plain.__esModule, typeof plain.default, plain.a, Object.prototype.toString.call(plain));
  log('plain.desc', Object.getOwnPropertyDescriptor(plain, '__esModule'), Object.isFrozen(plain), Object.isExtensible(plain));
  log('plain.cache', plain === require('./esm/plain.mjs'), Object.keys(require.cache).some((k) => k.endsWith('plain.mjs')));
  const importedPlain = await import('./esm/plain.mjs');
  log('plain.import', plain === importedPlain, importedPlain.default === plain.default, Object.keys(importedPlain));

  const nodefault = require('./esm/nodefault.mjs');
  log('nodefault', Object.keys(nodefault), nodefault === (await import('./esm/nodefault.mjs')), nodefault.__esModule);

  const withEsm = require('./esm/withesmodule.mjs');
  log('withesmodule', Object.keys(withEsm), withEsm.__esModule, withEsm === (await import('./esm/withesmodule.mjs')));

  const modExports = require('./esm/modexports.mjs');
  log('modexports', modExports, Object.keys(modExports));

  const live = require('./esm/live.mjs');
  log('live.before', live.count, Object.keys(live));
  live.inc();
  log('live.after', live.count, (await import('./esm/live.mjs')).count);

  for (const [label, id] of [['tla', './esm/tla.mjs'], ['tla-parent', './esm/tla-parent.mjs'], ['throws', './esm/throws.mjs'], ['throws-again', './esm/throws.mjs'], ['syntax', './esm/syntax-error.mjs']]) {
    attempt(`error ${label}`, () => Object.keys(require(id)));
  }
  try { require('./esm/tla-parent.mjs'); } catch (e) { log('tla.stack', e.requireStack, Object.keys(e), String(e.message).split('\n').slice(1).map(norm)); }

  attempt('meta', () => require('./esm/meta.mjs').info);
  attempt('chain', () => Object.keys(require('./cjs/needs-esm.cjs')));
  attempt('chain.esm', () => Object.keys(require('./esm/chain.mjs')));

  attempt('pkg-sync', () => Object.keys(require('pkg-sync')));
  attempt('pkg-sync.require-first', () => require('pkg-sync/only-require').kind);
  attempt('pkg-sync.sync-first', () => Object.keys(require('pkg-sync/sync-first')));
  attempt('esm-pkg', () => Object.keys(require('esm-pkg')));
  attempt('esm-pkg.cjs-user', () => Object.keys(require('./cjs/cjs-pkg-user.cjs').fromPkg));

  attempt('detect.esm', () => Object.keys(require('./detect/esm-syntax.js')));
  attempt('detect.plain', () => require('./detect/plain.js'));
  attempt('detect.syntax-error', () => require('./detect/syntax-error.js'));
  attempt('detect.runtime-syntax', () => require('./detect/runtime-syntax.js'));

  await import('./esm/cycle.mjs');
  const cycle = require('./cjs/req-cycle.cjs').err;
  log('cycle', err(cycle), String(cycle && cycle.message).includes('in a cycle'));

  console.log(out.join('\n'));
})();
