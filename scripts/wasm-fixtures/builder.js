// A minimal WebAssembly binary encoder for the WebAssembly differential corpus (issue #1378).
// The fixtures build their modules in JS so that nothing binary is checked in.
"use strict";

const T = { i32: 0x7f, i64: 0x7e, f32: 0x7d, f64: 0x7c, funcref: 0x70, externref: 0x6f };
const KIND = { func: 0, table: 1, memory: 2, global: 3 };

function uleb(n) {
  const out = [];
  do {
    let b = n & 0x7f;
    n >>>= 7;
    if (n !== 0) b |= 0x80;
    out.push(b);
  } while (n !== 0);
  return out;
}
function sleb(n) {
  n = BigInt(n);
  const out = [];
  for (;;) {
    const b = Number(n & 0x7fn);
    n >>= 7n;
    if ((n === 0n && !(b & 0x40)) || (n === -1n && b & 0x40)) {
      out.push(b);
      return out;
    }
    out.push(b | 0x80);
  }
}
const str = (s) => {
  const b = Array.from(new TextEncoder().encode(s));
  return [...uleb(b.length), ...b];
};
const vec = (items) => [...uleb(items.length), ...items.flat()];
const section = (id, body) => [id, ...uleb(body.length), ...body];
const limits = (min, max) => (max === undefined ? [0, ...uleb(min)] : [1, ...uleb(min), ...uleb(max)]);

// Opcodes used by the corpus.
const op = {
  unreachable: [0x00],
  block: (t = 0x40) => [0x02, t],
  loop: (t = 0x40) => [0x03, t],
  if_: (t = 0x40) => [0x04, t],
  else_: [0x05],
  end: [0x0b],
  br_if: (d) => [0x0d, ...uleb(d)],
  return_: [0x0f],
  call: (i) => [0x10, ...uleb(i)],
  call_indirect: (type, table = 0) => [0x11, ...uleb(type), ...uleb(table)],
  drop: [0x1a],
  local_get: (i) => [0x20, ...uleb(i)],
  local_set: (i) => [0x21, ...uleb(i)],
  global_get: (i) => [0x23, ...uleb(i)],
  global_set: (i) => [0x24, ...uleb(i)],
  table_get: (t = 0) => [0x25, ...uleb(t)],
  table_set: (t = 0) => [0x26, ...uleb(t)],
  i32_load: (off = 0) => [0x28, 2, ...uleb(off)],
  i32_load8_u: (off = 0) => [0x2d, 0, ...uleb(off)],
  i32_store: (off = 0) => [0x36, 2, ...uleb(off)],
  i32_store8: (off = 0) => [0x3a, 0, ...uleb(off)],
  memory_size: [0x3f, 0],
  memory_grow: [0x40, 0],
  i32_const: (n) => [0x41, ...sleb(n)],
  i64_const: (n) => [0x42, ...sleb(n)],
  f32_const: (x) => [0x43, ...new Uint8Array(new Float32Array([x]).buffer)],
  f64_const: (x) => [0x44, ...new Uint8Array(new Float64Array([x]).buffer)],
  ref_null: (t) => [0xd0, t],
  ref_func: (i) => [0xd2, ...uleb(i)],
  i32_eqz: [0x45],
  i32_eq: [0x46],
  i32_lt_s: [0x48],
  i32_add: [0x6a],
  i32_sub: [0x6b],
  i32_mul: [0x6c],
  i32_div_s: [0x6d],
  i64_add: [0x7c],
  i64_mul: [0x7e],
  f32_add: [0x92],
  f32_mul: [0x94],
  f64_add: [0xa0],
  f64_mul: [0xa2],
  i64_extend_i32_u: [0xad],
};

// spec: { types: [[params, results]], imports: [{module, name, kind, type|desc}], funcs: [typeIdx],
// tables: [[elemType, min, max]], mems: [[min, max]], globals: [{type, mutable, init}],
// exports: [[name, kind, index]], start, elems: [{offset, funcs}], codes: [{locals, body}],
// datas: [{offset, bytes}], customs: [[name, bytes]] }
function build(spec) {
  const out = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
  const add = (id, body) => out.push(...section(id, body));
  const s = spec;
  if (s.types) add(1, vec(s.types.map(([p, r]) => [0x60, ...vec(p.map((t) => [T[t]])), ...vec(r.map((t) => [T[t]]))])));
  if (s.imports) {
    add(2, vec(s.imports.map((i) => {
      const head = [...str(i.module), ...str(i.name), KIND[i.kind]];
      if (i.kind === "func") return [...head, ...uleb(i.type)];
      if (i.kind === "table") return [...head, T[i.elem || "funcref"], ...limits(...i.limits)];
      if (i.kind === "memory") return [...head, ...limits(...i.limits)];
      return [...head, T[i.type], i.mutable ? 1 : 0];
    })));
  }
  if (s.funcs) add(3, vec(s.funcs.map((t) => uleb(t))));
  if (s.tables) add(4, vec(s.tables.map(([e, min, max]) => [T[e], ...limits(min, max)])));
  if (s.mems) add(5, vec(s.mems.map(([min, max]) => limits(min, max))));
  if (s.globals) add(6, vec(s.globals.map((g) => [T[g.type], g.mutable ? 1 : 0, ...g.init, ...op.end])));
  if (s.exports) add(7, vec(s.exports.map(([n, k, i]) => [...str(n), KIND[k], ...uleb(i)])));
  if (s.start !== undefined) add(8, uleb(s.start));
  if (s.elems) add(9, vec(s.elems.map((e) => [0, ...op.i32_const(e.offset), ...op.end, ...vec(e.funcs.map((f) => uleb(f)))])));
  if (s.codes) {
    add(10, vec(s.codes.map((c) => {
      const body = [...vec((c.locals || []).map((t) => [1, T[t]])), ...c.body.flat(), ...op.end];
      return [...uleb(body.length), ...body];
    })));
  }
  if (s.datas) add(11, vec(s.datas.map((d) => [0, ...op.i32_const(d.offset), ...op.end, ...uleb(d.bytes.length), ...d.bytes])));
  for (const [name, bytes] of s.customs || []) add(0, [...str(name), ...bytes]);
  return new Uint8Array(out);
}

module.exports = { build, op, T, uleb, sleb };
