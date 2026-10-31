const util = require('util');
const assert = require('assert');
function spy(target, log) {
  const h = {};
  for (const t of ['ownKeys', 'getOwnPropertyDescriptor']) {
    h[t] = function(...a) {
      log.push(t + ':' + (typeof a[1] === 'symbol' ? 'sym' : String(a[1])));
      return Reflect[t](...a);
    };
  }
  return new Proxy(target, h);
}
function keyed(target, keys, log) {
  return new Proxy(target, {
    ownKeys() { log.push('ownKeys'); return keys; },
    getOwnPropertyDescriptor(t, k) {
      log.push('gopd:' + String(k));
      return Reflect.getOwnPropertyDescriptor(t, k);
    },
  });
}
function show(label, fn, log) {
  let r;
  try { r = String(fn()); } catch (e) { r = 'THROW ' + e.constructor.name; }
  console.log((label + ' ' + r + ' ' + log.join(' ')).trimEnd());
}
const s = Symbol('s');
let log = [];
show('lenless', () => util.isDeepStrictEqual(
  new Proxy([1, 2, 3], { ownKeys() { return []; } }), [1, 2, 3]), log);
log = [];
show('arr', () => util.isDeepStrictEqual(spy([1, 2], log), [1, 2]), log);
log = [];
show('arr-rhs', () => util.isDeepStrictEqual([1, 2], spy([1, 2], log)), log);
log = [];
const arrx = () => Object.assign([1, 2], { x: 1, [s]: 2, 4294967295: 3, '01': 4 });
show('keys', () => util.isDeepStrictEqual(
  keyed(arrx(), ['length', s, 'x', '1', '4294967295', '01', '4294967294', '0'], log), arrx()), log);
log = [];
show('loose', () => {
  const mk = () => Object.assign([1], { x: 1, [s]: 2 });
  try { assert.deepEqual(spy(mk(), log), mk()); return true; } catch (e) { return false; }
}, log);
log = [];
show('dup', () => util.isDeepStrictEqual(keyed([1], ['length', 'length'], log), [1]), log);
log = [];
show('plain', () => assert.deepStrictEqual(spy([1, 2], log), [1, 2]) === undefined, log);
const r = Proxy.revocable([1], {});
r.revoke();
show('revoked', () => util.isDeepStrictEqual(r.proxy, [1]), []);
show('revoked-rhs', () => util.isDeepStrictEqual([1], r.proxy), []);
log = [];
show('inspect', () => util.inspect(keyed([1, 2], ['length', '0', '1'], log)), log);
