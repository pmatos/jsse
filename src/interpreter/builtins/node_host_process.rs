//! `process` floor primitives for Node host mode (issue #1215): launch
//! arguments, an environment snapshot, the working directory and the
//! `process.exitCode` sink. The JS prelude builds `process.argv`, `env`,
//! `cwd()`, `chdir()` and `exitCode` on these; see
//! `docs/adr/2026-10-26-0854-node-host-mode.md`.

use super::super::*;
use std::path::{Component, Path, PathBuf};

/// The program a `--node` launch runs in place of the script file: the
/// prelude's CommonJS loader reads `process.argv[1]` and runs it as the main
/// module. The entry point is a hidden global that is taken private here.
const NODE_MAIN_BOOTSTRAP: &str = "(function () {\n  var run = globalThis.__node_run_main;\n  delete globalThis.__node_run_main;\n  run();\n})();\n//# sourceURL=node:internal/main/run_main_module";

impl Interpreter {
    pub(crate) fn node_main_bootstrap() -> &'static str {
        NODE_MAIN_BOOTSTRAP
    }

    pub(super) fn setup_node_host_process(&mut self) {
        // __host_argv() -> [execPath, absScriptPath?, ...args]
        let argv_fn = self.create_function(JsFunction::native(
            "__host_argv".to_string(),
            0,
            |interp, _this, _args| {
                let mut items = Vec::new();
                if let Some(argv) = &interp.node_host.argv {
                    items.push(js_str(&argv.exec_path));
                    if let Some(script) = &argv.script {
                        items.push(js_str(&absolute_lexical(script).to_string_lossy()));
                    }
                    items.extend(argv.args.iter().map(|a| js_str(a)));
                }
                Completion::Normal(interp.create_array(items))
            },
        ));
        self.install_host_global("__host_argv", argv_fn);

        // __host_main_kind() -> "cjs" | "esm" | "eval": how the launcher runs
        // the main program (see `Interpreter::node_main_bootstrap`)
        let main_kind_fn = self.create_function(JsFunction::native(
            "__host_main_kind".to_string(),
            0,
            |interp, _this, _args| Completion::Normal(js_str(interp.node_host.main_kind.as_str())),
        ));
        self.install_host_global("__host_main_kind", main_kind_fn);

        // __host_argv0() -> the launcher's original argv[0]
        let argv0_fn = self.create_function(JsFunction::native(
            "__host_argv0".to_string(),
            0,
            |interp, _this, _args| {
                let argv0 = interp.node_host.argv.as_ref().map_or("", |a| &a.argv0);
                Completion::Normal(js_str(argv0))
            },
        ));
        self.install_host_global("__host_argv0", argv0_fn);

        // __host_env_snapshot() -> [k0, v0, k1, v1, ...]
        //
        // The OS environment at call time. The environment is never written
        // back: `process.env` is a JS-owned object (see the ADR).
        let env_fn = self.create_function(JsFunction::native(
            "__host_env_snapshot".to_string(),
            0,
            |interp, _this, _args| {
                let mut items = Vec::new();
                for (k, v) in std::env::vars_os() {
                    items.push(js_str(&k.to_string_lossy()));
                    items.push(js_str(&v.to_string_lossy()));
                }
                Completion::Normal(interp.create_array(items))
            },
        ));
        self.install_host_global("__host_env_snapshot", env_fn);

        // __host_cwd() -> string
        let cwd_fn = self.create_function(JsFunction::native(
            "__host_cwd".to_string(),
            0,
            |interp, _this, _args| match std::env::current_dir() {
                Ok(dir) => Completion::Normal(js_str(&dir.to_string_lossy())),
                Err(e) => Completion::Throw(interp.create_error("Error", &e.to_string())),
            },
        ));
        self.install_host_global("__host_cwd", cwd_fn);

        // __host_chdir(dir)
        //
        // Throws an Error carrying Node's `errno`/`code`/`syscall`/`path`/`dest`.
        let chdir_fn = self.create_function(JsFunction::native(
            "__host_chdir".to_string(),
            1,
            |interp, _this, args| {
                let dest = match interp
                    .to_string_lossy(args.first().unwrap_or(JsValue::undefined_ref()))
                {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                let Err(err) = std::env::set_current_dir(&dest) else {
                    return Completion::Normal(JsValue::UNDEFINED);
                };
                let cwd = std::env::current_dir()
                    .map(|d| d.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let (code, desc, errno) = errno_info(&err);
                let msg = format!("{code}: {desc}, chdir '{cwd}' -> '{dest}'");
                let error = interp.create_error("Error", &msg);
                if let Some(id) = error.as_object_id() {
                    let mut o = interp.get_object_cell_expect(id).borrow_mut();
                    let props = [
                        ("errno", JsValue::number(f64::from(-errno))),
                        ("code", js_str(code)),
                        ("syscall", js_str("chdir")),
                        ("path", js_str(&cwd)),
                        ("dest", js_str(&dest)),
                    ];
                    for (name, value) in props {
                        o.insert_property(
                            name.to_string(),
                            PropertyDescriptor::data(value, true, true, true),
                        );
                    }
                }
                Completion::Throw(error)
            },
        ));
        self.install_host_global("__host_chdir", chdir_fn);

        // __host_set_exit_code(n | undefined)
        //
        // Records the status to exit with when the run ends normally. An
        // explicit `__host_exit` and an uncaught throw take precedence in the
        // CLI. `undefined` clears the sink.
        let set_exit_fn = self.create_function(JsFunction::native(
            "__host_set_exit_code".to_string(),
            1,
            |interp, _this, args| {
                let code = match args.first() {
                    None => None,
                    Some(v) if v.is_undefined() => None,
                    Some(v) => match interp.to_number_value(v) {
                        Ok(n) if n.is_finite() => Some(n.trunc() as i64 as i32),
                        Ok(_) => None,
                        Err(e) => return Completion::Throw(e),
                    },
                };
                interp.node_host.exit_code = code;
                Completion::Normal(JsValue::UNDEFINED)
            },
        ));
        self.install_host_global("__host_set_exit_code", set_exit_fn);
    }
}

fn js_str(s: &str) -> JsValue {
    JsValue::string(JsString::from_str(s))
}

/// `(code, description, errno)` for the `chdir` failures Node reports with a
/// libuv name; anything else degrades to `UNKNOWN`.
fn errno_info(err: &std::io::Error) -> (&'static str, &'static str, i32) {
    match err.raw_os_error() {
        Some(2) => ("ENOENT", "no such file or directory", 2),
        Some(13) => ("EACCES", "permission denied", 13),
        Some(20) => ("ENOTDIR", "not a directory", 20),
        Some(36) => ("ENAMETOOLONG", "name too long", 36),
        Some(40) => ("ELOOP", "too many symbolic links encountered", 40),
        Some(5) => ("EIO", "i/o error", 5),
        Some(n) => ("UNKNOWN", "unknown error", n),
        None => ("UNKNOWN", "unknown error", 4094),
    }
}

/// Resolve `path` against the working directory and normalise `.`/`..`
/// lexically, like Node's `path.resolve` (symlinks are not followed).
fn absolute_lexical(path: &Path) -> PathBuf {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut out = PathBuf::new();
    for comp in abs.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}
