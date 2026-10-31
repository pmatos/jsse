//! The `os` binding for Node host mode (issue #1222). One hidden global,
//! `__host_os`, holds functions shaped like Node's `internalBinding('os')`:
//! flat primitive arrays for lists, and a `ctx` object that a failing call
//! fills with `errno`/`code`/`message`/`syscall` before returning `undefined`.
//! `node-builtin-loader.js` captures the object, deletes the global and seeds it
//! as the private module `internal/host/os`; `node-lib/os.js` is the only
//! consumer. Nothing here is installed without `--node`. The OS environment is
//! never read: `HOME`/`TMPDIR` go through the prelude's `process.env` (see the
//! ADR). See `docs/adr/2026-10-26-0854-node-host-mode.md`.

use super::super::*;
use super::node_host_os_constants::{Table, host_constants};
use super::node_host_os_sys::{self as sys, errno};

fn js_str(s: &str) -> JsValue {
    JsValue::string(JsString::from_str(s))
}

fn num(n: f64) -> JsValue {
    JsValue::number(n)
}

fn done(v: JsValue) -> Completion {
    Completion::Normal(v)
}

/// libuv's `(code, message)` for the errnos these queries can report.
fn uv_error(errno: i32) -> (&'static str, &'static str) {
    match errno {
        errno::EPERM => ("EPERM", "operation not permitted"),
        errno::ENOENT => ("ENOENT", "no such file or directory"),
        errno::ESRCH => ("ESRCH", "no such process"),
        errno::ENOMEM => ("ENOMEM", "not enough memory"),
        errno::EACCES => ("EACCES", "permission denied"),
        errno::EINVAL => ("EINVAL", "invalid argument"),
        errno::ENAMETOOLONG => ("ENAMETOOLONG", "name too long"),
        errno::ENOSYS => ("ENOSYS", "function not implemented"),
        errno::ENOBUFS => ("ENOBUFS", "no buffer space available"),
        _ => ("UNKNOWN", "unknown error"),
    }
}

fn fail(interp: &mut Interpreter, ctx: Option<&JsValue>, syscall: &str, errno: i32) -> Completion {
    if let Some(id) = ctx.and_then(JsValue::as_object_id) {
        let (code, message) = uv_error(errno);
        let mut o = interp.get_object_cell_expect(id).borrow_mut();
        o.insert_value("errno", num(-f64::from(errno)));
        o.insert_value("code", js_str(code));
        o.insert_value("message", js_str(message));
        o.insert_value("syscall", js_str(syscall));
    }
    done(JsValue::UNDEFINED)
}

fn int_arg(interp: &mut Interpreter, args: &[JsValue], i: usize) -> Result<i32, JsValue> {
    let v = args.get(i).unwrap_or(JsValue::undefined_ref());
    interp.to_number_value(v).map(|n| n as i32)
}

fn pairs(table: Table) -> Vec<JsValue> {
    table
        .iter()
        .flat_map(|(name, value)| [js_str(name), num(f64::from(*value))])
        .collect()
}

pub(super) fn platform(interp: &Interpreter) -> String {
    #[cfg(test)]
    if let Some(p) = interp.node_host.platform_override {
        return p.to_string();
    }
    let _ = interp;
    sys::node_platform(std::env::consts::OS)
}

fn get_platform(interp: &mut Interpreter, _t: &JsValue, _a: &[JsValue]) -> Completion {
    done(js_str(&platform(interp)))
}

fn get_arch(_i: &mut Interpreter, _t: &JsValue, _a: &[JsValue]) -> Completion {
    done(js_str(&sys::node_arch(std::env::consts::ARCH)))
}

fn get_os_information(interp: &mut Interpreter, _t: &JsValue, _a: &[JsValue]) -> Completion {
    let [kind, version, release, machine] = sys::uname().unwrap_or_default();
    done(interp.create_array(vec![
        js_str(&kind),
        js_str(&version),
        js_str(&release),
        js_str(&machine),
    ]))
}

fn get_hostname(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    match sys::hostname() {
        Ok(name) => done(js_str(&name)),
        Err(e) => fail(interp, args.first(), "uv_os_gethostname", e),
    }
}

fn get_home_directory(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    match sys::passwd() {
        Ok(pw) => done(js_str(&String::from_utf8_lossy(&pw.dir))),
        Err(e) => fail(interp, args.first(), "uv_os_homedir", e),
    }
}

fn get_uptime(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    match sys::uptime() {
        Ok(secs) => done(num(secs)),
        Err(e) => fail(interp, args.first(), "uv_uptime", e),
    }
}

fn get_free_mem(_i: &mut Interpreter, _t: &JsValue, _a: &[JsValue]) -> Completion {
    done(num(sys::free_mem()))
}

fn get_total_mem(_i: &mut Interpreter, _t: &JsValue, _a: &[JsValue]) -> Completion {
    done(num(sys::total_mem()))
}

fn get_available_parallelism(_i: &mut Interpreter, _t: &JsValue, _a: &[JsValue]) -> Completion {
    done(num(sys::available_parallelism() as f64))
}

fn get_load_avg(interp: &mut Interpreter, _t: &JsValue, _a: &[JsValue]) -> Completion {
    let avg = sys::loadavg().iter().map(|n| num(*n)).collect();
    done(interp.create_array(avg))
}

fn get_cpus(interp: &mut Interpreter, _t: &JsValue, _a: &[JsValue]) -> Completion {
    let mut flat = Vec::new();
    for cpu in sys::cpus() {
        flat.push(js_str(&cpu.model));
        flat.push(num(cpu.speed));
        flat.extend(cpu.times.iter().map(|t| num(*t)));
    }
    done(interp.create_array(flat))
}

fn get_interface_addresses(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let addrs = match sys::interface_addresses() {
        Ok(a) => a,
        Err(e) => return fail(interp, args.first(), "uv_interface_addresses", e),
    };
    let mut flat = Vec::new();
    for a in addrs {
        flat.push(js_str(&a.name));
        flat.push(js_str(&a.address));
        flat.push(js_str(&a.netmask));
        flat.push(js_str(a.family));
        flat.push(js_str(&a.mac));
        flat.push(JsValue::boolean(a.internal));
        flat.push(num(a.scope_id as f64));
    }
    done(interp.create_array(flat))
}

fn text_or_bytes(interp: &mut Interpreter, bytes: &[u8], raw: bool) -> JsValue {
    if raw {
        interp.create_array(bytes.iter().map(|b| num(f64::from(*b))).collect())
    } else {
        js_str(&String::from_utf8_lossy(bytes))
    }
}

fn get_user_info(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let pw = match sys::passwd() {
        Ok(pw) => pw,
        Err(e) => return fail(interp, args.first(), "uv_os_get_passwd", e),
    };
    let raw = args.get(1).is_some_and(|v| interp.to_boolean_val(v));
    let username = text_or_bytes(interp, &pw.name, raw);
    let homedir = text_or_bytes(interp, &pw.dir, raw);
    let shell = pw
        .shell
        .as_deref()
        .map_or(JsValue::NULL, |s| text_or_bytes(interp, s, raw));
    let id = interp.create_object_id();
    let mut o = interp.get_object_cell_expect(id).borrow_mut();
    o.insert_value("uid", num(f64::from(pw.uid)));
    o.insert_value("gid", num(f64::from(pw.gid)));
    o.insert_value("username", username);
    o.insert_value("homedir", homedir);
    o.insert_value("shell", shell);
    drop(o);
    done(JsValue::object(id))
}

fn get_priority(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let pid = match int_arg(interp, args, 0) {
        Ok(n) => n,
        Err(e) => return Completion::Throw(e),
    };
    match sys::get_priority(pid) {
        Ok(p) => done(num(f64::from(p))),
        Err(e) => fail(interp, args.get(1), "uv_os_getpriority", e),
    }
}

fn set_priority(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let (pid, priority) = match (int_arg(interp, args, 0), int_arg(interp, args, 1)) {
        (Ok(p), Ok(q)) => (p, q),
        (Err(e), _) | (_, Err(e)) => return Completion::Throw(e),
    };
    match sys::set_priority(pid, priority) {
        Ok(()) => done(num(0.0)),
        Err(e) => {
            fail(interp, args.get(2), "uv_os_setpriority", e);
            done(num(-1.0))
        }
    }
}

fn get_constants(interp: &mut Interpreter, _t: &JsValue, _a: &[JsValue]) -> Completion {
    let c = host_constants();
    let lists = [c.signals, c.errno, c.dlopen]
        .map(|t| interp.create_array(pairs(t)))
        .to_vec();
    done(interp.create_array(lists))
}

impl Interpreter {
    pub(super) fn setup_node_host_os(&mut self) {
        let binding = self.create_object_id();
        type Native = fn(&mut Interpreter, &JsValue, &[JsValue]) -> Completion;
        let functions: [(&str, usize, Native); 15] = [
            ("getPlatform", 0, get_platform),
            ("getArch", 0, get_arch),
            ("getOSInformation", 0, get_os_information),
            ("getHostname", 1, get_hostname),
            ("getHomeDirectory", 1, get_home_directory),
            ("getUptime", 1, get_uptime),
            ("getFreeMem", 0, get_free_mem),
            ("getTotalMem", 0, get_total_mem),
            ("getAvailableParallelism", 0, get_available_parallelism),
            ("getLoadAvg", 0, get_load_avg),
            ("getCPUs", 0, get_cpus),
            ("getInterfaceAddresses", 1, get_interface_addresses),
            ("getUserInfo", 2, get_user_info),
            ("getPriority", 2, get_priority),
            ("setPriority", 3, set_priority),
        ];
        for (name, arity, native) in functions {
            let f = self.create_function(JsFunction::native(String::new(), arity, native));
            self.get_object_cell_expect(binding)
                .borrow_mut()
                .insert_value(name, f);
        }
        let constants = self.create_function(JsFunction::native(String::new(), 0, get_constants));
        let mut o = self.get_object_cell_expect(binding).borrow_mut();
        o.insert_value("getConstants", constants);
        o.insert_value("isBigEndian", JsValue::boolean(cfg!(target_endian = "big")));
        drop(o);
        self.install_host_global("__host_os", JsValue::object(binding));
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

    const CHECK: &str = r#"
        function check(got, want) {
          if (JSON.stringify(got) !== JSON.stringify(want)) {
            throw new Error(JSON.stringify(got) + " != " + JSON.stringify(want));
          }
        }
    "#;

    #[test]
    fn windows_paths_and_line_endings() {
        let source = format!(
            r#"{CHECK}
            for (const k of ["TEMP", "TMP", "SystemRoot", "windir"]) delete process.env[k];
            process.env.USERPROFILE = "C:\\Users\\me";
            process.env.SystemRoot = "C:\\Windows";
            const os = require("os");
            check([os.platform(), os.EOL, os.devNull], ["win32", "\r\n", "\\\\.\\nul"]);
            check(os.homedir(), "C:\\Users\\me");
            check(os.tmpdir(), "C:\\Windows\\temp");
            process.env.TMP = "D:\\tmp\\";
            check(os.tmpdir(), "D:\\tmp");
            process.env.TMP = "E:\\";
            check(os.tmpdir(), "E:\\");
            process.env.TEMP = "F:\\temp";
            check(os.tmpdir(), "F:\\temp");
            check(`${{os.platform}}`, "win32");
            "#
        );
        run_as("win32", &source);
    }

    #[test]
    fn darwin_and_linux_share_unix_line_endings_and_env() {
        let source = format!(
            r#"{CHECK}
            process.env.HOME = "/Users/me";
            process.env.TMPDIR = "/var/folders/xy/T/";
            const os = require("os");
            check([os.EOL, os.devNull, os.homedir(), os.tmpdir()], ["\n", "/dev/null", "/Users/me", "/var/folders/xy/T"]);
            "#
        );
        for platform in ["darwin", "linux", "freebsd"] {
            let source = format!("{source}\ncheck(require('os').platform(), {platform:?});");
            run_as(platform, &source);
        }
    }

    #[test]
    fn homedir_reads_the_prelude_environment_not_the_os_one() {
        run_as(
            "linux",
            &format!(
                r#"{CHECK}
                const os = require("os");
                process.env.HOME = "/edited";
                check(os.homedir(), "/edited");
                delete process.env.HOME;
                check(os.homedir(), os.userInfo().homedir);
                process.env.HOME = "";
                check(os.homedir(), "");
                "#
            ),
        );
    }

    #[test]
    fn binding_failures_become_system_errors() {
        run_as(
            "linux",
            &format!(
                r#"{CHECK}
                const os = require("os");
                try {{ os.getPriority(99999999); throw new Error("no throw"); }}
                catch (e) {{
                  check([e.code, e.name, e.errno, e.syscall, e.info.code],
                        ["ERR_SYSTEM_ERROR", "SystemError", -3, "uv_os_getpriority", "ESRCH"]);
                  check(e.message, "A system error occurred: uv_os_getpriority returned ESRCH (no such process)");
                }}
                "#
            ),
        );
    }

    #[test]
    fn binding_is_private_to_the_module_system() {
        run_as(
            "linux",
            r#"
            if (typeof __host_os !== "undefined") throw new Error("global left behind");
            try { require("internal/host/os"); throw new Error("reachable"); }
            catch (e) { if (e.code !== "MODULE_NOT_FOUND") throw e; }
            "#,
        );
    }
}
