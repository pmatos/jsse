//! CodSpeed benchmarks for the jsse engine.
//!
//! Workloads mirror `benchmarks/scripts/*.js`, scaled down so each iteration
//! stays short under CodSpeed's CPU simulation. Every benchmark creates a
//! fresh interpreter, so realm setup is included in the `eval` group and
//! measured on its own by `eval::empty_program`.

use divan::black_box;

fn main() {
    // The interpreter recurses deeply; run the whole harness on the same
    // engine-sized stack the CLI uses.
    jsse::run_on_engine_stack(divan::main);
}

fn eval(src: &str) {
    assert!(jsse::bench_eval(black_box(src)), "benchmark script failed");
}

mod parse {
    use super::*;

    /// A realistic mid-sized script: every micro-benchmark script concatenated.
    const SCRIPTS: &[&str] = &[
        include_str!("../benchmarks/scripts/bench_array.js"),
        include_str!("../benchmarks/scripts/bench_closure_create.js"),
        include_str!("../benchmarks/scripts/bench_closures.js"),
        include_str!("../benchmarks/scripts/bench_fib.js"),
        include_str!("../benchmarks/scripts/bench_json.js"),
        include_str!("../benchmarks/scripts/bench_loop.js"),
        include_str!("../benchmarks/scripts/bench_object.js"),
        include_str!("../benchmarks/scripts/bench_opmix.js"),
        include_str!("../benchmarks/scripts/bench_regex.js"),
        include_str!("../benchmarks/scripts/bench_string.js"),
    ];

    const CLASSES: &str = r#"
class Vec2 {
  #x; #y;
  constructor(x, y) { this.#x = x; this.#y = y; }
  get x() { return this.#x; }
  get y() { return this.#y; }
  add(o) { return new Vec2(this.#x + o.x, this.#y + o.y); }
  static zero() { return new Vec2(0, 0); }
}
const sum = async (...xs) => xs.reduce((a, { x = 0, y = 0 } = {}) => a + x + y, 0);
function* gen(n) { for (let i = 0; i < n; i++) yield `item ${i}: ${i * 2}`; }
const obj = { a: 1, b: [1, 2, 3], ...{ c: 3 }, ['k' + 1]: /re+gex/giu, d: 1n << 64n };
label: for (const [k, v] of Object.entries(obj)) { if (k === 'b') continue label; }
"#;

    #[divan::bench]
    fn bench_scripts(bencher: divan::Bencher) {
        let src = SCRIPTS.concat();
        bencher.bench_local(|| assert!(jsse::bench_parse(black_box(&src))));
    }

    #[divan::bench]
    fn modern_syntax_x50(bencher: divan::Bencher) {
        // Each copy gets its own block so lexical declarations do not clash.
        let src = format!("{{{CLASSES}}}\n").repeat(50);
        bencher.bench_local(|| assert!(jsse::bench_parse(black_box(&src))));
    }
}

mod eval {
    use super::*;

    #[divan::bench]
    fn empty_program() {
        eval("");
    }

    #[divan::bench]
    fn fib_recursive() {
        eval(
            r#"
function fib(n) { if (n < 2) return n; return fib(n - 1) + fib(n - 2); }
if (fib(18) !== 2584) throw new Error("bad");
"#,
        );
    }

    #[divan::bench]
    fn loop_sum() {
        eval(
            r#"
var sum = 0;
for (var i = 0; i < 20000; i++) { sum += i; }
if (sum !== 199990000) throw new Error("bad");
"#,
        );
    }

    #[divan::bench]
    fn closures() {
        eval(
            r#"
function makeAdder(x) { return function (y) { return x + y; }; }
var sum = 0;
for (var i = 0; i < 5000; i++) { var add = makeAdder(i & 15); sum += add(i & 255); }
"#,
        );
    }

    #[divan::bench]
    fn array_ops() {
        eval(
            r#"
var arr = [];
for (var i = 0; i < 5000; i++) arr.push(i);
var mapped = arr.map(function (x) { return x * 2; });
var total = mapped.filter(function (x) { return x % 3 === 0; })
  .reduce(function (a, b) { return a + b; }, 0);
arr.sort(function (a, b) { return b - a; });
"#,
        );
    }

    #[divan::bench]
    fn object_props() {
        eval(
            r#"
var obj = {};
for (var i = 0; i < 3000; i++) obj["key" + i] = i;
var sum = 0;
for (var k in obj) sum += obj[k];
var points = [];
for (var j = 0; j < 2000; j++) points.push({ x: j, y: j * 2, z: j * 3 });
var acc = 0;
for (var p = 0; p < points.length; p++) acc += points[p].x + points[p].y + points[p].z;
"#,
        );
    }

    #[divan::bench]
    fn classes_and_methods() {
        eval(
            r#"
class Vec2 {
  constructor(x, y) { this.x = x; this.y = y; }
  add(o) { return new Vec2(this.x + o.x, this.y + o.y); }
  get len2() { return this.x * this.x + this.y * this.y; }
}
let v = new Vec2(0, 0);
const d = new Vec2(1, 2);
for (let i = 0; i < 3000; i++) v = v.add(d);
if (v.len2 !== 45000000) throw new Error("bad");
"#,
        );
    }

    #[divan::bench]
    fn string_ops() {
        eval(
            r#"
var s = "";
for (var i = 0; i < 3000; i++) s += "x" + (i % 10);
var parts = s.split("x");
var joined = parts.join("-").toUpperCase();
var idx = joined.indexOf("9-0");
var t = `${s.length}:${joined.slice(0, 10)}`;
"#,
        );
    }

    #[divan::bench]
    fn json_roundtrip() {
        eval(
            r#"
var data = [];
for (var i = 0; i < 500; i++) data.push({ id: i, name: "item" + i, value: i * 1.5, tags: ["a", "b"] });
var str = JSON.stringify(data);
var parsed = JSON.parse(str);
if (parsed.length !== 500) throw new Error("bad");
"#,
        );
    }

    #[divan::bench]
    fn regex_exec() {
        eval(
            r#"
var s = "the quick brown fox jumps over the lazy dog ".repeat(50);
var re = /[a-z]+/g;
var count = 0, m;
while ((m = re.exec(s)) !== null) count++;
var replaced = s.replace(/o(\w)/g, "0$1");
if (count !== 450) throw new Error("bad");
"#,
        );
    }

    #[divan::bench]
    fn typed_array_opmix() {
        eval(
            r#"
var heap32 = new Int32Array(1 << 10);
function leaf(sp) { var r0 = sp >> 2; r0 = (r0 + 4) | 0; return r0 & 65535; }
function mixed(n) {
  var fp = 16;
  for (var i = 0; i < n; i++) {
    var r0 = heap32[fp];
    r0 = leaf(r0);
    heap32[fp] = r0;
    var r1 = heap32[fp + 1];
    r1 = (r0 + r1) | 0;
    r1 = r1 & 65535;
    heap32[fp + 1] = r1;
  }
  return heap32[fp];
}
mixed(5000);
"#,
        );
    }

    #[divan::bench]
    fn promises() {
        eval(
            r#"
const keep = [];
let acc = 0;
for (let i = 0; i < 2000; i++) {
  const p = new Promise((resolve) => { resolve(i); });
  if ((i & 63) === 0) keep.push(p);
  acc = (acc + i) | 0;
}
Promise.all(keep).then((vs) => { let s = 0; for (const v of vs) s += v; });
"#,
        );
    }

    #[divan::bench]
    fn map_set() {
        eval(
            r#"
const m = new Map();
const s = new Set();
for (let i = 0; i < 3000; i++) { m.set("k" + i, i); s.add(i % 100); }
let sum = 0;
for (const [k, v] of m) sum += v;
for (let i = 0; i < 3000; i++) if (m.has("k" + i) && s.has(i % 100)) sum++;
"#,
        );
    }
}
