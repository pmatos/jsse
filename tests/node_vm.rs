//! The embedded `vm` module under `--node` (#1238). The fixture matrix in
//! scripts/vm-fixtures/{probe,errors}.js are diffed against Node by
//! `scripts/run-node-vm-oracle.sh`, which also checks that the pinned
//! expected-*.txt files are Node's output; these tests hold jsse to the pinned files (so
//! they need no Node) and cover what the matrix cannot: the private binding,
//! the default environment and behaviour under the collector.
#![cfg(unix)]

use std::path::PathBuf;
use std::process::{Command, Output};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("scripts/vm-fixtures")
}

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

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).trim_end().to_string()
}

fn node(source: &str, flags: &[&str]) -> String {
    let mut args = vec!["--node"];
    args.extend_from_slice(flags);
    args.extend(["-e", source]);
    let out = run(&args, None);
    assert!(out.status.success(), "{}", stderr(&out));
    stdout(&out)
}

fn probe(flags: &[&str], gc_stress: Option<&str>) {
    for name in ["probe", "errors"] {
        let script = fixtures().join(format!("{name}.js"));
        let mut args = vec!["--node"];
        args.extend_from_slice(flags);
        args.push(script.to_str().unwrap());
        let out = run(&args, gc_stress);
        assert!(out.status.success(), "{name}: {}", stderr(&out));
        let want =
            std::fs::read_to_string(fixtures().join(format!("expected-{name}.txt"))).unwrap();
        assert_eq!(stdout(&out), want.trim_end(), "{name}");
    }
}

#[test]
fn matches_node_under_the_bytecode_vm() {
    probe(&[], None);
}

#[test]
fn matches_node_under_the_tree_walker() {
    probe(&["--no-bytecode"], None);
}

#[test]
fn matches_node_under_gc_stress() {
    probe(&[], Some("7"));
}

#[test]
fn binding_is_private_and_modules_resolve_under_both_names() {
    let source = r#"
      const vm = require("vm");
      print([
        vm === require("node:vm"),
        typeof __host_vm,
        (() => { try { require("internal/host/vm"); return "reachable"; } catch (e) { return e.code; } })(),
        (() => { try { require("internal/vm/sync"); return "reachable"; } catch (e) { return e.code; } })(),
      ].join("|"));
    "#;
    for flags in [&[][..], &["--no-bytecode"][..]] {
        assert_eq!(
            node(source, flags),
            "true|undefined|MODULE_NOT_FOUND|MODULE_NOT_FOUND"
        );
    }
}

#[test]
fn a_context_has_no_host_globals_and_the_main_realm_keeps_them() {
    let source = r#"
      const vm = require("vm");
      const inside = vm.runInNewContext(
        "[typeof $262, typeof setTimeout, typeof ShadowRealm, typeof console.log, typeof structuredClone].join()", {});
      print([inside, typeof setTimeout, typeof structuredClone, typeof ShadowRealm].join("|"));
    "#;
    assert_eq!(
        node(source, &[]),
        "undefined,undefined,undefined,function,undefined|function|function|function"
    );
}

#[test]
fn many_contexts_share_one_collector() {
    let source = r#"
      const vm = require("vm");
      let total = 0;
      for (let i = 0; i < 40; i++) {
        const c = vm.createContext({ i });
        total += vm.runInContext("var seen = i; function f() { return seen } f() + 1", c);
        if (c.seen !== i || c.f() !== i) throw new Error("lost write at " + i);
      }
      $262.gc();
      print(total);
    "#;
    for stress in [None, Some("3")] {
        let out = run(&["--node", "-e", source], stress);
        assert!(out.status.success(), "{}", stderr(&out));
        assert_eq!(stdout(&out), "820");
    }
}

#[test]
fn default_mode_has_no_vm() {
    let out = run(
        &["-e", "print(typeof require, typeof __host_vm, typeof vm)"],
        None,
    );
    assert!(out.status.success(), "{}", stderr(&out));
    assert_eq!(stdout(&out), "undefined undefined undefined");
}
