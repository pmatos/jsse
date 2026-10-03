use std::process::{Command, Output};

const PROGRAM: &str = r#"
var log = [];
function make(n) { var o = { n: n, next: null }; return o; }
var head = make(0), cur = head;
for (var i = 1; i < 200; i++) { cur.next = make(i); cur = cur.next; }
var sum = 0;
for (var p = head; p; p = p.next) sum += p.n;
log.push(sum);
var [a, b, ...rest] = [1, 2, 3, 4, 5];
log.push(a + b + rest.length);
var m = new Map([[1, { v: 1 }], [2, { v: 2 }]]);
for (var [k, v] of m) log.push(k + v.v);
log.push(JSON.stringify([...new Set([3, 3, 4])].map(function (x) { return x * 2; })));
Promise.all([1, Promise.resolve(2)]).then(function (xs) { console.log(log.join(","), xs.join("+")); });
"#;

/// Loop-free: every statement is a straight-line call to an allocating
/// function. The top-level `function make` declaration makes the script body
/// bail to the tree-walker (compiled chunks don't support function
/// declarations), but `make`'s own body still compiles to bytecode, and it
/// has no loop — so before issue #808, its compiled chunk had no safepoint at
/// all, meaning `--bytecode` stress could never collect inside it and never
/// checked that `o` stays rooted between its allocation and `return o`.
const PROGRAM_STRAIGHT_LINE: &str = r#"
function make(n) { var o = new Object(); o.n = n; return o; }
var results = [];
results.push(make(1).n);
results.push(make(2).n);
results.push(make(3).n);
results.push(make(4).n);
results.push(make(5).n);
console.log(results.join(","));
"#;

fn run_with(program: &str, bytecode: bool, stress: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_jsse"));
    if bytecode {
        cmd.args(["--bytecode", "-e", program]);
    } else {
        cmd.args(["-e", program]);
    }
    cmd.env_remove("JSSE_GC_STRESS");
    if let Some(v) = stress {
        cmd.env("JSSE_GC_STRESS", v);
    }
    cmd.output().expect("failed to spawn jsse")
}

fn run(stress: Option<&str>) -> Output {
    run_with(PROGRAM, false, stress)
}

fn stdout(output: &Output) -> String {
    assert!(
        output.status.success(),
        "jsse failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn stress_collection_at_every_safepoint_preserves_program_results() {
    let baseline = stdout(&run(None));
    assert_eq!(baseline.trim(), "19900,6,2,4,[6,8] 1+2");
    for period in ["1", "2", "7"] {
        assert_eq!(
            stdout(&run(Some(period))),
            baseline,
            "JSSE_GC_STRESS={period}"
        );
    }
}

/// Direct regression coverage for issue #808: before statement-boundary
/// safepoints existed, `make`'s compiled body (no loop, so no back-edge
/// safepoint either) had no safepoint for the VM to reach, so `--bytecode`
/// stress could never collect while `o` was live inside it. The top-level
/// script itself already ran on the tree-walker (see the comment on
/// `PROGRAM_STRAIGHT_LINE`) and was already safepointed there regardless of
/// this PR; `bytecode/tests.rs::loop_free_allocating_calls_take_bytecode_path`
/// guards that `make`'s body is the part actually exercised under
/// `--bytecode`.
#[test]
fn bytecode_stress_collection_on_straight_line_program_preserves_results() {
    let baseline = stdout(&run_with(PROGRAM_STRAIGHT_LINE, true, None));
    assert_eq!(baseline.trim(), "1,2,3,4,5");
    for period in ["1", "2", "7"] {
        assert_eq!(
            stdout(&run_with(PROGRAM_STRAIGHT_LINE, true, Some(period))),
            baseline,
            "JSSE_GC_STRESS={period}"
        );
    }
}

#[test]
fn unparseable_or_zero_stress_value_leaves_the_mode_off() {
    let baseline = stdout(&run(None));
    for value in ["0", "", "abc", "-1"] {
        assert_eq!(
            stdout(&run(Some(value))),
            baseline,
            "JSSE_GC_STRESS={value:?}"
        );
    }
}
