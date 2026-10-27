// Differential driver for import.meta and the CommonJS side of ES module
// interop (#1241): runs on jsse --node and on Node and must print the same.
import path from 'node:path';
import { createRequire } from 'node:module';
import { pathToFileURL } from 'node:url';
import mutating, { n, bump } from './cjs/mutating.cjs';
import { count, inc } from './esm/live.mjs';

const ROOT = path.dirname(import.meta.filename);
const ROOT_URL = pathToFileURL(ROOT).href;
const out = [];
const norm = (v) => String(v).split(ROOT_URL).join('<url>').split(ROOT).join('<root>');
const log = (...a) => out.push(a.map((x) => (typeof x === 'string' ? norm(x) : x === undefined ? 'undefined' : JSON.stringify(x, (k, v) => (typeof v === 'string' ? norm(v) : v)))).join(' '));
const err = (e) => `${e && e.name} ${e && e.code} ${e instanceof SyntaxError ? '-' : norm(String(e && e.message).split('\n')[0])}`;
const attempt = (label, fn) => { try { log(label, fn()); } catch (e) { log(label, 'threw', err(e)); } };

// import.meta
log('meta.keys', Object.keys(import.meta));
log('meta.proto', Object.getPrototypeOf(import.meta));
log('meta.same', import.meta === import.meta);
log('meta.values', import.meta.url, import.meta.filename, import.meta.dirname, import.meta.main);
log('meta.desc', Object.getOwnPropertyDescriptor(import.meta, 'url').writable, Object.getOwnPropertyDescriptor(import.meta, 'resolve').enumerable);
log('meta.resolve.fn', import.meta.resolve.name, import.meta.resolve.length, typeof import.meta.resolve);
for (const s of ['./cjs/dot.cjs', './nope.mjs', './esm', './esm/', './with space/m.mjs', './x.txt', 'cjs-pkg', 'esm-pkg', 'pkg-sync', 'pkg-sync/sync-first', 'pkg-sync/only-require', '#int', 'node:fs', 'fs', 'node:nope', 'data:text/javascript,1', 'https://example.org/a.js', 'file:///abs/x.js', '/abs/y.js', 'missing-pkg', 'missing-pkg/sub', 1, undefined]) {
  attempt(`meta.resolve(${String(s)})`, () => import.meta.resolve(s));
}
log('meta.resolve.parent', import.meta.resolve('./a.mjs', 'file:///elsewhere/b.mjs'));
const spaced = await import('./with space/m.mjs');
log('meta.space', spaced.where);
const dyn = await import('./esm/meta.mjs');
log('meta.dynamic', dyn.info);

// createRequire(import.meta.url)
const require = createRequire(import.meta.url);
log('createRequire.relative', Object.keys(require('./cjs/dot.cjs')));
log('createRequire.resolve', require.resolve('./cjs/dot.cjs'), require.resolve('cjs-pkg'));
log('createRequire.package', require('cjs-pkg').name);
log('createRequire.url-arg', typeof createRequire(new URL('./x.js', import.meta.url)).resolve);
log('createRequire.cache-shared', require('./cjs/dot.cjs') === (await import('./cjs/dot.cjs')).default);
attempt('createRequire.esm', () => Object.keys(require('./esm/nodefault.mjs')));
attempt('createRequire.package-sync', () => require('pkg-sync'));
attempt('createRequire.package-esm', () => Object.keys(require('esm-pkg')));
attempt('createRequire.sync-first', () => require('pkg-sync/sync-first').kind);
attempt('createRequire.only-require', () => require('pkg-sync/only-require').kind);

// import of CommonJS: named exports, default, module.exports
const cases = ['dot', 'define', 'literal', 'literal-stop', 'reexport', 'reexport-reset', 'babel', 'tsstar', 'strings', 'computed', 'esmodule', 'throwing', 'nonobject', 'needs-esm'];
for (const name of cases) {
  try {
    const ns = await import(`./cjs/${name}.cjs`);
    const keys = Object.keys(ns);
    const values = {};
    for (const k of keys) if (k !== 'default' && k !== 'module.exports') values[k] = typeof ns[k] === 'function' ? '[fn]' : ns[k];
    log('cjs', name, keys, values, 'default===require', ns.default === require(`./cjs/${name}.cjs`), 'module.exports===default', ns['module.exports'] === ns.default);
  } catch (e) {
    log('cjs', name, 'threw', err(e));
  }
}
log('cjs.esModule.default', Object.keys((await import('./cjs/esmodule.cjs')).default), (await import('./cjs/esmodule.cjs')).default.__esModule);

// snapshot versus live bindings
log('snapshot.before', n, mutating.n);
bump();
log('snapshot.after', n, mutating.n, mutating === require('./cjs/mutating.cjs'));
await new Promise((r) => setTimeout(r, 5));
log('snapshot.late', 'late' in (await import('./cjs/mutating.cjs')), mutating.late);
log('live.esm', count);
inc();
log('live.esm.after', count);

// import of an ES module that was required, and the reverse
const required = require('./esm/plain.mjs');
const imported = await import('./esm/plain.mjs');
log('required.facade', Object.keys(required), required.__esModule, required === imported, required.a === imported.a, Object.keys(imported));
const chain = await import('./esm/chain.mjs');
log('chain', chain.same, chain.viaCjsKeys);

// error codes
for (const [label, id] of [['tla', './esm/tla.mjs'], ['tla-parent', './esm/tla-parent.mjs'], ['throws', './esm/throws.mjs'], ['syntax', './esm/syntax-error.mjs'], ['cycle', './esm/cycle.mjs']]) {
  attempt(`require(${label})`, () => Object.keys(require(id)));
}
log('process.features.require_module', process.features.require_module);

console.log(out.join('\n'));
