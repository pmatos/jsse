// Differential driver for the CommonJS loader (#1219): runs on jsse --node and
// on Node, prints normalised results, and the outputs must be identical.
// Every path is shown relative to this directory; errors are shown as data
// (code, message, requireStack), never as stack traces.
const path = require('path');
const ROOT = __dirname;
const PREFIX_LIB = path.resolve(process.execPath, '..', '..', 'lib', 'node');

const ancestors = {};
for (let d = ROOT, i = 0; ; d = path.dirname(d), i++) {
  ancestors[path.join(d, 'node_modules')] = i === 0 ? '<root>/node_modules' : `<up${i}>/node_modules`;
  if (path.dirname(d) === d) break;
}

function norm(value) {
  return JSON.parse(JSON.stringify(value, (k, v) => {
    if (typeof v === 'string' && ancestors[v]) return ancestors[v];
    if (typeof v === 'string') return v.split(PREFIX_LIB).join('<prefix>/lib/node').split(ROOT).join('<root>');
    if (typeof v === 'function') return '[function]';
    if (v === undefined) return '[undefined]';
    return v;
  }));
}

// The chain of parent node_modules directories depends on where the checkout lives.
const isUp = (p) => /^<up\d+>\//.test(p);
const collapse = (list) => list.map((p) => (ancestors[p] && isUp(ancestors[p]) ? '<ancestor>/node_modules' : p))
  .filter((p, i, all) => !(p === '<ancestor>/node_modules' && all[i - 1] === p));

function t(label, fn) {
  let out;
  try {
    out = { ok: fn() };
  } catch (e) {
    out = { error: { name: e.name, code: e.code, message: e.message, requireStack: e.requireStack, path: e.path, requestPath: e.requestPath } };
  }
  console.log(label, JSON.stringify(norm(out)));
}

const req = (id) => () => require(id);

// builtins
t('path===node:path', () => require('path') === require('node:path'));
t('fs===node:fs', () => require('fs') === require('node:fs'));
t('fs.readFileSync', () => typeof require('fs').readFileSync);
t('timers', () => [require('timers') === require('node:timers'), typeof require('timers').setTimeout, require('timers/promises') === require('timers').promises, require('timers').setTimeout === setTimeout]);
t('path.sep', () => require('path').sep);
t('resolve builtin', () => [require.resolve('path'), require.resolve('node:path')]);
t('unknown node: builtin', req('node:nope'));
t('node:internal', req('node:internal/errors'));
t('internal is a plain name', req('internal/errors'));
t('non-string id', req(1));
t('empty id', req(''));
t('object id', req({}));

// relative, absolute, extensions, directories
t('relative with ext', req('./lib/dir/index.js'));
t('relative without ext', req('./lib/dir/index'));
t('directory index', req('./lib/dir'));
t('directory trailing slash', req('./lib/trailing/'));
t('directory ./lib/trailing/.', req('./lib/trailing/.'));
t('package main', req('./lib/pkgdir'));
t('package main missing, index fallback', req('./lib/pkgbad'));
t('package main missing, no index', req('./lib/pkgnoindex'));
t('absolute', () => require(path.join(ROOT, 'lib/dir')));
t('dotted name a.b', req('./lib/ext/a.b'));
t('dotfile', req('./lib/ext/.hidden'));
t('missing relative', req('./lib/missing'));
t('missing bare', req('no-such-package'));
t('missing scoped', req('@scope/nothing'));
t('file as directory', req('./lib/dir/index.js/x'));
t('up and down', req('./sub/deep/viaparent'));
t('up and down, from a nested directory', req('./sub/deep/mod'));
t('unregistered extension is loaded as .js', req('./lib/ext/note.txt'));

// JSON
t('json', req('./lib/data.json'));
t('json without extension', req('./lib/data'));
t('json bom', req('./lib/bom.json'));
t('json invalid', () => { try { require('./lib/bad.json'); } catch (e) { return [e.name, e.message.startsWith(path.join(ROOT, 'lib/bad.json') + ': ')]; } });
t('json cached identity', () => require('./lib/data.json') === require('./lib/data.json'));

// wrapper semantics
t('wrapper', req('./lib/wrapper'));
t('no globals from module scope', req('./lib/noglobals'));
t('shebang', req('./lib/shebang'));
t('toplevel return', req('./lib/toplevel-return'));
t('syntax error', () => { try { require('./lib/syntax'); } catch (e) { return e.name; } });
t('main module', () => [module.id, module.parent, module.loaded, module.filename === __filename, require.main === module, process.mainModule === module, __dirname === ROOT]);
t('module.paths', () => collapse(module.paths));
t('module.path', () => module.path);
t('global leak', () => [typeof globalThis.require, typeof globalThis.module, typeof globalThis.exports, typeof globalThis.__filename, typeof globalThis.__node_run_main, typeof globalThis.__node_builtin, typeof globalThis.__node_builtins, typeof globalThis.__host_builtin_source, typeof globalThis.__host_main_cjs]);
t('this at top level', () => this === module.exports);

// cycles and caching
t('cycle', () => { const a = require('./lib/a'); return [a, require('./lib/b')]; });
t('throwing module not cached', () => {
  const key = path.join(ROOT, 'lib/throws.js');
  const before = module.children.length;
  let first, second;
  try { require('./lib/throws'); } catch (e) { first = e.message; }
  const during = key in require.cache;
  try { require('./lib/throws'); } catch (e) { second = e.message; }
  return [first, second, during, module.children.length === before];
});
t('cache key and identity', () => {
  const m = require('./lib/dir');
  const key = path.join(ROOT, 'lib/dir/index.js');
  return [m, Object.keys(require.cache).includes(key), require.cache[key].exports === m, require.cache[key].loaded, require.cache[key].id === key];
});
t('cache delete reloads', () => {
  const a = require('./lib/counter');
  const b = require('./lib/counter');
  delete require.cache[require.resolve('./lib/counter')];
  const c = require('./lib/counter');
  return [a, b, c];
});
t('module.children', () => module.children.map((c) => path.relative(ROOT, c.filename)).filter((f) => f.startsWith('lib/a') || f.startsWith('lib/dir')));
t('module.parent of required', () => { const m = require.cache[path.join(ROOT, 'lib/dir/index.js')]; return [m.parent === module, m.loaded]; });
t('relative resolution cache after delete', () => {
  const k = require.resolve('./lib/dir');
  delete require.cache[k];
  return [require('./lib/dir'), k === path.join(ROOT, 'lib/dir/index.js')];
});

// node_modules
t('plain package', req('plain'));
t('plain/package.json', req('plain/package.json'));
t('plain/index.js', req('plain/index'));
t('scoped main', req('@scope/pkg'));
t('scoped deep', req('@scope/pkg/lib/extra'));
t('no exports: main', req('noexports'));
t('no exports: deep', req('noexports/lib/deep'));
t('nested node_modules', req('nested'));
t('file wins over directory', req('dual'));
t('directory when ending slash', req('dual/'));
t('NODE_PATH', req('npmod'));
t('HOME .node_modules', req('homemod'));

// exports
t('exports: conditions (require)', req('exp'));
t('exports: subpath', req('exp/feature'));
t('exports: pattern', req('exp/features/one'));
t('exports: longer pattern', req('exp/features/data.json'));
t('exports: null pattern', req('exp/features/private/secret'));
t('exports: array fallback', req('exp/array'));
t('exports: nested conditions', req('exp/cond'));
t('exports: null target', req('exp/nullish'));
t('exports: trailing slash key', req('exp/dir/one'));
t('exports: missing file', req('exp/missing'));
t('exports: package.json export', req('exp/package.json'));
t('exports: not exported', req('exp/lib/main.js'));
t('exports: absolute target', req('exp/abs'));
t('exports: escaping target', req('exp/escape'));
t('exports: bare target', req('exp/bare'));
t('exports: string sugar', req('expstr'));
t('exports: string sugar, not exported subpath', req('expstr/other.js'));
t('exports: conditions order', req('expcond'));
t('exports: mixed keys', req('expbad'));
t('exports: resolve', () => [require.resolve('exp'), require.resolve('exp/feature'), require.resolve('exp/features/one')]);
t('exports: self reference', req('selfref'));
t('exports: imports', req('imp'));
t('exports: self reference, missing target', req('./node_modules/selfmiss/probe'));

// require.resolve and paths
t('resolve relative', () => require.resolve('./lib/dir'));
t('resolve missing', () => require.resolve('./lib/missing'));
t('resolve non-string', () => require.resolve(1));
t('resolve options.paths relative', () => require.resolve('./mod', { paths: [path.join(ROOT, 'sub/deep')] }));
t('resolve options.paths bare', () => require.resolve('npmod', { paths: [path.join(ROOT, 'np')] }));
t('resolve options.paths bare, node_modules', () => require.resolve('plain', { paths: [path.join(ROOT, 'sub/deep')] }));
t('resolve options.paths invalid', () => require.resolve('plain', { paths: 'x' }));
t('resolve.paths bare', () => collapse(require.resolve.paths('x')));
t('resolve.paths relative', () => require.resolve.paths('./x'));
t('resolve.paths builtin', () => require.resolve.paths('fs'));
t('resolve.paths non-string', () => require.resolve.paths(1));
t('require shape', () => [Object.keys(require), Object.keys(require.extensions), require.cache === require.cache, typeof require.resolve.paths, require.name, require.resolve.name]);
t('custom extension', () => {
  require.extensions['.txt'] = (m, filename) => { m.exports = require('fs').readFileSync(filename, 'utf8').trim(); };
  return require('./lib/ext/note.txt');
});
t('required module sees its own require.main', () => require('./lib/wrapper').names);
