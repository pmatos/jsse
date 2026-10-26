//! The `process` metrics and `perf_hooks` module under `--node` (#1227),
//! reached through `require`. Moving values are checked by shape and
//! monotonicity; `scripts/run-node-perf-oracle.sh` diffs the same surface
//! against Node.

use std::process::{Command, Output};

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

fn node_with(source: &str, flags: &[&str], gc_stress: Option<&str>) -> String {
    let mut args = vec!["--node"];
    args.extend_from_slice(flags);
    args.extend(["-e", source]);
    let out = run(&args, gc_stress);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    stdout(&out)
}

fn node(source: &str) -> String {
    node_with(source, &[], None)
}

#[test]
fn identity_matches_the_host() {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        other => other,
    };
    let platform = match std::env::consts::OS {
        "macos" => "darwin",
        "windows" => "win32",
        other => other,
    };
    let source = r#"
      print([process.platform, process.arch, typeof process.pid, process.title === process.argv0,
        process.version === "v" + process.versions.node, typeof process.versions.jsse,
        process.release.name, process.config.variables.target_arch === process.arch].join(" "));
    "#;
    assert_eq!(
        node(source),
        format!("{platform} {arch} number true true string node true")
    );
}

#[cfg(unix)]
#[test]
fn ppid_is_the_launching_process() {
    let out = node("print(process.ppid, process.pid !== process.ppid)");
    assert_eq!(out, format!("{} true", std::process::id()));
}

#[test]
fn clocks_are_monotonic_and_plausible() {
    let source = r#"
      const { performance } = require("perf_hooks");
      const a = process.hrtime.bigint();
      const t = process.hrtime();
      const n1 = performance.now();
      let x = 0;
      for (let i = 0; i < 1e5; i++) x += i;
      const n2 = performance.now();
      const d = process.hrtime(t);
      const b = process.hrtime.bigint();
      print([b > a, n2 > n1, n1 > 0, d[0] >= 0 && d[1] >= 0, process.uptime() > 0,
        Math.abs(performance.timeOrigin + performance.now() - Date.now()) < 100,
        Math.abs(performance.now() - Number(process.hrtime.bigint()) / 1e6) < 50].join(" "));
    "#;
    for flags in [&[][..], &["--no-bytecode"][..]] {
        assert_eq!(
            node_with(source, flags, None),
            "true true true true true true true"
        );
    }
}

#[test]
fn memory_and_cpu_usage_have_node_shapes() {
    let source = r#"
      const m = process.memoryUsage();
      const c1 = process.cpuUsage();
      let x = 0;
      for (let i = 0; i < 1e6; i++) x += i;
      const c2 = process.cpuUsage(c1);
      const r = process.resourceUsage();
      print(JSON.stringify([Object.keys(m).join(), m.rss > 1e6, m.heapUsed <= m.heapTotal,
        process.memoryUsage.rss() > 1e6, c2.user >= 0, c2.system >= 0,
        Object.keys(r).length, r.maxRSS > 0]));
    "#;
    assert_eq!(
        node(source),
        r#"["rss,heapTotal,heapUsed,external,arrayBuffers",true,true,true,true,true,16,true]"#
    );
}

#[cfg(unix)]
#[test]
fn umask_round_trips() {
    let source = r#"
      const old = process.umask();
      const a = process.umask(0o77);
      const b = process.umask("022");
      const c = process.umask();
      process.umask(old);
      print([a === old, b, c, process.umask() === old].join(" "));
    "#;
    assert_eq!(node(source), "true 63 18 true");
}

#[test]
fn argument_errors_are_node_errors() {
    let source = r#"
      const probe = (fn) => { try { fn(); } catch (e) { print(e.name, e.code, e.message); } };
      probe(() => process.hrtime([1]));
      probe(() => process.hrtime("x"));
      probe(() => process.cpuUsage({ user: -1, system: 0 }));
      probe(() => process.umask("9"));
    "#;
    assert_eq!(
        node(source),
        "RangeError ERR_OUT_OF_RANGE The value of \"time\" is out of range. It must be 2. Received 1\n\
TypeError ERR_INVALID_ARG_TYPE The \"time\" argument must be an instance of Array. Received type string ('x')\n\
RangeError ERR_INVALID_ARG_VALUE The property 'prevValue.user' is invalid. Received -1\n\
TypeError ERR_INVALID_ARG_VALUE The argument 'mask' must be a 32-bit unsigned integer or an octal string. Received '9'"
    );
}

#[test]
fn user_timing_and_observers() {
    let source = r#"
      const { performance, PerformanceObserver } = require("node:perf_hooks");
      const seen = [];
      const obs = new PerformanceObserver((list) => {
        seen.push(list.getEntries().map((e) => e.entryType + ":" + e.name).join());
        obs.disconnect();
      });
      obs.observe({ entryTypes: ["mark", "measure"] });
      performance.mark("a");
      performance.mark("b", { startTime: performance.now() + 5 });
      const m = performance.measure("a-b", "a", "b");
      print(m.duration >= 5, m.entryType, performance.getEntriesByName("a")[0].name);
      try { performance.measure("x", "missing"); } catch (e) { print(e.name, e.code, e.message); }
      setImmediate(() => print(seen.join("|")));
    "#;
    assert_eq!(
        node(source),
        "true measure a\nSyntaxError 12 The \"missing\" performance mark has not been set\nmark:a,measure:a-b,mark:b"
    );
}

#[test]
fn histograms_follow_hdr_arithmetic() {
    let source = r#"
      const { createHistogram } = require("perf_hooks");
      const h = createHistogram();
      for (const v of [1, 2, 3, 4, 5, 100, 2000, 2001, 100000, 1234567]) h.record(v);
      print(h.min, h.max, h.mean, h.stddev, h.count, h.percentile(50));
      print(JSON.stringify([...h.percentiles]));
      const empty = createHistogram();
      print(empty.min, empty.max, empty.mean, JSON.stringify([...empty.percentiles]));
    "#;
    assert_eq!(
        node(source),
        "1 1234943 133854.8 368056.96595087013 10 5\n\
[[0,1],[50,5],[75,2001],[87.5,99968],[93.75,1233920],[100,1233920]]\n\
9223372036854776000 0 NaN [[100,0]]"
    );
}

#[test]
fn event_loop_delay_samples_without_keeping_the_loop_alive() {
    let source = r#"
      const { monitorEventLoopDelay } = require("perf_hooks");
      const h = monitorEventLoopDelay({ resolution: 5 });
      print(h.enable(), h.enable());
      setTimeout(() => print(h.disable(), h.count > 0, h.min > 0), 40);
      const idle = monitorEventLoopDelay();
      idle.enable();
    "#;
    assert_eq!(node(source), "true false\ntrue true true");
}

#[test]
fn performance_globals_are_lazy_and_internals_stay_private() {
    let source = r#"
      const names = ["performance", "PerformanceObserver", "PerformanceEntry", "PerformanceMark",
        "PerformanceMeasure", "PerformanceObserverEntryList", "PerformanceResourceTiming"];
      const enumerable = names.filter((n) => Object.keys(globalThis).includes(n));
      const probe = (id) => { try { require(id); return "loaded"; } catch (e) { return e.code; } };
      print(typeof __host_process, enumerable.length, globalThis.performance === require("perf_hooks").performance,
        probe("internal/host/process"), probe("internal/perf/binding"), probe("perf_hooks"));
    "#;
    assert_eq!(
        node(source),
        "undefined 0 true MODULE_NOT_FOUND MODULE_NOT_FOUND loaded"
    );
}

#[test]
fn default_environment_has_no_node_surface() {
    let out = run(
        &[
            "-e",
            "print(typeof process, typeof __host_process, typeof require, typeof PerformanceObserver)",
        ],
        None,
    );
    assert!(out.status.success());
    assert_eq!(stdout(&out), "undefined undefined undefined undefined");
}

#[test]
fn survives_gc_stress() {
    let source = r#"
      const { performance, createHistogram } = require("perf_hooks");
      performance.mark("a");
      const h = createHistogram();
      for (let i = 1; i < 50; i++) h.record(i);
      const m = performance.measure("m", "a");
      print(h.count, h.max, m.entryType, process.memoryUsage().rss > 0, process.cpuUsage().user >= 0);
    "#;
    for flags in [&[][..], &["--no-bytecode"][..]] {
        assert_eq!(
            node_with(source, flags, Some("3")),
            "49 49 measure true true"
        );
    }
}
