//! Process metrics for Node host mode (issue #1227). One hidden global,
//! `__host_process`, holds the queries `process.memoryUsage`, `cpuUsage`,
//! `resourceUsage`, `umask`, `pid`/`ppid`/`platform`/`arch`/`title` and the
//! `performance` time origin are built on. `node-process-metrics.js` captures
//! it for `process`; `node-builtin-loader.js` then takes the global private as
//! the module `internal/host/process` (used by `perf_hooks`). Nothing here is
//! installed without `--node`. See `docs/adr/2026-10-26-1930-node-host-perf.md`.

use super::super::*;
use super::node_host_os::platform;
use super::node_host_os_sys as sys;

fn num(n: f64) -> JsValue {
    JsValue::number(n)
}

fn js_str(s: &str) -> JsValue {
    JsValue::string(JsString::from_str(s))
}

fn done(v: JsValue) -> Completion {
    Completion::Normal(v)
}

/// Linux `/proc/self/status` size in bytes (`VmRSS:  1234 kB`).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn status_bytes(text: &str, key: &str) -> Option<f64> {
    sys::parse_meminfo(text, key).map(|kb| kb as f64 * 1024.0)
}

/// `[rss, heapTotal, heapUsed]` in bytes. The "heap" is the process's
/// anonymous memory (the allocator's heap): `VmData` is what it has mapped and
/// `RssAnon` what is resident, so the numbers rise and fall with allocation.
fn memory_bytes() -> [f64; 3] {
    #[cfg(target_os = "linux")]
    if let Ok(text) = std::fs::read_to_string("/proc/self/status") {
        let rss = status_bytes(&text, "VmRSS");
        let total = status_bytes(&text, "VmData");
        let used = status_bytes(&text, "RssAnon");
        if let (Some(rss), Some(total), Some(used)) = (rss, total, used) {
            return [rss, total.max(used), used];
        }
    }
    let rss = rusage().map_or(0.0, |r| r[2] * 1024.0);
    [rss, rss, rss]
}

/// `getrusage(RUSAGE_SELF)` as Node's `resourceUsage()` order:
/// user and system CPU (microseconds), maxRSS (KiB), then the counters.
#[cfg(unix)]
fn rusage() -> Option<[f64; 16]> {
    // SAFETY: getrusage fills the zeroed struct it is given.
    let ru = unsafe {
        let mut ru: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut ru) != 0 {
            return None;
        }
        ru
    };
    let micros = |t: libc::timeval| t.tv_sec as f64 * 1e6 + t.tv_usec as f64;
    let max_rss = if cfg!(target_os = "macos") {
        ru.ru_maxrss as f64 / 1024.0
    } else {
        ru.ru_maxrss as f64
    };
    Some([
        micros(ru.ru_utime),
        micros(ru.ru_stime),
        max_rss,
        ru.ru_ixrss as f64,
        ru.ru_idrss as f64,
        ru.ru_isrss as f64,
        ru.ru_minflt as f64,
        ru.ru_majflt as f64,
        ru.ru_nswap as f64,
        ru.ru_inblock as f64,
        ru.ru_oublock as f64,
        ru.ru_msgsnd as f64,
        ru.ru_msgrcv as f64,
        ru.ru_nsignals as f64,
        ru.ru_nvcsw as f64,
        ru.ru_nivcsw as f64,
    ])
}

#[cfg(not(unix))]
fn rusage() -> Option<[f64; 16]> {
    None
}

#[cfg(unix)]
fn parent_pid() -> f64 {
    // SAFETY: getppid has no preconditions.
    f64::from(unsafe { libc::getppid() })
}

#[cfg(not(unix))]
fn parent_pid() -> f64 {
    0.0
}

/// Sets the file-mode creation mask and returns the previous one; `None`
/// only reads it (the mask can only be read by setting it, as Node does).
#[cfg(unix)]
fn umask(new: Option<u32>) -> u32 {
    // SAFETY: umask has no preconditions and cannot fail.
    unsafe {
        let old = libc::umask(new.unwrap_or(0) as libc::mode_t);
        if new.is_none() {
            libc::umask(old);
        }
        old as u32
    }
}

#[cfg(not(unix))]
fn umask(_new: Option<u32>) -> u32 {
    0
}

fn info(interp: &mut Interpreter, _t: &JsValue, _a: &[JsValue]) -> Completion {
    let title = interp.node_host.argv.as_ref().map_or("", |a| &a.argv0);
    let items = vec![
        num(f64::from(std::process::id())),
        num(parent_pid()),
        js_str(&platform(interp)),
        js_str(&sys::node_arch(std::env::consts::ARCH)),
        js_str(title),
        js_str(env!("CARGO_PKG_VERSION")),
    ];
    done(interp.create_array(items))
}

fn memory_usage(interp: &mut Interpreter, _t: &JsValue, _a: &[JsValue]) -> Completion {
    let [rss, total, used] = memory_bytes();
    let items = [rss, total, used, 0.0, 0.0].map(num).to_vec();
    done(interp.create_array(items))
}

fn rss(_i: &mut Interpreter, _t: &JsValue, _a: &[JsValue]) -> Completion {
    done(num(memory_bytes()[0]))
}

fn cpu_usage(interp: &mut Interpreter, _t: &JsValue, _a: &[JsValue]) -> Completion {
    let ru = rusage().unwrap_or_default();
    done(interp.create_array(vec![num(ru[0]), num(ru[1])]))
}

fn resource_usage(interp: &mut Interpreter, _t: &JsValue, _a: &[JsValue]) -> Completion {
    let items = rusage().unwrap_or_default().map(num).to_vec();
    done(interp.create_array(items))
}

fn umask_native(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let new = match args.first() {
        Some(v) if !v.is_undefined() => match interp.to_number_value(v) {
            Ok(n) => Some(n as u32 & 0o777),
            Err(e) => return Completion::Throw(e),
        },
        _ => None,
    };
    done(num(f64::from(umask(new))))
}

/// Microseconds since the Unix epoch at which the host floor started: the
/// `performance.timeOrigin` of this process.
fn time_origin(interp: &mut Interpreter, _t: &JsValue, _a: &[JsValue]) -> Completion {
    let elapsed = interp
        .node_host
        .clock_start
        .map_or(std::time::Duration::ZERO, |s| s.elapsed());
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    done(num(now.saturating_sub(elapsed).as_micros() as f64))
}

/// Milliseconds since the host floor started, with sub-microsecond
/// resolution: `performance.now()`.
fn now(interp: &mut Interpreter, _t: &JsValue, _a: &[JsValue]) -> Completion {
    let secs = interp
        .node_host
        .clock_start
        .map_or(0.0, |s| s.elapsed().as_secs_f64());
    done(num(secs * 1e3))
}

impl Interpreter {
    pub(super) fn setup_node_host_process_metrics(&mut self) {
        let binding = self.create_object_id();
        type Native = fn(&mut Interpreter, &JsValue, &[JsValue]) -> Completion;
        let functions: [(&str, usize, Native); 8] = [
            ("info", 0, info),
            ("memoryUsage", 0, memory_usage),
            ("rss", 0, rss),
            ("cpuUsage", 0, cpu_usage),
            ("resourceUsage", 0, resource_usage),
            ("umask", 1, umask_native),
            ("timeOrigin", 0, time_origin),
            ("now", 0, now),
        ];
        for (name, arity, native) in functions {
            let f = self.create_function(JsFunction::native(String::new(), arity, native));
            self.get_object_cell_expect(binding)
                .borrow_mut()
                .insert_value(name, f);
        }
        self.install_host_global("__host_process", JsValue::object(binding));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    fn run_as(platform: &'static str, source: &str) {
        crate::run_on_engine_stack(|| {
            let mut interp = Interpreter::new();
            interp.enable_node_host();
            interp.node_host.platform_override = Some(platform);
            interp.set_node_host_argv("jsse".into(), "/bin/jsse".into(), None, Vec::new());
            interp.load_node_prelude().expect("prelude loads");
            let program = Parser::new(source)
                .and_then(|mut p| p.parse_program())
                .expect("parse");
            let c = interp.run(&program);
            assert!(
                matches!(c, Completion::Normal(_) | Completion::Empty),
                "{platform}: {c:?}"
            );
        });
    }

    #[test]
    fn process_platform_follows_the_binding_and_selects_the_path_flavour() {
        for (platform, windows) in [("win32", true), ("darwin", false), ("linux", false)] {
            let source = format!(
                r#"
                const path = require("path");
                const want = {windows};
                if (process.platform !== "{platform}") throw new Error("platform " + process.platform);
                if ((path === path.win32) !== want) throw new Error("path flavour for {platform}");
                if (process.config.variables.host_arch !== process.arch) throw new Error("config");
                "#
            );
            run_as(platform, &source);
        }
    }

    #[test]
    fn status_sizes_are_kibibytes_scaled_to_bytes() {
        let text = "Name:\tjsse\nVmRSS:\t    2048 kB\nRssAnon:\t 512 kB\n";
        assert_eq!(status_bytes(text, "VmRSS"), Some(2048.0 * 1024.0));
        assert_eq!(status_bytes(text, "RssAnon"), Some(512.0 * 1024.0));
        assert_eq!(status_bytes(text, "VmData"), None);
    }

    #[cfg(unix)]
    #[test]
    fn rusage_reports_sixteen_counters_and_a_resident_set() {
        let ru = rusage().expect("getrusage");
        assert!(ru[2] > 0.0, "maxRSS");
        assert!(memory_bytes()[0] > 0.0);
    }
}
