// Differential driver for the ES module loader (#1240): runs on jsse --node and
// on Node, prints normalised results, and the outputs must be identical. Paths
// are shown relative to this directory; errors are shown as data.
import './src/log.js';
import fs, { readFileSync as readFile } from 'node:fs';
import * as pathNs from 'path';
import path from 'node:path';
import { EventEmitter } from 'events';
import util from 'node:util';
import data from './src/data.json' with { type: 'json' };
import named, { x, counter, bump } from './src/util.js';
import * as re from './src/reexport.js';
import legacyFn from './legacy/b.js';
import { which } from '#cond';
import { where } from '#int';
import { star } from '#star/a';
import dual from 'dual';
import dualSub from '#dep';
import selfref from 'selfref';
import scoped from '@scope/pkg';
import { feature } from '@scope/pkg/feature';
import impUse from 'imp-pkg';
import esmOnly, { extra } from 'esm-only';
import legacyMain from 'legacy-main';
import { self as selfExport } from 'esm-proj/self';
import condorder from 'condorder';
import pkgJson from 'dual/package.json' with { type: 'json' };
import pkgData from 'json-pkg/data.json' with { type: 'json' };
import plainCjs from './plain/cjs.js';
import { typeless } from './plain/esm.js';
import cjsInEsmPackage from './src/pkgtype-cjs.cjs';
import * as cycleA from './src/cycle-a.js';
import './src/order.js';

const ROOT = path.dirname(process.argv[1]);

function norm(value) {
  return JSON.parse(JSON.stringify(value, (k, v) => {
    if (typeof v === 'string') return v.split('file://' + ROOT).join('<root-url>').split(ROOT).join('<root>');
    if (typeof v === 'function') return '[function]';
    if (v === undefined) return '[undefined]';
    return v;
  }));
}

function show(label, value) {
  console.log(label, JSON.stringify(norm(value)));
}

async function t(label, fn) {
  let out;
  try {
    out = { ok: await fn() };
  } catch (e) {
    out = { error: { name: e.name, code: e.code, message: e.message.split('\n')[0] } };
  }
  show(label, out);
}

const keys = (ns) => Object.keys(ns);
const imp = (spec, opts) => () => import(spec, opts);
const dyn = (spec, opts) => async () => {
  const ns = await import(spec, opts);
  return { keys: keys(ns), default: ns.default };
};

show('log-after-static-graph', globalThis.log);
show('builtins', {
  fsDefault: typeof fs.readFileSync, fsNamed: readFile === fs.readFileSync,
  pathNsHasJoin: keys(pathNs).includes('join'), pathNsDefault: pathNs.default === path, pathJoin: pathNs.join === path.join,
  emitter: typeof EventEmitter, util: typeof util.format, tag: Object.prototype.toString.call(pathNs),
});
show('json', { data, same: data === (await import('./src/data.json', { with: { type: 'json' } })).default });
show('es', { named: named(), x, counter, ns: keys(re), nsutil: keys(re.nsutil), y: re.y, util: re.util === named });
bump();
show('live-binding', { counter, viaNs: re.counter });
show('cycle', { a: cycleA.a, b: cycleA.seenFromB() });
cycleA.setA('a1');
show('cycle-live', { a: cycleA.a });
show('cjs-default', { legacyFn: legacyFn(), plainCjs, typeless, cjsInEsmPackage });
show('imports', { which, where, star, dualSub, impUse });
show('packages', { dual, selfref, scoped, feature, esmOnly, extra, legacyMain, selfExport, condorder, pkgName: pkgJson.name, pkgData });

await t('ns-cjs-default', async () => Object.keys((await import('./legacy/a.cjs')).default));
await t('ns-cjs-same-as-static', async () => (await import('./legacy/a.cjs')).default === (await import('./legacy/a.cjs')).default);
await t('ns-esm-shape', dyn('./src/dyn.js'));
await t('ns-builtin-has-default', async () => { const ns = await import('node:os'); return typeof ns.default.EOL + typeof ns.EOL; });
await t('ns-frozen', async () => { const ns = await import('./src/dyn.js'); return [Object.isFrozen(ns), Object.isSealed(ns), Reflect.set(ns, 'named', 1)]; });
await t('dyn-url-file', imp('file://' + ROOT + '/src/dyn.js'));
await t('dyn-encoded', async () => { const ns = await import('./src/with%20space.mjs'); return ns.spaced; });
await t('dyn-query-same-file', async () => keys(await import('./src/dyn.js?x=1')));
await t('dyn-deep-relative', async () => (await import('./sub/deep/leaf.mjs')).seen);
await t('dyn-parent-relative', async () => (await import('../proj/src/dyn.js')).named);
await t('dyn-absolute', async () => (await import(ROOT + '/src/dyn.js')).named);

const legacyA = (await import('./legacy/a.cjs')).default;
await t('cjs-dynamic-sibling', async () => (await legacyA.dynamicSibling()).fromLegacyDir);
await t('cjs-dynamic-later', async () => (await legacyA.dynamicLater()).fromLegacyDir);
await t('cjs-dynamic-esm-above', async () => (await legacyA.dynamicEsmAbove()).named);
await t('cjs-exports-identity', async () => legacyA.self());
await t('typeless-cjs-dynamic', async () => (await (await import('./plain/dyn-caller.js')).default.go()).default);
await t('esm-scope-dynamic', async () => keys(await (await import('./src/scope-a.js')).loadSibling()));
await t('importer-relative-to-importer', async () => { const m = await import('./src/importer.js'); return keys(await m.load('./util.js')); });

await t('err-not-found', imp('./src/nope.js'));
await t('err-missing-extension', imp('./src/util'));
await t('err-directory', imp('./sub'));
await t('err-package-missing', imp('nope-pkg'));
await t('err-package-subpath', imp('dual/missing'));
await t('err-package-hidden', imp('dual/hidden'));
await t('err-package-no-main-export', imp('json-pkg'));
await t('err-bad-target', imp('badtarget'));
await t('ok-bad-target-other', dyn('badtarget/ok'));
await t('err-imports-undefined', imp('#nope'));
await t('err-imports-bad-target', imp('#bad'));
await t('err-invalid-package-name', imp('.hidden/x'));
await t('err-scheme-http', imp('http://example.com/x.js'));
await t('err-scheme-unknown', imp('foo:bar'));
await t('err-unknown-builtin', imp('node:nope'));
await t('err-unknown-extension', imp('./src/unknown.txt'));
await t('err-json-missing-attr', imp('./src/data.json'));
await t('err-json-wrong-type', imp('./src/util.js', { with: { type: 'json' } }));
await t('err-json-builtin', imp('node:fs', { with: { type: 'json' } }));
await t('err-throws-at-eval', imp('./src/thrower.js'));
await t('err-throws-twice', async () => { try { await import('./src/thrower.js'); } catch {} return await import('./src/thrower.js'); });
await t('err-tla-reject', imp('./src/tla-reject.js'));
await t('err-bad-options', imp('./src/dyn.js', 3));
await t('ok-pkg-conditions-dynamic', dyn('dual'));
await t('ok-pkg-pattern', async () => (await import('dual/sub/x')).default);
await t('ok-json-dynamic', dyn('./src/data.json', { with: { type: 'json' } }));
await t('ok-builtin-prefixless', async () => keys(await import('fs')).includes('readFileSync'));
await t('ok-builtin-subpath', async () => typeof (await import('node:path/posix')).default.join);

show('log-end', globalThis.log);
