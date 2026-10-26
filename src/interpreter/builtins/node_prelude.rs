//! The JavaScript half of Node host mode (issue #1216): `process`, `console`,
//! `util`, `Buffer`, `TextEncoder`/`TextDecoder` and `crypto.getRandomValues`,
//! built on the `__host_*` floor. The sources are embedded in the binary and
//! evaluated, in this fixed order, only by `load_node_prelude`, which only the
//! `--node` launch path calls; without `--node` they are never parsed or run.
//! See `docs/adr/2026-10-26-0854-node-host-mode.md`.

use super::super::*;

/// Load order matters: the process shim extends the `process` object that the
/// console/util shim creates.
const NODE_PRELUDE: &[(&str, &str)] = &[
    (
        "node-shim.js",
        include_str!("../../../scripts/node-shim.js"),
    ),
    (
        "node-process-shim.js",
        include_str!("../../../scripts/node-process-shim.js"),
    ),
    (
        "node-buffer-shim.js",
        include_str!("../../../scripts/node-buffer-shim.js"),
    ),
    (
        "node-crypto-shim.js",
        include_str!("../../../scripts/node-crypto-shim.js"),
    ),
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
            for (const name of ["__host_argv", "__host_argv0", "__host_env_snapshot", "__host_cwd", "__host_chdir", "__host_set_exit_code"]) {
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
        let mut interp = floor_interp();
        interp.set_node_host_argv("jsse".into(), "/bin/jsse".into(), None, Vec::new());
        interp.load_node_prelude().expect("prelude loads");
        run_ok(
            &mut interp,
            r#"
            const got = [
              typeof process, typeof Buffer, typeof TextEncoder, typeof TextDecoder,
              typeof crypto.getRandomValues, typeof __host_argv, typeof __host_cwd, typeof __host_write,
            ].join(",");
            const want = "object,function,function,function,function,undefined,undefined,function";
            if (got !== want) throw new Error(got);
            "#,
        );
    }
}
