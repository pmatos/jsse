//! The JavaScript half of Node host mode (issue #1216): `process`, `console`,
//! `util`, `Buffer`, `TextEncoder`/`TextDecoder` and `crypto.getRandomValues`,
//! built on the `__host_*` floor. The sources are embedded in the binary and
//! evaluated, in this fixed order, only by `load_node_prelude`, which only the
//! `--node` launch path calls; without `--node` they are never parsed or run.
//! See `docs/adr/2026-10-26-0854-node-host-mode.md`.

use super::super::*;

/// Node defines these as non-enumerable globals (`util` is not a script global
/// there at all), so they must not show up in `Object.keys(globalThis)`.
const HIDE_GLOBALS: &str = r#"(function (names) {
  for (var i = 0; i < names.length; i++) {
    var d = Object.getOwnPropertyDescriptor(globalThis, names[i]);
    if (d) {
      d.enumerable = false;
      Object.defineProperty(globalThis, names[i], d);
    }
  }
})(["process", "Buffer", "TextEncoder", "TextDecoder", "util"]);"#;

/// Load order matters: the process shim extends the `process` object that the
/// console/util shim creates, the timers shim needs the event-loop shim's
/// handoff, the fs scripts need `Buffer` and `util`, the Buffer shim consumes the
/// base64 decoder that `node-base64-decode.js` leaves on `globalThis`, and the
/// globals are hidden last.
const NODE_PRELUDE: &[(&str, &str)] = &[
    (
        "node-v8-error.js",
        include_str!("../../../scripts/node-v8-error.js"),
    ),
    (
        "node-shim.js",
        include_str!("../../../scripts/node-shim.js"),
    ),
    (
        "node-process-shim.js",
        include_str!("../../../scripts/node-process-shim.js"),
    ),
    (
        "node-process-metrics.js",
        include_str!("../../../scripts/node-process-metrics.js"),
    ),
    (
        "node-base64-decode.js",
        include_str!("../../../scripts/node-base64-decode.js"),
    ),
    (
        "node-buffer-shim.js",
        include_str!("../../../scripts/node-buffer-shim.js"),
    ),
    (
        "node-textdecoder.js",
        include_str!("../../../scripts/node-textdecoder.js"),
    ),
    (
        "node-crypto-shim.js",
        include_str!("../../../scripts/node-crypto-shim.js"),
    ),
    (
        "node-event-loop-shim.js",
        include_str!("../../../scripts/node-event-loop-shim.js"),
    ),
    (
        "node-timers-shim.js",
        include_str!("../../../scripts/node-timers-shim.js"),
    ),
    (
        "node-fs-support.js",
        include_str!("../../../scripts/node-fs-support.js"),
    ),
    (
        "node-fs-stats.js",
        include_str!("../../../scripts/node-fs-stats.js"),
    ),
    (
        "node-fs-meta.js",
        include_str!("../../../scripts/node-fs-meta.js"),
    ),
    (
        "node-fs-dirs.js",
        include_str!("../../../scripts/node-fs-dirs.js"),
    ),
    (
        "node-fs-ops.js",
        include_str!("../../../scripts/node-fs-ops.js"),
    ),
    (
        "node-fs-cp.js",
        include_str!("../../../scripts/node-fs-cp.js"),
    ),
    (
        "node-fs-shim.js",
        include_str!("../../../scripts/node-fs-shim.js"),
    ),
    (
        "node-process-signals.js",
        include_str!("../../../scripts/node-process-signals.js"),
    ),
    (
        "node-wasm-shim.js",
        include_str!("../../../scripts/node-wasm-shim.js"),
    ),
    (
        "node-builtin-loader.js",
        include_str!("../../../scripts/node-builtin-loader.js"),
    ),
    ("hide-globals", HIDE_GLOBALS),
];

impl Interpreter {
    /// Evaluate the embedded Node prelude as global scripts.
    ///
    /// Must run after `enable_node_host` and `set_node_host_argv`, because the
    /// prelude snapshots the floor primitives and the launch arguments. Returns
    /// a description of the failure if a module does not parse or throws; that
    /// is an engine bug, never a user error.
    pub(crate) fn load_node_prelude(&mut self) -> Result<(), String> {
        for (name, source) in NODE_PRELUDE {
            let program = crate::parser::Parser::new(source)
                .and_then(|mut p| p.parse_program())
                .map_err(|e| format!("node prelude {name}: parse error: {e:?}"))?;
            if let Completion::Throw(val) = self.run(&program) {
                return Err(format!("node prelude {name}: {}", self.format_value(&val)));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    fn run_ok(interp: &mut Interpreter, source: &str) {
        let program = Parser::new(source)
            .and_then(|mut p| p.parse_program())
            .expect("parse");
        let c = interp.run(&program);
        assert!(
            matches!(c, Completion::Normal(_) | Completion::Empty),
            "unexpected completion: {c:?}"
        );
    }

    fn floor_interp() -> Interpreter {
        let mut interp = Interpreter::new();
        interp.enable_node_host();
        interp
    }

    #[test]
    fn floor_alone_does_not_evaluate_the_prelude() {
        let mut interp = floor_interp();
        run_ok(
            &mut interp,
            r#"
            for (const name of ["process", "Buffer", "TextEncoder", "TextDecoder", "crypto", "util"]) {
              if (typeof globalThis[name] !== "undefined") throw new Error(name + " defined by the floor alone");
            }
            "#,
        );
    }

    #[test]
    fn process_floor_primitives_are_non_enumerable_configurable_functions() {
        let mut interp = floor_interp();
        run_ok(
            &mut interp,
            r#"
            for (const name of ["__host_argv", "__host_argv0", "__host_env_snapshot", "__host_cwd", "__host_chdir", "__host_set_exit_code", "__host_builtin_source"]) {
              const d = Object.getOwnPropertyDescriptor(globalThis, name);
              if (!d) throw new Error(name + " missing");
              if (d.enumerable || !d.configurable) throw new Error(name + " has wrong attributes");
              if (typeof d.value !== "function") throw new Error(name + " not a function");
            }
            "#,
        );
    }

    #[test]
    fn load_installs_the_globals_and_takes_the_process_floor_private() {
        crate::run_on_engine_stack(|| {
            let mut interp = floor_interp();
            interp.set_node_host_argv("jsse".into(), "/bin/jsse".into(), None, Vec::new());
            interp.load_node_prelude().expect("prelude loads");
            run_ok(
                &mut interp,
                r#"
                const hidden = Object.keys(globalThis).filter((k) => ["process", "Buffer", "TextEncoder", "TextDecoder", "util"].includes(k));
                if (hidden.length) throw new Error("enumerable globals: " + hidden);
                const got = [
                  typeof process, typeof Buffer, typeof TextEncoder, typeof TextDecoder,
                  typeof crypto.getRandomValues, typeof __host_argv, typeof __host_cwd, typeof __host_write,
                  typeof __host_builtin_source, typeof __node_builtin, typeof __node_builtins,
                  typeof __host_main_kind, typeof __node_run_main, typeof require, typeof require("fs").readFileSync,
                ].join(",");
                const want = "object,function,function,function,function,undefined,undefined,function,undefined,undefined,undefined,undefined,undefined,function,function";
                if (got !== want) throw new Error(got);
                "#,
            );
        });
    }
}
