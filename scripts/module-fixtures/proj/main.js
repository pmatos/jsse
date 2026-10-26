// Differential driver for the `module` builtin (#1220): runs on jsse --node and
// on Node and prints normalised results that must be identical. Paths are shown
// relative to this directory; errors as name/code/first message line.
const path = require('path');
const ROOT = __dirname;
const m = require('module');
const show = (v) => JSON.stringify(v, (k, x) => {
  if (typeof x === 'string') return x.split(ROOT).join('<root>');
  if (typeof x === 'function') return `[function ${x.name}/${x.length}]`;
  if (x === undefined) return '[undefined]';
  return x;
});
function t(label, fn) {
  let out;
  try {
    out = { ok: fn() };
  } catch (e) {
    out = { error: [e.name, e.code, String(e.message).split('\n')[0]] };
  }
  console.log(label, show(out));
}
const desc = (o, k) => {
  const d = Object.getOwnPropertyDescriptor(o, k);
  if (d === undefined) return 'absent';
  return [d.enumerable, 'value' in d ? d.writable : 'accessor', d.configurable];
};

t('identity', () => [m === require('node:module'), m.Module === m, m.constructor === Function, module.constructor === m, require.main.constructor === m, m.name, m.length]);
const NAMES = ['isBuiltin', '_findPath', '_nodeModulePaths', '_resolveLookupPaths', '_load', '_resolveFilename',
  'createRequire', '_initPaths', '_preloadModules', 'syncBuiltinESMExports', 'runMain', 'enableCompileCache',
  'findPackageJSON', 'flushCompileCache', 'getCompileCacheDir', 'stripTypeScriptTypes', 'findSourceMap',
  'getSourceMapsSupport', 'setSourceMapsSupport', 'SourceMap', 'Module', 'builtinModules', 'constants',
  '_cache', '_pathCache', '_extensions', 'globalPaths', 'wrap', 'wrapper'];
for (const k of NAMES) {
  const v = m[k];
  t(`static ${k}`, () => [typeof v, typeof v === 'function' ? [v.name, v.length] : '', desc(m, k)]);
}
t('prototype', () => [Object.getOwnPropertyNames(m.prototype).sort(), desc(m.prototype, 'isPreloading')[2], m.prototype.isPreloading]);
t('cache identity', () => [m._cache === require.cache, m._extensions === require.extensions, Object.keys(m._extensions), Object.getPrototypeOf(m._cache)]);

t('builtinModules', () => [m.builtinModules.length, m.builtinModules.join(), Object.isFrozen(m.builtinModules), Array.isArray(m.builtinModules)]);
const ids = ['fs', 'node:fs', 'node:test', 'test', 'sea', 'node:sea', 'node:sqlite', 'sqlite', 'node:test/reporters', 'test/reporters',
  'internal/errors', 'wasi', '_http_agent', 'node:_http_agent', 'node:node:fs', 'FS', '', 'fs/', 'path/posix', 'node:path/win32', 'module', 'node:module', './fs', 'http'];
t('isBuiltin', () => ids.map((id) => m.isBuiltin(id)).join());
t('isBuiltin non-string', () => [m.isBuiltin(), m.isBuiltin(5), m.isBuiltin(null), m.isBuiltin({}), m.isBuiltin(['fs']), m.isBuiltin(new String('fs'))]);

t('wrapper', () => [m.wrapper.slice(), m.wrap('x'), m.wrap('')]);
t('wrap descriptors', () => [desc(m, 'wrap'), desc(m, 'wrapper'), Array.isArray(m.wrapper)]);
t('constants', () => [m.constants, Object.isFrozen(m.constants), Object.isFrozen(m.constants.compileCacheStatus), Object.getPrototypeOf(m.constants)]);
t('compile cache', () => [m.getCompileCacheDir(), m.flushCompileCache(), m.syncBuiltinESMExports()]);
t('enableCompileCache result', () => {
  const r = m.enableCompileCache();
  return [Object.keys(r), r.status, typeof r.message];
});
t('enableCompileCache args', () => {
  const types = [5, null, {}, { directory: 5 }].map((a) => { try { m.enableCompileCache(a); return 'ok'; } catch (e) { return e.code; } });
  return types;
});

const cr = m.createRequire;
const lib = path.join(ROOT, 'lib');
const url = (p) => require('url').pathToFileURL(p);
t('createRequire path', () => cr(path.join(lib, 'b.js'))('./a'));
t('createRequire file url string', () => cr(url(path.join(lib, 'b.js')).href)('./a.js'));
t('createRequire URL object', () => cr(url(path.join(lib, 'b.js'))).resolve('./a'));
t('createRequire trailing slash', () => cr(lib + '/').resolve('./a'));
t('createRequire bare', () => cr(path.join(lib, 'b.js'))('pkg'));
t('createRequire subpath', () => cr(path.join(lib, 'b.js'))('pkg/sub'));
t('createRequire builtin', () => cr(path.join(lib, 'b.js'))('node:path') === path);
t('createRequire module', () => cr(path.join(lib, 'b.js'))('module') === m);
for (const bad of ['rel.js', 'http://x/a', 5, undefined, null, 'file:///a%2fb', {}]) {
  t(`createRequire bad ${String(bad)}`, () => cr(bad));
}
t('createRequire shape', () => {
  const r = cr(path.join(lib, 'b.js'));
  return [typeof r, r.name, Object.keys(r), r.main === process.mainModule, r.cache === m._cache, r.extensions === m._extensions,
    r.resolve.paths('x').slice(0, 2), r.resolve.paths('./x'), r.resolve.paths('fs')];
});
t('createRequire missing', () => cr(path.join(lib, 'b.js'))('./nope'));
t('createRequire child of fake parent', () => {
  const r = cr(path.join(lib, 'b.js'));
  r('./a');
  const fake = m._cache[path.join(lib, 'a.js')];
  return [fake.parent && fake.parent.id, fake.parent && fake.parent.filename, fake.parent && fake.parent.paths.length > 0];
});

const f = m.findPackageJSON;
const base = path.join(lib, 'b.js');
const fcases = [
  ['relative', () => f('./a.js', base)], ['file url base', () => f('./a.js', url(base).href)], ['URL base', () => f('./a.js', url(base))],
  ['dir base', () => f('./a.js', lib + '/')], ['bare', () => f('pkg', base)], ['bare subpath', () => f('pkg/sub', base)],
  ['bare package.json', () => f('pkg/package.json', base)], ['bare missing subpath', () => f('pkg/zzz', base)],
  ['bare missing', () => f('nopkg', base)], ['scoped missing', () => f('@s/x', base)], ['missing file', () => f('./nofile.js', base)],
  ['absolute specifier', () => f(path.join(lib, 'a.js'), path.join(ROOT, 'x.js'))], ['file url specifier', () => f(url(path.join(lib, 'a.js')).href)],
  ['no base', () => f('./a.js')], ['builtin bare', () => f('fs', base)], ['builtin url', () => f('node:fs', base)],
  ['no args', () => f()], ['number specifier', () => f(5, base)], ['number base', () => f('./a.js', 5)],
  ['relative base', () => f('./a.js', 'rel/b.js')], ['parent dir', () => f('..', base)], ['dot', () => f('.', base)],
  ['into node_modules', () => f('../node_modules/pkg/sub/x.js', base)], ['http base', () => f('./a.js', 'http://x/y')],
  ['http specifier', () => f('http://x/y', base)], ['directory', () => f(lib, path.join(ROOT, 'x.js'))],
  ['length', () => f.length],
];
for (const [label, fn] of fcases) t(`findPackageJSON ${label}`, fn);

// Hooks that tools patch.
const origExt = m._extensions['.js'];
const loaded = [];
m._extensions['.js'] = function(module, filename) {
  loaded.push(path.relative(ROOT, filename));
  return origExt(module, filename);
};
require('./lib/uses_pkg.js');
m._extensions['.js'] = origExt;
t('patched .js extension', () => loaded);

m._extensions['.txt'] = function(module, filename) {
  module.exports = require('fs').readFileSync(filename, 'utf8').trim();
};
t('custom .txt extension', () => [require('./lib/hello.txt'), Object.keys(m._extensions)]);
t('require.resolve custom ext', () => path.relative(ROOT, require.resolve('./lib/hello')));
delete m._extensions['.txt'];

const origResolve = m._resolveFilename;
m._resolveFilename = function(request, parent, isMain, options) {
  if (request === 'aliased') request = path.join(ROOT, 'lib/alias_target.js');
  return origResolve.call(this, request, parent, isMain, options);
};
t('patched _resolveFilename', () => [require('aliased'), path.relative(ROOT, require.resolve('aliased'))]);
m._resolveFilename = origResolve;
t('restored _resolveFilename', () => require('aliased'));

const origLoad = m._load;
const requests = [];
m._load = function(request, parent, isMain) {
  requests.push(request);
  if (request === 'virtual') return { virtual: true };
  return origLoad.apply(this, arguments);
};
t('patched _load', () => [require('virtual'), require('./lib/a'), require('fs') === require('node:fs'), requests]);
m._load = origLoad;

delete m._cache[path.join(ROOT, 'node_modules/pkg/sub/x.js')];
const origCompile = m.prototype._compile;
const compiled = [];
m.prototype._compile = function(content, filename) {
  compiled.push(path.relative(ROOT, filename));
  return origCompile.call(this, content.replace('"pkg/sub"', '"patched"'), filename);
};
t('patched _compile', () => [require('pkg/sub'), compiled]);
m.prototype._compile = origCompile;

t('_nodeModulePaths', () => [m._nodeModulePaths(path.join(ROOT, 'lib')).slice(0, 2), m._nodeModulePaths('/'), m._nodeModulePaths('/a/node_modules/b')]);
t('_resolveLookupPaths', () => [m._resolveLookupPaths('fs'), m._resolveLookupPaths('./x', { id: 'i', filename: base }), m._resolveLookupPaths('x', { paths: ['/p'] }).slice(0, 1)]);
t('_findPath', () => [m._findPath('pkg', [path.join(ROOT, 'node_modules')]), m._findPath('./nope', [lib]), m._findPath('a', [lib])]);
t('_initPaths', () => { const before = m.globalPaths.slice(); m._initPaths(); return [Array.isArray(m.globalPaths), before.length === m.globalPaths.length]; });

// Module.wrap is consulted only once patched.
t('patched wrap', () => {
  const before = require('./lib/wrapped.js');
  delete m._cache[path.join(lib, 'wrapped.js')];
  const orig = m.wrap;
  m.wrap = (script) => m.wrapper[0] + 'var injected = 42;' + script + m.wrapper[1];
  const after = require('./lib/wrapped.js');
  delete m._cache[path.join(lib, 'wrapped.js')];
  m.wrap = orig;
  return [before, after, m.wrap === orig];
});
t('patched wrapper', () => {
  delete m._cache[path.join(lib, 'wrapped.js')];
  m.wrapper[0] += 'var injected = 7;';
  const r = require('./lib/wrapped.js');
  m.wrapper[0] = m.wrapper[0].replace('var injected = 7;', '');
  return [r, m.wrap('x')];
});

t('_preloadModules', () => {
  globalThis.__preloaded = [];
  m._preloadModules(['./lib/a.js']);
  m._preloadModules(5);
  m._preloadModules([]);
  return [m._cache[path.join(lib, 'a.js')].parent.id, m._cache[path.join(lib, 'a.js')].parent.filename, m.prototype.isPreloading];
});
t('isPreloading while preloading', () => {
  delete m._cache[path.join(lib, 'preload_flag.js')];
  m._preloadModules(['./lib/preload_flag.js']);
  return [m._cache[path.join(lib, 'preload_flag.js')].exports, m.prototype.isPreloading];
});
t('runMain', () => [typeof m.runMain, m.runMain.length, m.runMain.name]);

import('module').then((ns) => {
  t('esm import', () => [ns.default === m, typeof ns.createRequire, ns.builtinModules === m.builtinModules, typeof ns.isBuiltin]);
  return import('node:module');
}).then((ns) => t('esm import node:', () => ns.default === m));
