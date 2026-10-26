//! The embedded `os` module under `--node` (#1222), reached through the hidden
//! `__node_builtin` accessor until the CommonJS loader (#1219) adds `require`.
//! Values are checked against what the process can see itself (`/proc`, the
//! environment it was started with); `scripts/run-node-os-oracle.sh` diffs the
//! same module against Node.

use std::process::{Command, Output};

fn run(args: &[&str], envs: &[(&str, &str)], gc_stress: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_jsse"));
    if let Some(period) = gc_stress {
        cmd.env("JSSE_GC_STRESS", period);
    }
    cmd.envs(envs.iter().copied()).args(args);
    cmd.output().expect("run jsse")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).trim_end().to_string()
}

fn node(source: &str, envs: &[(&str, &str)], flags: &[&str], gc_stress: Option<&str>) -> String {
    let mut args = vec!["--node"];
    args.extend_from_slice(flags);
    args.extend(["-e", source]);
    let out = run(&args, envs, gc_stress);
    assert!(out.status.success(), "{}", stderr(&out));
    stdout(&out)
}

#[test]
fn module_surface_and_aliases() {
    let source = r#"
      const os = __node_builtin("os");
      print([
        os === __node_builtin("node:os"),
        Object.keys(os).join(),
        typeof os.constants.signals.SIGINT,
        Object.isFrozen(os.constants.signals),
        Object.getOwnPropertyDescriptor(os, "constants").writable,
        Object.getOwnPropertyDescriptor(os, "EOL").writable,
        `${os.platform}` === os.platform(),
        `${os.endianness}` === os.endianness(),
      ].join("|"));
    "#;
    for flags in [&[][..], &["--no-bytecode"][..]] {
        assert_eq!(
            node(source, &[], flags, None),
            "true|arch,availableParallelism,cpus,endianness,freemem,getPriority,homedir,hostname,loadavg,networkInterfaces,platform,release,setPriority,tmpdir,totalmem,type,userInfo,uptime,version,machine,constants,EOL,devNull|number|true|false|false|true|true"
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn values_agree_with_what_the_process_can_see() {
    let source = r#"
      const os = __node_builtin("os");
      print(JSON.stringify([
        os.platform(), os.arch(), os.type(), os.endianness(), os.EOL, os.devNull,
        os.homedir(), os.tmpdir(), os.hostname(), os.userInfo().username.length > 0,
        os.availableParallelism() >= 1, os.freemem() > 0, os.freemem() <= os.totalmem(),
        os.uptime() > 0, os.loadavg().length, os.cpus().length >= 1,
      ]));
    "#;
    let hostname = std::fs::read_to_string("/proc/sys/kernel/hostname").expect("hostname");
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => other,
    };
    let endian = if cfg!(target_endian = "big") {
        "BE"
    } else {
        "LE"
    };
    let want = format!(
        "[\"linux\",\"{arch}\",\"Linux\",\"{endian}\",\"\\n\",\"/dev/null\",\"/home/x\",\"/scratch\",\"{}\",true,true,true,true,true,3,true]",
        hostname.trim_end()
    );
    let envs = [("HOME", "/home/x"), ("TMPDIR", "/scratch/")];
    assert_eq!(node(source, &envs, &[], None), want);
}

#[cfg(target_os = "linux")]
#[test]
fn memory_and_release_come_from_proc() {
    let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap();
    let total_kb: u64 = meminfo
        .lines()
        .find_map(|l| l.strip_prefix("MemTotal:"))
        .and_then(|r| r.split_whitespace().next())
        .and_then(|n| n.parse().ok())
        .expect("MemTotal");
    let release = std::fs::read_to_string("/proc/sys/kernel/osrelease").unwrap();
    let source = r#"
      const os = __node_builtin("os");
      print(os.totalmem(), os.release(), os.cpus().length === os.availableParallelism());
    "#;
    assert_eq!(
        node(source, &[], &[], None),
        format!("{} {} true", total_kb * 1024, release.trim_end())
    );
}

#[test]
fn environment_edits_are_seen_at_call_time() {
    let source = r#"
      const os = __node_builtin("os");
      const out = [os.homedir(), os.tmpdir()];
      process.env.HOME = "/edited";
      process.env.TMPDIR = "";
      process.env.TMP = "/tmp-edited//";
      out.push(os.homedir(), os.tmpdir());
      delete process.env.HOME;
      out.push(os.homedir() === os.userInfo().homedir);
      print(out.join(" "));
    "#;
    assert_eq!(
        node(source, &[("HOME", "/h"), ("TMPDIR", "/t")], &[], None),
        "/h /t /edited /tmp-edited/ true"
    );
}

#[test]
fn errors_and_validation_match_node_messages() {
    let source = r#"
      const os = __node_builtin("os");
      const probe = (fn) => { try { fn(); } catch (e) { print(e.name, e.code, e.message); } };
      probe(() => os.getPriority("a"));
      probe(() => os.setPriority(0, 100));
      probe(() => os.setPriority(0, 1.5));
      probe(() => os.setPriority(2 ** 33));
      probe(() => os.getPriority(99999999));
    "#;
    assert_eq!(
        node(source, &[], &[], None),
        "TypeError ERR_INVALID_ARG_TYPE The \"pid\" argument must be of type number. Received type string ('a')\n\
         RangeError ERR_OUT_OF_RANGE The value of \"priority\" is out of range. It must be >= -20 && <= 19. Received 100\n\
         RangeError ERR_OUT_OF_RANGE The value of \"priority\" is out of range. It must be an integer. Received 1.5\n\
         RangeError ERR_OUT_OF_RANGE The value of \"priority\" is out of range. It must be >= -20 && <= 19. Received 8_589_934_592\n\
         SystemError ERR_SYSTEM_ERROR A system error occurred: uv_os_getpriority returned ESRCH (no such process)"
    );
}

#[test]
fn works_under_gc_stress() {
    let source = r#"
      const os = __node_builtin("os");
      print(Object.keys(os.networkInterfaces()).length > 0, os.cpus().length > 0, os.userInfo({ encoding: "buffer" }).username.length > 0, Object.keys(os.constants.errno).length > 0);
    "#;
    assert_eq!(node(source, &[], &[], Some("10")), "true true true true");
}

#[test]
fn binding_is_private_and_absent_without_node() {
    let probe = r#"print(typeof __host_os, typeof __node_builtin)"#;
    assert_eq!(
        stdout(&run(&["-e", probe], &[], None)),
        "undefined undefined"
    );
    let source = r#"
      print(typeof __host_os);
      try { __node_builtin("internal/host/os"); } catch (e) { print(e.code); }
      try { __node_builtin("os/nope"); } catch (e) { print(e.code); }
    "#;
    assert_eq!(
        node(source, &[], &[], None),
        "undefined\nERR_UNKNOWN_BUILTIN_MODULE\nERR_UNKNOWN_BUILTIN_MODULE"
    );
}
