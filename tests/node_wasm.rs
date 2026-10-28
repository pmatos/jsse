//! The `WebAssembly` global under `--node` (#1378). Known answers from the WebAssembly JS API;
//! `scripts/run-node-wasm-oracle.sh` diffs the full corpus against Node. The default environment has
//! no `WebAssembly` at all.

use std::process::{Command, Output};

const BUILDER: &str = include_str!("../scripts/wasm-fixtures/builder.js");

fn run(args: &[&str], gc_stress: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_jsse"));
    if let Some(period) = gc_stress {
        cmd.env("JSSE_GC_STRESS", period);
    }
    cmd.args(args);
    cmd.output().expect("run jsse")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

fn with_builder(source: &str) -> String {
    format!(
        "const {{ build, op }} = (function () {{ const module = {{ exports: {{}} }}; {BUILDER}\n\
         return module.exports; }})();\n{source}"
    )
}

fn node(source: &str, flags: &[&str], gc_stress: Option<&str>) -> String {
    let mut args = vec!["--node"];
    args.extend_from_slice(flags);
    let program = with_builder(source);
    args.extend(["-e", &program]);
    let out = run(&args, gc_stress);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    stdout(&out)
}

fn each_mode(source: &str, want: &str) {
    for flags in [&[][..], &["--no-bytecode"][..]] {
        assert_eq!(node(source, flags, None), want, "flags {flags:?}");
    }
    assert_eq!(node(source, &[], Some("3")), want, "GC stress");
}

#[test]
fn absent_without_node_host() {
    let out = run(&["-e", "print(typeof WebAssembly)"], None);
    assert_eq!(stdout(&out), "undefined");
    let out = run(
        &[
            "--node",
            "-e",
            "print(typeof WebAssembly, typeof __host_wasm)",
        ],
        None,
    );
    assert_eq!(stdout(&out), "object undefined");
}

#[test]
fn calls_coercion_and_traps() {
    let source = r#"
      const t = (p, r) => [p, r];
      const { exports: x } = new WebAssembly.Instance(new WebAssembly.Module(build({
        types: [t(["i32", "i32"], ["i32"]), t(["i64", "i64"], ["i64"]), t(["f32"], ["f64"]), t([], [])],
        funcs: [0, 1, 2, 3, 0],
        exports: [["add", "func", 0], ["add64", "func", 1], ["widen", "func", 2], ["trap", "func", 3], ["div", "func", 4]],
        codes: [
          { body: [op.local_get(0), op.local_get(1), op.i32_add] },
          { body: [op.local_get(0), op.local_get(1), op.i64_add] },
          { body: [op.local_get(0), 0xbb] },
          { body: [op.unreachable] },
          { body: [op.local_get(0), op.local_get(1), op.i32_div_s] },
        ],
      })));
      const caught = (f, m) => { try { f(); } catch (e) { return e.name + (m ? ":" + e.message : ""); } };
      print([
        x.add(2147483647, 1), x.add("3", 4.9), x.add64(2n ** 63n - 1n, 1n), x.widen(0.1),
        caught(() => x.add64(1, 2)), caught(() => x.trap(), 1), caught(() => x.div(1, 0), 1),
        x.add.name, x.add.length, Object.isFrozen(x), x.add(1, 2),
      ].join("\n"));
    "#;
    each_mode(
        source,
        "-2147483648\n7\n-9223372036854775808\n0.10000000149011612\n\
         TypeError\nRuntimeError:unreachable\n\
         RuntimeError:divide by zero\n0\n2\ntrue\n3",
    );
}

#[test]
fn memory_aliases_and_detaches_on_grow() {
    let source = r#"
      const t = (p, r) => [p, r];
      const m = new WebAssembly.Instance(new WebAssembly.Module(build({
        types: [t(["i32", "i32"], []), t(["i32"], ["i32"])],
        mems: [[1, 3]],
        funcs: [0, 1, 1],
        exports: [["memory", "memory", 0], ["store", "func", 0], ["load", "func", 1], ["grow", "func", 2]],
        codes: [
          { body: [op.local_get(0), op.local_get(1), op.i32_store()] },
          { body: [op.local_get(0), op.i32_load()] },
          { body: [op.local_get(0), op.memory_grow] },
        ],
        datas: [{ offset: 4, bytes: [104, 105] }],
      }))).exports;
      const b = m.memory.buffer;
      const u8 = new Uint8Array(b);
      const out = [b.byteLength, b === m.memory.buffer, u8[4], u8[5]];
      u8[16] = 7;
      out.push(m.load(16));
      m.store(32, 0x0a0b0c0d);
      out.push(u8[32], u8[35]);
      out.push(m.grow(1), b.byteLength, u8.length, b.detached, m.memory.buffer.byteLength, m.memory.buffer === b);
      out.push(new Uint8Array(m.memory.buffer)[32], m.grow(5), m.memory.grow(1), m.memory.buffer.byteLength);
      let err;
      try { m.memory.grow(1); } catch (e) { err = e.name; }
      out.push(err);
      let transfer;
      try { m.memory.buffer.transfer(); } catch (e) { transfer = e.name; }
      let clone;
      try { structuredClone(m.memory.buffer, { transfer: [m.memory.buffer] }); } catch (e) { clone = e.name; }
      out.push(transfer, clone, structuredClone(m.memory.buffer).byteLength);
      print(out.join(" "));
    "#;
    each_mode(
        source,
        "65536 true 104 105 7 13 10 1 0 0 true 131072 false 13 -1 2 196608 RangeError \
         TypeError DataCloneError 196608",
    );
}

#[test]
fn imports_exceptions_and_reentrancy() {
    let source = r#"
      const t = (p, r) => [p, r];
      const spec = {
        types: [t(["i32"], ["i32"]), t([], []), t(["i32", "f64", "i64"], ["f64"])],
        imports: [
          { module: "env", name: "f", kind: "func", type: 0 },
          { module: "env", name: "boom", kind: "func", type: 1 },
          { module: "env", name: "sum", kind: "func", type: 2 },
        ],
        funcs: [0, 1, 0],
        exports: [["twice", "func", 3], ["boom", "func", 4], ["sum", "func", 5]],
        codes: [
          { body: [op.local_get(0), op.call(0), op.call(0)] },
          { body: [op.call(1)] },
          { body: [op.local_get(0), op.f64_const(0.5), op.i64_const(-1), op.call(2), op.drop, op.local_get(0)] },
        ],
      };
      spec.types.push(t(["i32"], ["i32"]));
      const mod = new WebAssembly.Module(build(spec));
      const seen = [];
      const marker = { marker: true };
      let depth = 0;
      const inst = new WebAssembly.Instance(mod, { env: {
        f(v) { return depth++ < 2 ? inst.exports.twice(v) + 1 : v + 10; },
        boom() { throw marker; },
        sum(a, b, c) { seen.push([a, b, c]); return "1.5"; },
      } });
      let same;
      try { inst.exports.boom(); } catch (e) { same = e === marker; }
      print([inst.exports.twice(0), depth, same, inst.exports.sum(9), JSON.stringify(seen, (k, v) => typeof v === "bigint" ? v + "n" : v)].join(" "));
    "#;
    each_mode(source, "42 6 true 9 [[9,0.5,\"-1n\"]]");
}

#[test]
fn tables_globals_and_promises() {
    let source = r#"
      const t = (p, r) => [p, r];
      (async () => {
        const bytes = build({
          types: [t(["i32"], ["i32"]), t(["i32", "i32"], ["i32"])],
          funcs: [0, 1],
          tables: [["funcref", 2, 4]],
          globals: [{ type: "i32", mutable: true, init: op.i32_const(5) }],
          exports: [["inc", "func", 0], ["call", "func", 1], ["tbl", "table", 0], ["g", "global", 0]],
          elems: [{ offset: 0, funcs: [0] }],
          codes: [
            { body: [op.local_get(0), op.i32_const(1), op.i32_add] },
            { body: [op.local_get(1), op.local_get(0), op.call_indirect(0)] },
          ],
        });
        const { module, instance } = await WebAssembly.instantiate(bytes);
        const e = instance.exports;
        const out = [module instanceof WebAssembly.Module, e.tbl.get(0) === e.inc, String(e.tbl.get(1)), e.tbl.length, e.call(0, 41)];
        let trap;
        try { e.call(1, 0); } catch (err) { trap = err.name + ":" + err.message; }
        out.push(trap, e.tbl.grow(1), e.tbl.length, e.g.value);
        e.g.value = 9;
        const bad = await WebAssembly.compile(new Uint8Array(8)).catch((err) => err.constructor === WebAssembly.CompileError);
        out.push(e.g.value, bad, WebAssembly.validate(bytes), WebAssembly.validate(bytes.subarray(0, 12)));
        console.log(out.join(" "));
      })();
    "#;
    each_mode(
        source,
        "true true null 2 42 RuntimeError:function signature mismatch 2 3 5 9 true true false",
    );
}

#[test]
fn start_function_and_multi_value() {
    let source = r#"
      const t = (p, r) => [p, r];
      const x = new WebAssembly.Instance(new WebAssembly.Module(build({
        types: [t([], []), t(["i32", "i64"], ["i64", "i32"])],
        globals: [{ type: "i32", mutable: true, init: op.i32_const(1) }],
        funcs: [0, 1],
        start: 0,
        exports: [["swap", "func", 1], ["g", "global", 0]],
        codes: [
          { body: [op.global_get(0), op.i32_const(10), op.i32_add, op.global_set(0)] },
          { body: [op.local_get(1), op.local_get(0)] },
        ],
      }))).exports;
      print(x.g.value, JSON.stringify(x.swap(3, 4n), (k, v) => typeof v === "bigint" ? v + "n" : v));
    "#;
    each_mode(source, "11 [\"4n\",3]");
}

#[test]
fn link_errors_and_bad_input() {
    let source = r#"
      const names = [];
      const attempt = (f) => { try { f(); names.push("ok"); } catch (e) { names.push(e.constructor.name); } };
      const mod = new WebAssembly.Module(build({
        types: [[["i32"], ["i32"]]],
        imports: [{ module: "m", name: "f", kind: "func", type: 0 }, { module: "m", name: "mem", kind: "memory", limits: [1] }],
      }));
      attempt(() => new WebAssembly.Instance(mod));
      attempt(() => new WebAssembly.Instance(mod, {}));
      attempt(() => new WebAssembly.Instance(mod, { m: { f() {}, mem: {} } }));
      attempt(() => new WebAssembly.Instance(mod, { m: { f: 1, mem: new WebAssembly.Memory({ initial: 1 }) } }));
      attempt(() => new WebAssembly.Instance(mod, { m: { f() {}, mem: new WebAssembly.Memory({ initial: 1 }) } }));
      attempt(() => new WebAssembly.Module(new Uint8Array([1, 2, 3])));
      attempt(() => new WebAssembly.Module("x"));
      attempt(() => new WebAssembly.Memory({ initial: 70000 }));
      print(names.join(" "));
    "#;
    each_mode(
        source,
        "TypeError TypeError LinkError LinkError ok CompileError TypeError RangeError",
    );
}
