//! CodSpeed benchmarks for the jsse engine.
//!
//! See `benches/README.md`. Every benchmark creates a fresh interpreter, so
//! realm setup is included in `eval` and measured alone by `eval::empty_program`.

use divan::black_box;

fn main() {
    // The interpreter recurses deeply; run the whole harness on the same
    // engine-sized stack the CLI uses.
    jsse::run_on_engine_stack(divan::main);
}

fn eval(src: &str) {
    assert!(jsse::bench_eval(black_box(src)), "benchmark script failed");
}

fn eval_with_check(src: &str, check: &str) {
    assert!(
        jsse::bench_eval_with_check(black_box(src), Some(black_box(check))),
        "benchmark script or validation failed"
    );
}

mod parse {
    use super::*;

    /// A realistic mid-sized script: the micro-benchmark scripts joined.
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
        let src = SCRIPTS.join("\n");
        bencher.bench_local(|| assert!(jsse::bench_parse(black_box(&src))));
    }

    #[divan::bench]
    fn modern_syntax_x50(bencher: divan::Bencher) {
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
if (sum !== 666808) throw new Error("bad closure sum");
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
if (total !== 8331666 || mapped.length !== 5000 || mapped[4999] !== 9998 ||
    arr.length !== 5000 || arr[0] !== 4999 || arr[4999] !== 0) throw new Error("bad array result");
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
if (sum !== 4498500 || acc !== 11994000) throw new Error("bad property sums");
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
if (parts.length !== 3001 || joined.length !== 6000 || idx !== 19 ||
    t !== "6000:-0-1-2-3-4") throw new Error("bad string result");
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
if (parsed.length !== 500 || parsed[499].id !== 499 || parsed[499].name !== "item499" ||
    parsed[499].value !== 748.5 || parsed[499].tags.join("") !== "ab") throw new Error("bad JSON result");
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
if (count !== 450 || replaced !== "the quick br0wn f0x jumps 0ver the lazy d0g ".repeat(50))
  throw new Error("bad regex result");
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
if (mixed(5000) !== 5 || heap32[17] !== 24999) throw new Error("bad typed-array result");
"#,
        );
    }

    #[divan::bench]
    fn promises() {
        eval_with_check(
            r#"
const keep = [];
let acc = 0;
for (let i = 0; i < 2000; i++) {
  const p = new Promise((resolve) => { resolve(i); });
  if ((i & 63) === 0) keep.push(p);
  acc = (acc + i) | 0;
}
var benchmarkResult;
Promise.all(keep).then((vs) => { let s = 0; for (const v of vs) s += v; benchmarkResult = s; });
"#,
            "if (acc !== 1999000 || keep.length !== 32 || benchmarkResult !== 31744) throw new Error('bad promise result');",
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
if (sum !== 4501500 || m.size !== 3000 || s.size !== 100) throw new Error("bad collection result");
"#,
        );
    }

    #[divan::bench]
    fn closures_local() {
        eval(
            r#"
function makeAdder(x) { return function (y) { return x + y; }; }
function main() {
  var sum = 0;
  for (var i = 0; i < 5000; i++) { var add = makeAdder(i & 15); sum += add(i & 255); }
  return sum;
}
if (main() !== 666808) throw new Error("bad local closure sum");
"#,
        );
    }

    mod regexp {
        use super::*;

        // A would-be match lies far beyond lastIndex. A failed sticky search
        // must stay anchored, including through the lookahead matcher (#921).
        const STICKY: &str = r#"
var src = "x".repeat(14000) + "5";
var re = /DIGIT/y;
function scan() {
  var hits = 0;
  for (var i = 0; i < 1500; i++) {
    re.lastIndex = i % 50;
    if (re.exec(src) !== null) hits++;
    if ((i & 7) === 0 && !/\d/.test("5")) throw new Error("bad short subject");
  }
  re.lastIndex = 14000;
  var tail = re.exec(src);
  if (hits !== 0 || tail === null || tail[0] !== "5" || re.lastIndex !== 14001)
    throw new Error("bad sticky result");
}
scan();
"#;

        fn sticky(bencher: divan::Bencher, pattern: &str) {
            let src = STICKY.replace("DIGIT", pattern);
            bencher.bench_local(|| eval(&src));
        }

        #[divan::bench]
        fn sticky_failure(bencher: divan::Bencher) {
            sticky(bencher, r"\d");
        }

        #[divan::bench]
        fn sticky_lookahead_failure(bencher: divan::Bencher) {
            sticky(bencher, r"(?!0)\d");
        }
    }

    mod async_jobs {
        use super::*;

        #[divan::bench]
        fn then_chain() {
            eval_with_check(
                r#"
var benchmarkResult;
var p = Promise.resolve(0);
for (var i = 0; i < 2000; i++) p = p.then(v => v + 1);
p.then(v => { benchmarkResult = v; });
"#,
                "if (benchmarkResult !== 2000) throw new Error('bad chain result');",
            );
        }

        #[divan::bench]
        fn await_loop() {
            eval_with_check(
                r#"
var benchmarkResult;
async function leaf(x) { return x + 1; }
async function main() {
  let sum = 0;
  for (let i = 0; i < 1000; i++) {
    sum += await i;
    sum += await Promise.resolve(i);
    sum += await leaf(i);
  }
  return sum;
}
main().then(v => { benchmarkResult = v; });
"#,
                "if (benchmarkResult !== 1499500) throw new Error('bad await result');",
            );
        }
    }

    mod generators {
        use super::*;

        #[divan::bench]
        fn manual_next() {
            eval(
                r#"
function* acc() {
  let total = 0;
  while (true) { const v = yield total; total += v; }
}
const it = acc();
const first = it.next();
let last;
for (let i = 0; i < 3000; i++) last = it.next(i);
const closed = it.return(7);
if (first.done || first.value !== 0 || last.done || last.value !== 4498500 ||
    !closed.done || closed.value !== 7) throw new Error("bad generator result");
"#,
            );
        }

        #[divan::bench]
        fn pipeline() {
            eval(
                r#"
function* range(n) { for (let i = 1; i <= n; i++) yield i; }
function* map(data) { for (const x of data) yield x * 2; }
function* filter(data) { for (const x of data) if (x % 3 !== 0) yield x; }
function* takeWhile(data) { for (const x of data) { if (x >= 4000) return; yield x; } }
let sum = 0;
for (const x of takeWhile(filter(map(range(3000))))) sum += x;
if (sum !== 2665334) throw new Error("bad pipeline result");
"#,
            );
        }
    }

    mod fields {
        use super::*;

        const ACCESS: &str = r#"
class Acc {
  #a = 1; #b = 2; #c = 3;
  run(n) {
    let sum = 0;
    for (let i = 0; i < n; i++) {
      this.#a = this.#a + 1;
      sum += this.#a + this.#b + this.#c;
      this.#c = sum & 255;
    }
    return sum;
  }
}
if (new Acc().run(5000) !== 13161075) throw new Error("bad field result");
"#;

        const GETTERS: &str = r#"
class Point {
  #a = 1; #b = 2;
  get first() { return this.#a; }
  get second() { return this.#b; }
  run(n) {
    let sum = 0;
    for (let i = 0; i < n; i++) { sum += this.first + this.second; this.#a++; }
    return sum;
  }
}
if (new Point().run(5000) !== 12512500) throw new Error("bad getter result");
"#;

        fn access(bencher: divan::Bencher, src: &str) {
            bencher.bench_local(|| eval(src));
        }

        #[divan::bench]
        fn private_fields(bencher: divan::Bencher) {
            access(bencher, ACCESS);
        }

        #[divan::bench]
        fn public_fields(bencher: divan::Bencher) {
            access(bencher, &ACCESS.replace('#', ""));
        }

        #[divan::bench]
        fn private_getters(bencher: divan::Bencher) {
            access(bencher, GETTERS);
        }

        #[divan::bench]
        fn public_getters(bencher: divan::Bencher) {
            access(bencher, &GETTERS.replace('#', ""));
        }
    }

    #[divan::bench]
    fn map_set_rolling() {
        eval(
            r#"
const window = 256;
const m = new Map();
const s = new Set();
let hits = 0;
for (let i = 0; i < 5000; i++) {
  m.set(i, i * 2); s.add(i);
  if (i >= window) { m.delete(i - window); s.delete(i - window); }
  if (m.has(i - 5)) hits++;
  if (s.has(i - 7)) hits++;
  if (m.get(i - 11) === (i - 11) * 2) hits++;
}
let sum = 0;
for (const [k, v] of m) sum += k + v;
if (m.size !== window || s.size !== window || hits !== 14977 || sum !== 3741312 ||
    m.has(4743) || !m.has(4744)) throw new Error("bad rolling collection result");
"#,
        );
    }

    mod gc {
        use super::*;

        // More than one nursery of retained objects, followed by multiple
        // nurseries of writes concentrated in two cards of an old container.
        // Keep these sizes large enough to collect: validate with perf-counters
        // before reducing them for the simulator. No forced/stress collections.
        const RETAINED: &str = r#"
var table = NEW_TABLE;
function run() {
  for (var i = 0; i < 16384; i++) { INITIAL_STORE; }
  for (var j = 0; j < 24576; j++) { UPDATE_STORE; }
  var sum = 0;
  for (var k = 0; k < 16384; k++) sum += READ_VALUE.value;
  if (sum !== 142598144) throw new Error("bad retained-object result");
}
run();
"#;

        fn retained(
            bencher: divan::Bencher,
            new_table: &str,
            initial: &str,
            update: &str,
            read: &str,
        ) {
            let src = RETAINED
                .replace("NEW_TABLE", new_table)
                .replace("INITIAL_STORE", initial)
                .replace("UPDATE_STORE", update)
                .replace("READ_VALUE", read);
            bencher.bench_local(|| eval(&src));
        }

        #[divan::bench]
        fn retained_array(bencher: divan::Bencher) {
            retained(
                bencher,
                "new Array(16384)",
                "table[i] = {value: i}",
                "table[15360 + (j & 1023)] = {value: j}",
                "table[k]",
            );
        }

        #[divan::bench]
        fn retained_map(bencher: divan::Bencher) {
            retained(
                bencher,
                "new Map()",
                "table.set(i, {value: i})",
                "table.set(15360 + (j & 1023), {value: j})",
                "table.get(k)",
            );
        }
    }

    mod opmix {
        use super::*;

        // Separate cases keep calls, element traffic and labeled control flow
        // from hiding one another (cf. benchmarks/scripts/bench_opmix.js).
        // Each case parses only its own function.
        const HEAP: &str = "var heap32 = new Int32Array(1024);\n";
        const ARITH: &str = r#"
function arith(n) {
  var r0 = 0, r1 = 1, r2 = 2;
  for (var i = 0; i < n; i++) {
    r0 = (r0 + 4) | 0; r1 = (r0 + r1) | 0; r1 &= 65535;
    r2 = r1 >> 2; r0 = (r2 + r0) | 0; r0 &= 65535;
  }
  return r0 + r1 + r2;
}
"#;
        const ELEM: &str = r#"
function elem(n) {
  var fp = 16;
  for (var i = 0; i < n; i++) {
    var r0 = (heap32[fp] + 4) | 0;
    heap32[fp] = r0;
    var r1 = (r0 + heap32[fp + 1]) & 65535;
    heap32[fp + 1] = r1;
  }
  return heap32[fp] + heap32[fp + 1];
}
"#;
        const CALLED: &str = r#"
function leaf(x) { return ((x >> 2) + 4) & 65535; }
function called(n) {
  var sum = 0;
  for (var i = 0; i < n; i++) { sum = leaf(sum); sum = leaf(sum); }
  return sum;
}
"#;
        const LABELED_LOOP: &str = r#"
function labeledLoop(n) {
  var i = 0, fp = 16;
  loop: while (true) {
    if (i >= n) break loop;
    var r0 = (heap32[fp] + 4) | 0;
    heap32[fp] = r0;
    i++;
    if ((i & 1) === 0) continue loop;
    heap32[fp + 1] = (r0 + heap32[fp + 1]) & 65535;
  }
  return heap32[fp] + heap32[fp + 1];
}
"#;
        const LABELED_CONTROL: &str = r#"
function labeledControl(n) {
  var i = 0;
  loop: while (true) { if (i >= n) break loop; i++; continue loop; }
  return i;
}
"#;

        fn run(bencher: divan::Bencher, source: &[&str], name: &str, expected: i32) {
            let src = format!(
                "{}\nif ({name}(5000) !== {expected}) throw new Error('bad opmix result');",
                source.concat()
            );
            bencher.bench_local(|| eval(&src));
        }

        #[divan::bench]
        fn arithmetic(bencher: divan::Bencher) {
            run(bencher, &[ARITH], "arith", 129073);
        }

        #[divan::bench]
        fn elements(bencher: divan::Bencher) {
            run(bencher, &[HEAP, ELEM], "elem", 26032);
        }

        #[divan::bench]
        fn leaf_calls(bencher: divan::Bencher) {
            run(bencher, &[CALLED], "called", 5);
        }

        #[divan::bench]
        fn labeled_loop(bencher: divan::Bencher) {
            run(bencher, &[HEAP, LABELED_LOOP], "labeledLoop", 50784);
        }

        #[divan::bench]
        fn labeled_control(bencher: divan::Bencher) {
            run(bencher, &[LABELED_CONTROL], "labeledControl", 5000);
        }
    }

    #[divan::bench]
    fn bigint_mulmod256() {
        eval(
            r#"
const P = 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffefffffc2fn;
function mod(a, b = P) { const r = a % b; return r >= 0n ? r : b + r; }
let x = 0x79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798n;
let y = 0x483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8n;
let acc = 0n;
for (let i = 0; i < 1000; i++) {
  x = mod(x * y); y = mod(y * y + x); acc = mod(acc + x);
}
if (acc % 1000000007n !== 251339868n || x % 1000003n !== 688878n) throw new Error("bad BigInt result");
"#,
        );
    }
}
