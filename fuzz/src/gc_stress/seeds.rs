//! Seed corpus for the `gc_stress` target, defined as (name, period index,
//! engine, source) and committed as `fuzz/corpus/gc_stress/NN-<name>.bin`.
//! Regenerate with
//! `cargo test --manifest-path fuzz/Cargo.toml --lib -- --ignored regenerate_seed_corpus`.
//!
//! `JSON.parse` with a reviver is deliberately absent: it trips the oracle
//! (pmatos/jsse#1198); add it back once that is fixed.
//!
//! Most seeds re-enter JS from native code while holding freshly allocated
//! values: safepoints only occur in JS, so that is where an unrooted value is
//! exposed to a forced collection.

use super::{PERIODS, decode, encode, step_budget};
use jsse::SigOutcome;
use std::collections::HashSet;
use std::path::PathBuf;

struct Seed {
    name: &'static str,
    period_index: u8,
    bytecode: bool,
    src: &'static str,
}

const fn seed(name: &'static str, period_index: u8, bytecode: bool, src: &'static str) -> Seed {
    Seed {
        name,
        period_index,
        bytecode,
        src,
    }
}

fn seeds() -> Vec<Seed> {
    vec![
        seed(
            "linked-list-and-tree",
            0,
            true,
            r#"var head = null;
for (var i = 0; i < 40; i++) head = { v: i, next: head, tag: 's' + i };
function tree(d) { return d ? { l: tree(d - 1), r: tree(d - 1), d: [d] } : { leaf: [d] }; }
var t = tree(5);
var n = 0;
for (var p = head; p; p = p.next) n += p.v;
n + t.l.r.d[0];"#,
        ),
        seed(
            "closures-capture-fresh-objects",
            1,
            true,
            r#"var fns = [];
for (let i = 0; i < 40; i++) { let o = { i: i, a: [i] }; fns.push(() => o.a[0] + o.i); }
var sum = 0;
for (var f of fns) sum += f();
sum;"#,
        ),
        seed(
            "growth-push-spread-concat",
            2,
            false,
            r#"var a = [];
for (var i = 0; i < 40; i++) { a.push({ k: i }); a = [...a, 'x' + i]; a = a.concat([i]); }
var s = '';
for (var j = 0; j < 40; j++) s += j + ',';
a.length + s.length;"#,
        ),
        seed(
            "map-set-weak-collections-churn",
            3,
            true,
            r#"var m = new Map(), s = new Set(), wm = new WeakMap(), ws = new WeakSet();
for (var i = 0; i < 60; i++) {
  var k = { i: i };
  m.set(k, [i]); s.add('v' + i); wm.set(k, { v: i }); ws.add(k);
  if (i % 3 == 0) { m.delete(k); s.delete('v' + i); }
}
var n = 0;
m.forEach(function (v, key) { n += v[0] + wm.get(key).v; });
for (var x of s) n += x.length;
n;"#,
        ),
        seed(
            "native-reentry-array-callbacks",
            0,
            true,
            r#"var out = [3, 1, 2, 5, 4].map(function (x) { return { x: x, pad: [x] }; });
out.sort(function (a, b) { var t = { a: a, b: b, z: [a.x] }; return a.x - b.x + t.z[0] - t.z[0]; });
var r = out.reduce(function (acc, o) { return { sum: acc.sum + o.x, last: [o] }; }, { sum: 0 });
var f = Array.from({ length: 6 }, function (_, i) { return { i: i, s: String(i) }; });
var fm = f.filter(function (o) { return { keep: o.i % 2 }.keep; });
r.sum + f.length + fm.length;"#,
        ),
        seed(
            "native-reentry-string-json",
            1,
            true,
            r#"var s = 'a-b-c-d-e-f'.replace(/-/g, function (m) { return ['x', { y: m }][0] + { z: m }.z; });
var j = JSON.parse('{"a":[1,2,{"b":3}],"c":{"d":[4,5]}}');
var t = JSON.stringify({ a: { toJSON: function () { return { q: [1, 2, 3] }; } }, b: [{ toJSON: function () { return 'z' + [1].length; } }] });
s + t + j.c.d[0];"#,
        ),
        seed(
            "accessors-and-coercion-allocate",
            2,
            false,
            r#"var o = {
  get g() { return { a: [1, 2] }; },
  set g(v) { this._s = { v: v, w: [v] }; },
  valueOf: function () { return { n: 1 }.n + [2].length; },
  toString: function () { return 'o' + { a: 1 }.a; }
};
var n = 0;
for (var i = 0; i < 30; i++) { o.g = { i: i }; n += o.g.a.length + (o + 1) + `${o}`.length + o._s.w.length; }
n;"#,
        ),
        seed(
            "proxy-and-reflect-reentry",
            3,
            true,
            r#"var log = [];
var p = new Proxy({ a: 1 }, {
  get: function (t, k, r) { log.push({ k: String(k) }); return { v: t[k] }; },
  has: function (t, k) { return { r: k in t }.r; },
  ownKeys: function (t) { return ['a', 'b' + [1].length]; },
  getOwnPropertyDescriptor: function (t, k) { return { value: { k: k }, configurable: true, enumerable: true }; }
});
var n = 0;
for (var i = 0; i < 20; i++) { n += p.a.v + ('a' in p ? 1 : 0) + Object.keys(p).length; }
n += Reflect.apply(function (a, b) { return { a: a, b: [b] }.b[0]; }, null, [{ x: 1 }, 2]);
n + log.length;"#,
        ),
        seed(
            "classes-super-private-fields",
            1,
            true,
            r#"class A { #p = { a: [1] }; static s = [{ z: 1 }]; constructor(x) { this.x = { x: x }; } get p() { return this.#p; } m() { return { m: [this.x.x] }; } }
class B extends A { #q = [{ q: 2 }]; constructor(x) { super({ w: x }); this.y = [x]; } m() { return { b: super.m(), q: this.#q }; } }
var n = 0;
for (var i = 0; i < 30; i++) { var b = new B(i); n += b.m().b.m[0].w + b.p.a[0] + B.s[0].z; }
n;"#,
        ),
        seed(
            "generators-async-promises",
            0,
            true,
            r#"function* gen(n) { for (var i = 0; i < n; i++) { var o = yield { i: i, a: [i] }; if (o) { o.seen = [i]; } } return { done: [n] }; }
var g = gen(10), total = 0, r;
while (!(r = g.next({})).done) total += r.value.i;
var log = [];
async function f(n) { var s = 0; for (var i = 0; i < n; i++) { s += (await { then: function (res) { res({ v: [i] }); } }).v[0]; } return { s: s }; }
f(8).then(function (v) { log.push(v.s); return Promise.all([1, 2, 3].map(function (x) { return Promise.resolve({ x: x }); })); }).then(function (a) { log.push(a.length); });
async function* ag() { for (var i = 0; i < 5; i++) yield { i: [i] }; }
(async function () { for await (var x of ag()) log.push(x.i[0]); })();
total + r.value.done[0];"#,
        ),
        seed(
            "destructuring-spread-tagged-templates",
            2,
            false,
            r#"function tag(s, ...v) { return { s: s.raw, v: v, n: [s.length] }; }
var n = 0;
for (var i = 0; i < 30; i++) {
  var { a, b: [c, ...d], ...rest } = { a: { i: i }, b: [{ c: 1 }, [2], { e: 3 }], f: [4], g: { h: 5 } };
  var [x, { y }] = [[i], { y: { z: i } }];
  var t = tag`a${{ i: i }}b${[i]}c`;
  n += a.i + d.length + Object.keys(rest).length + x[0] + y.z + t.v.length;
}
n;"#,
        ),
        seed(
            "iterator-helpers",
            0,
            true,
            r#"function* g() { for (var i = 0; i < 30; i++) yield { i: i, a: [i] }; }
var n = g().map(function (o) { return { m: o.a }; }).filter(function (o) { return o.m[0] % 2 == 0; }).take(8).drop(1).toArray().length;
n += g().flatMap(function (o) { return [{ f: o.i }, [o.i]]; }).take(10).reduce(function (s, v) { return s + { n: 1 }.n; }, 0);
n + g().some(function (o) { return o.a[0] == 20; }) + g().every(function (o) { return o.i < 100; });"#,
        ),
        seed(
            "symbol-bigint-string-churn",
            6,
            true,
            r#"var syms = [], bigs = [], strs = [];
for (var i = 0; i < 50; i++) {
  syms.push(Symbol('s' + i)); bigs.push(BigInt(i) ** 3n + 1n); strs.push('str' + i + 'x'.repeat(i % 7));
  var o = {}; o[syms[i]] = { v: bigs[i] };
}
var n = 0n;
for (var j = 0; j < 50; j++) n += bigs[j] + BigInt(strs[j].length);
String(n) + syms.length + Symbol.for('k').description;"#,
        ),
        seed(
            "typed-array-dataview-allocation",
            4,
            false,
            r#"var views = [];
for (var i = 0; i < 40; i++) {
  var b = new ArrayBuffer(16 + i);
  var u = new Uint8Array(b), d = new DataView(b), f = new Float64Array(2);
  u.fill(i); d.setInt16(2, i); f[0] = d.getInt16(2);
  views.push({ u: u, d: d, f: f, s: u.subarray(1, 5) });
}
var n = 0;
for (var v of views) n += v.s.length + v.f[0] + v.u.map(function (x) { return { x: x }.x; }).length;
n;"#,
        ),
        seed(
            "eval-and-function-constructor-in-loops",
            1,
            true,
            r#"var n = 0;
for (var i = 0; i < 25; i++) {
  n += eval('({ a: [' + i + '] })').a[0];
  n += new Function('x', 'return { r: [x, { y: x }] }')(i).r[1].y;
  n += (0, eval)('var __g' + i + ' = { v: [' + i + '] }; __g' + i + '.v[0]');
}
n;"#,
        ),
        seed(
            "long-loop-large-period",
            7,
            true,
            r#"var keep = [], n = 0;
for (var i = 0; i < 400; i++) {
  var o = { i: i, a: [i, { j: i }], s: 'k' + i };
  if (i % 5 == 0) keep.push(o);
  n += o.a[1].j;
  if (keep.length > 20) keep.shift();
}
n + keep.length;"#,
        ),
        seed(
            "long-loop-mid-period-tree-walker",
            5,
            false,
            r#"var m = new Map(), n = 0;
for (var i = 0; i < 120; i++) {
  var k = { i: i };
  m.set(k, { v: [i], k: k });
  if (m.size > 15) { var first = m.keys().next().value; m.delete(first); }
  n += m.get(k).v[0];
}
n + m.size;"#,
        ),
    ]
}

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("corpus/gc_stress")
}

fn file_name(n: usize, name: &str) -> String {
    format!("{:02}-{name}.bin", n + 1)
}

#[test]
#[ignore = "rewrites fuzz/corpus/gc_stress"]
fn regenerate_seed_corpus() {
    let dir = corpus_dir();
    std::fs::create_dir_all(&dir).unwrap();
    for (n, s) in seeds().iter().enumerate() {
        let bytes = encode(s.period_index, s.bytecode, s.src);
        std::fs::write(dir.join(file_name(n, s.name)), bytes).unwrap();
    }
}

#[test]
fn committed_seeds_match_definitions_and_run_clean() {
    let dir = corpus_dir();
    let defs = seeds();
    let mut on_disk: Vec<_> = std::fs::read_dir(&dir)
        .expect("seed corpus directory")
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    on_disk.sort();
    let expected: Vec<_> = defs
        .iter()
        .enumerate()
        .map(|(n, s)| file_name(n, s.name))
        .collect();
    assert_eq!(on_disk, expected, "run regenerate_seed_corpus");

    let mut periods = HashSet::new();
    let mut modes = HashSet::new();
    for (n, s) in defs.iter().enumerate() {
        let bytes = std::fs::read(dir.join(file_name(n, s.name))).unwrap();
        assert_eq!(
            bytes,
            encode(s.period_index, s.bytecode, s.src),
            "{} is stale: run regenerate_seed_corpus",
            s.name
        );
        assert!(bytes.len() <= 4096, "{} exceeds the target max_len", s.name);
        let input = decode(&bytes).unwrap();
        assert_eq!(input.period, PERIODS[usize::from(s.period_index)]);
        assert!(jsse::fuzz_input_is_runnable(input.src), "{}", s.name);
        assert!(!jsse::fuzz_input_is_gc_observable(input.src), "{}", s.name);
        periods.insert(input.period);
        modes.insert(input.bytecode);

        let (src, period, bytecode) = (input.src, input.period, input.bytecode);
        let budget = step_budget(period);
        let ((plain, plain_count), (stressed, stress_count)) = jsse::run_on_engine_stack(|| {
            (
                jsse::run_script_gc_stressed(src, budget, bytecode, 0),
                jsse::run_script_gc_stressed(src, budget, bytecode, period),
            )
        });
        assert!(
            matches!(plain.outcome, SigOutcome::Completed(_)),
            "{}: {:?}",
            s.name,
            plain.outcome
        );
        assert_eq!(plain, stressed, "{}: stress is observable", s.name);
        assert_eq!(plain_count, 0, "{}", s.name);
        assert!(
            stress_count >= 2 * u64::from(period),
            "{}: only {stress_count} stress safepoints, a major and a minor need {}",
            s.name,
            2 * period
        );
    }
    assert_eq!(periods.len(), PERIODS.len(), "every period needs a seed");
    assert_eq!(modes.len(), 2, "both engines need a seed");
}
