//! Node host mode `process` (#1215): script-argument splitting, `process.argv`/
//! `env`/`cwd`/`chdir`/`exitCode`/`exit` built on the `__host_*` floor, and
//! exit-status precedence. Child-process checks, because the behaviour is the
//! real argv and exit status. Under `--node` the embedded prelude (#1216) owns
//! the floor primitives, so these go through `process`.

use std::process::{Command, Output};

fn jsse(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jsse"))
        .args(args)
        .output()
        .expect("run jsse")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

fn script(name: &str, source: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("jsse-node-host-process-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    std::fs::write(&path, source).unwrap();
    path
}

const PRINT_ARGV: &str = "print(JSON.stringify(process.argv.slice(2)))";
const PRINT_EVAL_ARGV: &str = "print(JSON.stringify(process.argv.slice(1)))";

fn argv_tail(flags_and_script: &[&str]) -> String {
    let out = jsse(flags_and_script);
    assert!(
        out.status.success(),
        "{flags_and_script:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    stdout(&out)
}

#[test]
fn script_args_pass_through_verbatim() {
    let s = script("argv.js", PRINT_ARGV);
    let s = s.to_str().unwrap();
    assert_eq!(
        argv_tail(&["--node", s, "--", "a", "b", "c", "--version"]),
        r#"["--","a","b","c","--version"]"#
    );
    assert_eq!(argv_tail(&["--node", s, "--version"]), r#"["--version"]"#);
    assert_eq!(
        argv_tail(&["--node", s, "-e", "foo", "--no-bytecode", "--node", "-m"]),
        r#"["-e","foo","--no-bytecode","--node","-m"]"#
    );
    assert_eq!(argv_tail(&["--node", s]), "[]");
}

#[test]
fn leading_double_dash_ends_jsse_options_and_is_consumed() {
    let s = script("argv-dd.js", PRINT_ARGV);
    let s = s.to_str().unwrap();
    assert_eq!(argv_tail(&["--node", "--", s, "--", "z"]), r#"["--","z"]"#);
}

#[test]
fn options_before_the_script_still_apply() {
    let s = script("argv-opts.js", PRINT_ARGV);
    let s = s.to_str().unwrap();
    assert_eq!(
        argv_tail(&["--no-bytecode", "--node", "--step-budget", "100000", s, "x"]),
        r#"["x"]"#
    );
    assert_eq!(
        argv_tail(&["--step-budget=100000", "--node", s, "--step-budget", "1"]),
        r#"["--step-budget","1"]"#
    );
}

#[test]
fn eval_takes_leftover_tokens_as_script_args() {
    assert_eq!(
        argv_tail(&["--node", "-e", PRINT_EVAL_ARGV, "a", "--b", "--", "c"]),
        r#"["a","--b","--","c"]"#
    );
    assert_eq!(
        argv_tail(&["--node", "-e", PRINT_EVAL_ARGV, "--", "--x"]),
        r#"["--x"]"#
    );
    assert_eq!(
        argv_tail(&["--node", &format!("-e{PRINT_EVAL_ARGV}"), "a"]),
        r#"["a"]"#
    );
}

#[test]
fn jsse_options_before_the_script_are_still_handled_by_clap() {
    let out = jsse(&["--version"]);
    assert!(stdout(&out).starts_with("jsse "));
    let s = script("argv-version.js", PRINT_ARGV);
    let out = jsse(&["--node", s.to_str().unwrap(), "--version"]);
    assert_eq!(stdout(&out), r#"["--version"]"#);
}

#[test]
fn argv_shape_follows_node() {
    let s = script(
        "argv-shape.js",
        "var a = process.argv; print(a.length, a[0].charAt(0), a[1].split('/').pop(), process.argv0.length > 0)",
    );
    let out = jsse(&["--node", s.to_str().unwrap(), "x"]);
    assert_eq!(stdout(&out), "3 / argv-shape.js true");
    let out = jsse(&["--node", "-e", "print(process.argv.length)"]);
    assert_eq!(stdout(&out), "1");
}

#[test]
fn script_path_is_absolute_and_lexically_normalised() {
    let s = script("argv-path.js", "print(process.argv[1])");
    let dir = s.parent().unwrap();
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .current_dir(dir)
        .args(["--node", "./sub/../argv-path.js"])
        .output()
        .unwrap();
    assert_eq!(stdout(&out), s.to_string_lossy());
}

#[test]
fn process_env_reflects_the_os_environment() {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .env("JSSE_NODE_HOST_PROBE", "va lue")
        .args([
            "--node",
            "-e",
            "print(process.env.JSSE_NODE_HOST_PROBE, Object.keys(process.env).indexOf('JSSE_NODE_HOST_PROBE') >= 0)",
        ])
        .output()
        .unwrap();
    assert_eq!(stdout(&out), "va lue true");
}

#[test]
fn chdir_and_cwd_round_trip_and_report_node_style_errors() {
    let dir = std::fs::canonicalize(std::env::temp_dir()).unwrap();
    let source = format!(
        r#"
        process.chdir({dir:?});
        var ok = process.cwd();
        var e;
        try {{ process.chdir("/nonexistent-jsse-dir"); }} catch (x) {{ e = x; }}
        print(ok, e.code, e.errno, e.syscall, e.path === ok, e.dest);
        print(e.message === "ENOENT: no such file or directory, chdir '" + ok + "' -> '/nonexistent-jsse-dir'");
        print(process.cwd() === ok);
        "#,
        dir = dir.to_string_lossy()
    );
    let out = jsse(&["--node", "-e", &source]);
    let text = stdout(&out);
    let mut lines = text.lines();
    assert_eq!(
        lines.next().unwrap(),
        format!(
            "{} ENOENT -2 chdir true /nonexistent-jsse-dir",
            dir.to_string_lossy()
        )
    );
    assert_eq!(lines.next(), Some("true"));
    assert_eq!(lines.next(), Some("true"));
}

fn exit_status(source: &str) -> i32 {
    jsse(&["--node", "-e", source]).status.code().unwrap()
}

#[test]
fn exit_code_sets_the_process_status() {
    assert_eq!(exit_status("process.exitCode = 3"), 3);
    assert_eq!(exit_status("process.exitCode = 300"), 44);
    assert_eq!(exit_status("process.exitCode = -1"), 255);
    assert_eq!(
        exit_status("process.exitCode = 3; process.exitCode = undefined"),
        0
    );
    assert_eq!(
        exit_status("setTimeout(function () { process.exitCode = 5; }, 5)"),
        5
    );
    assert_eq!(
        exit_status("Promise.resolve().then(function () { process.exitCode = 6; })"),
        6
    );
}

#[test]
fn exit_status_precedence() {
    assert_eq!(exit_status("process.exitCode = 4; throw new Error('x')"), 1);
    assert_eq!(exit_status("process.exitCode = 4; process.exit(0)"), 0);
    assert_eq!(exit_status("process.exitCode = 4; process.exit(9)"), 9);
    assert_eq!(exit_status("process.exitCode = 4; process.exit()"), 4);
    assert_eq!(
        exit_status("process.exitCode = 4; try { process.exit(9) } catch (e) {} finally {}"),
        9
    );
}

#[test]
fn floor_primitives_absent_without_node_flag() {
    let out = jsse(&[
        "-e",
        "print([typeof __host_argv, typeof __host_argv0, typeof __host_env_snapshot, typeof __host_cwd, typeof __host_chdir, typeof __host_set_exit_code].join())",
    ]);
    assert_eq!(
        stdout(&out),
        "undefined,undefined,undefined,undefined,undefined,undefined"
    );
    assert_eq!(
        jsse(&[
            "-e",
            "typeof __host_set_exit_code === 'undefined' || (() => { throw 1 })()"
        ])
        .status
        .code(),
        Some(0)
    );
}

#[test]
fn prelude_owns_the_process_floor_primitives() {
    let out = jsse(&[
        "--node",
        "-e",
        "print(['__host_argv','__host_argv0','__host_env_snapshot','__host_cwd','__host_chdir','__host_set_exit_code'].map(function (n) { return typeof globalThis[n]; }).join())",
    ]);
    assert_eq!(stdout(&out), vec!["undefined"; 6].join(","));
}

#[test]
fn long_eval_with_equals_takes_leftover_tokens_as_script_args() {
    assert_eq!(
        argv_tail(&["--node", &format!("--eval={PRINT_EVAL_ARGV}"), "a", "-e"]),
        r#"["a","-e"]"#
    );
}
