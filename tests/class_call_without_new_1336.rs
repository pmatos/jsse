//! Calling a class constructor without `new` names the class in the
//! TypeError (#1336). The text is implementation-defined (ECMA-262 §10.2.1
//! [[Call]] step 2), so test262 cannot see it; expectations are Node's.

use std::process::Command;

const SCRIPT: &str = r#"
const out = [];
function t(f) {
  try { f(); out.push('no throw'); }
  catch (e) { out.push((e instanceof TypeError) + ' ' + e.message); }
}
class A {}
t(() => A());
t(() => (class {})());
const B = class {};
t(() => B());
const o = { C: class {} };
t(() => o.C());
class D { static name = 'x'; }
t(() => D());
class E {}
Object.defineProperty(E, 'name', { value: 'Z' });
t(() => E());
class F { static get name() { return 'g'; } }
t(() => F());
class G extends A {}
t(() => G());
t(() => A.call({}));
t(() => Reflect.apply(A, null, []));
t(() => [1].map(A));
class H { static name() {} }
t(() => H());
class I {}
delete I.name;
t(() => I());
let K = class L {};
t(() => K());
t(() => (class { static name = 5 })());
console.log(out.join('\n'));
"#;

const EXPECTED: &str = "\
true Class constructor A cannot be invoked without 'new'
true Class constructors cannot be invoked without 'new'
true Class constructor B cannot be invoked without 'new'
true Class constructor C cannot be invoked without 'new'
true Class constructor D cannot be invoked without 'new'
true Class constructor E cannot be invoked without 'new'
true Class constructor F cannot be invoked without 'new'
true Class constructor G cannot be invoked without 'new'
true Class constructor A cannot be invoked without 'new'
true Class constructor A cannot be invoked without 'new'
true Class constructor A cannot be invoked without 'new'
true Class constructor H cannot be invoked without 'new'
true Class constructor I cannot be invoked without 'new'
true Class constructor L cannot be invoked without 'new'
true Class constructors cannot be invoked without 'new'";

fn run(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(args)
        .args(["-e", SCRIPT])
        .output()
        .unwrap();
    assert!(out.status.success(), "{args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn bytecode_vm_names_the_class() {
    assert_eq!(run(&[]), EXPECTED);
}

#[test]
fn tree_walker_names_the_class() {
    assert_eq!(run(&["--no-bytecode"]), EXPECTED);
}

#[test]
fn node_host_mode_uses_the_same_text() {
    assert_eq!(run(&["--node"]), EXPECTED);
    assert_eq!(run(&["--node", "--no-bytecode"]), EXPECTED);
}
