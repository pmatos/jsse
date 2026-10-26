// Differential driver for the source map half of the `module` builtin (#1220):
// `SourceMap`, `findSourceMap`, `get/setSourceMapsSupport` and
// `process.setSourceMapsEnabled`, run on jsse --node and on Node.
const path = require('path');
const m = require('module');
const ROOT = path.join(__dirname, 'sm');
const show = (v) => JSON.stringify(v, (k, x) => (typeof x === 'string' ? x.split(ROOT).join('<root>') : x === undefined ? '[undefined]' : x));
function t(label, fn) {
  let out;
  try {
    out = { ok: fn() };
  } catch (e) {
    out = { error: [e.name, e.code, String(e.message).split('\n')[0]] };
  }
  console.log(label, show(out));
}
const MAP = { version: 3, file: 'a.js', sources: ['a.ts'], names: ['foo', 'bar'], mappings: 'AAAA,SAASA,GAAG;AACZ,IAAIC', sourceRoot: '/r' };
t('SourceMap shape', () => [m.SourceMap.name, m.SourceMap.length, Object.getOwnPropertyNames(m.SourceMap.prototype)]);
for (const bad of [undefined, {}, 5, null, { version: 3 }]) t(`new SourceMap(${JSON.stringify(bad)})`, () => new m.SourceMap(bad));
t('empty payload', () => new m.SourceMap({ version: 3, sources: [], mappings: '' }).payload);
t('payload', () => { const p = new m.SourceMap(MAP).payload; return [p, Object.getPrototypeOf(p), p.sources !== MAP.sources]; });
const sm = new m.SourceMap(MAP);
for (const [l, c] of [[0, 0], [0, 5], [0, 9], [0, 100], [1, 0], [1, 6], [2, 0], [-1, 0], [0, -1], [undefined, undefined], ['a', 'b']]) t(`findEntry(${l},${c})`, () => sm.findEntry(l, c));
for (const [l, c] of [[1, 1], [1, 10], [2, 1], [2, 6], [3, 1], [0, 0], ['a', 'b']]) t(`findOrigin(${l},${c})`, () => sm.findOrigin(l, c));
t('lineLengths', () => [sm.lineLengths, new m.SourceMap(MAP, { lineLengths: [3, 4] }).lineLengths, new m.SourceMap(MAP, { lineLengths: [] }).lineLengths, new m.SourceMap(MAP, { lineLengths: 5 }).lineLengths]);
t('sections', () => {
  const s = new m.SourceMap({ version: 3, sections: [
    { offset: { line: 0, column: 0 }, map: { version: 3, sources: ['q.ts'], names: [], mappings: 'AAAA' } },
    { offset: { line: 2, column: 4 }, map: { version: 3, sources: ['r.ts'], names: ['n'], mappings: 'AAAAA' } }] });
  return [s.findEntry(2, 4), s.findEntry(1, 0), s.findOrigin(3, 5), s.payload];
});
const mk = (mappings, extra) => new m.SourceMap({ version: 3, sources: ['a.ts', 'b.ts'], names: ['n0', 'n1'], mappings, ...extra });
t('multi source', () => { const s = mk('AAAA,CCCC;ACAA,EAEEA,CAAC'); return [s.findEntry(0, 1), s.findEntry(1, 0), s.findEntry(1, 3), s.findEntry(1, 5)]; });
t('segment of one field', () => { const s = mk('A,CAAC'); return [s.findEntry(0, 0), s.findOrigin(1, 1), s.findOrigin(1, 2)]; });
t('invalid vlq', () => mk('AAAA,!!').findOrigin(1, 1));
t('skipped lines', () => { const s = mk('AAAA;;;AACA'); return [s.findEntry(3, 0), s.findOrigin(4, 1), s.findOrigin(2, 1)]; });
t('absolute source and root', () => new m.SourceMap({ version: 3, sources: ['http://x/a.ts'], names: [], mappings: 'AAAA', sourceRoot: 'http://r/' }).findOrigin(1, 1));
t('negative deltas', () => { const s = mk('KAAK,GAAG;CAAC'); return [s.findEntry(0, 5), s.findEntry(1, 1)]; });
t('big vlq', () => { const s = mk('gBAAgB'); return [s.findEntry(0, 16), s.findEntry(0, 15)]; });

t('support initial', () => [m.getSourceMapsSupport(), process.sourceMapsEnabled, Object.getPrototypeOf(m.getSourceMapsSupport())]);
t('process descriptors', () => ['sourceMapsEnabled', 'setSourceMapsEnabled'].map((k) => {
  const d = Object.getOwnPropertyDescriptor(process, k);
  return [d.enumerable, d.configurable, 'get' in d, d.writable, typeof (d.get || d.value), (d.get || d.value).name, (d.get || d.value).length];
}));
const calls = [[true], [false], [1], [], [true, { nodeModules: true }], [true, { nodeModules: 1 }], [true, 5], [true, { generatedCode: true }], [false, { nodeModules: true }], [true, null], [true, undefined], [true, {}]];
for (const a of calls) {
  t(`setSourceMapsSupport(${a.map((x) => JSON.stringify(x)).join(',')})`, () => (m.setSourceMapsSupport(...a), m.getSourceMapsSupport()));
  t('  process.sourceMapsEnabled', () => process.sourceMapsEnabled);
}
t('setSourceMapsEnabled(true)', () => [process.setSourceMapsEnabled(true), m.getSourceMapsSupport()]);
t('setSourceMapsEnabled(1)', () => process.setSourceMapsEnabled(1));
t('setSourceMapsEnabled()', () => process.setSourceMapsEnabled());
t('setSourceMapsEnabled(false)', () => [process.setSourceMapsEnabled(false), process.sourceMapsEnabled, m.getSourceMapsSupport()]);

const d = (s) => (s === undefined ? undefined : { cls: s.constructor.name, payload: s.payload, lineLengths: s.lineLengths, e: s.findEntry(0, 9), o: s.findOrigin(2, 1) });
require('./sm/before.js');
t('disabled', () => m.findSourceMap(path.join(ROOT, 'before.js')));
process.setSourceMapsEnabled(true);
t('loaded before enabling', () => d(m.findSourceMap(path.join(ROOT, 'before.js'))));
for (const f of ['gen', 'inline', 'inlineraw', 'missing', 'badmap', 'old', 'none', 'two', 'block']) {
  require(`./sm/${f}.js`);
  t(`findSourceMap ${f}`, () => d(m.findSourceMap(path.join(ROOT, `${f}.js`))));
}
t('file url', () => d(m.findSourceMap(require('url').pathToFileURL(path.join(ROOT, 'gen.js')).href)));
t('same object', () => m.findSourceMap(path.join(ROOT, 'gen.js')) === m.findSourceMap(path.join(ROOT, 'gen.js')));
t('not loaded', () => d(m.findSourceMap(path.join(ROOT, 'gen.js.map'))));
t('non-string', () => [m.findSourceMap(), m.findSourceMap(5), m.findSourceMap({})]);
require('./sm/node_modules/dep');
t('node_modules (nodeModules on)', () => d(m.findSourceMap(path.join(ROOT, 'node_modules/dep/index.js'))));
process.setSourceMapsEnabled(false);
t('answers after disabling', () => d(m.findSourceMap(path.join(ROOT, 'gen.js'))));
require('./sm/late.js');
t('loaded while disabled', () => d(m.findSourceMap(path.join(ROOT, 'late.js'))));
m.setSourceMapsSupport(true);
require('./sm/node_modules/dep2');
t('node_modules (nodeModules off)', () => d(m.findSourceMap(path.join(ROOT, 'node_modules/dep2/index.js'))));
m.setSourceMapsSupport(false);
