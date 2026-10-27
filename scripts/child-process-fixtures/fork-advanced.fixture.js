// RUN: 0
// serialization: 'advanced' carries the structured-clone value set. jsse does
// not write V8's wire format, so the cross-engine passes skip this fixture.
// SAME-ENGINE
const cp = require('child_process');
const path = require('path');
const util = require('util');
const { show, forkOpts } = require('./lib');

const child = path.join(__dirname, 'fork-child.js');
const inspect = (v) => util.inspect(v, { depth: 6, breakLength: Infinity });
const hex = (view) => Buffer.from(view.buffer, view.byteOffset, view.byteLength).toString('hex');

function exchange(mode, message) {
  const c = cp.fork(child, [mode], forkOpts({ serialization: 'advanced' }));
  return new Promise((resolve, reject) => {
    c.once('message', (m) => { c.send({ cmd: 'quit' }); resolve(m); });
    c.once('exit', () => reject(new Error('child exited')));
    try { c.send(message); } catch (e) { c.kill(); reject(e); }
  });
}

async function main() {
  const cyc = { name: 'c', list: [] };
  cyc.self = cyc;
  cyc.list.push(cyc, cyc.list);
  const sparse = [1, , 3];
  sparse.extra = 'e';
  const shared = { s: 1 };
  const values = [
    undefined, null, true, 'str\n ', 0, -0, NaN, Infinity, -Infinity, 1.5e300, 12345678901234567890n, -5n,
    new Date(86400000), new Date(NaN), /a+b/gi, [], {}, { a: { b: [1, 'x', null, undefined] } },
    new Map([[1, { a: 1 }], ['k', new Set([1, 2n])], [{ o: 1 }, [1]]]), new Set(['a', 1, { z: 1 }]),
    cyc, sparse, [shared, shared], [Object(1), Object('s'), Object(true), Object(2n)],
    { 'k y': 1, 2: 'two', [Symbol('dropped')]: 1 },
  ];
  for (const v of values) {
    const m = (await exchange('echo', { v })).v;
    show('echo', [inspect(v) === inspect(m), inspect(m)]);
  }
  show('inspect in child', await exchange('inspect', { n: [1, 2n, new Map([[1, 2]])], d: new Date(0) }));

  const ab = new Uint8Array([1, 2, 3, 4]).buffer;
  const binary = await exchange('echo', {
    buf: Buffer.from('hi'), u8: new Uint8Array([1, 2, 3]), i16: new Int16Array([-2, 300]), f64: new Float64Array([1.5, -0]),
    big: new BigInt64Array([-1n, 2n]), dv: new DataView(ab, 1, 2), ab, sliced: new Uint8Array(ab, 2),
  });
  show('binary', {
    buf: [binary.buf.constructor.name, hex(binary.buf)], u8: [binary.u8.constructor.name, hex(binary.u8)],
    i16: [binary.i16.constructor.name, Array.from(binary.i16)], f64: Array.from(binary.f64, (x) => Object.is(x, -0) ? '-0' : x),
    big: Array.from(binary.big, String), dv: [binary.dv.constructor.name, hex(binary.dv)],
    ab: [binary.ab.constructor.name, binary.ab.byteLength, hex(new Uint8Array(binary.ab))],
    sliced: [binary.sliced.constructor.name, hex(binary.sliced)],
  });

  const e = await exchange('echo', { err: new RangeError('boom', { cause: 'why' }), type: new TypeError('t') });
  show('errors', [e.err instanceof RangeError, e.err.name, e.err.message, e.err.cause, typeof e.err.stack,
    e.type instanceof TypeError, e.type.message]);
  const same = await exchange('echo', [shared, shared]);
  show('shared identity', same[0] === same[1]);
  const proto = (await exchange('echo', JSON.parse('{"__proto__":{"polluted":1},"a":1}')));
  show('own __proto__ key', [Object.keys(proto), Object.getPrototypeOf(proto) === Object.prototype, proto.polluted]);

  for (const [label, v] of [['function', { f() {} }], ['nested symbol', { s: Symbol('s') }],
    ['weakmap', { w: new WeakMap() }], ['promise', { p: Promise.resolve(1) }]]) {
    try { await exchange('echo', v); show(label, 'sent'); } catch (err) { show(label, [err.name === 'Error' || err.name === 'DataCloneError', /could not be cloned/.test(err.message)]); }
  }
}

main();
