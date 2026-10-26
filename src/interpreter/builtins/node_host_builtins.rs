//! The embedded builtin-module table for Node host mode (issue #1221): module
//! name to JavaScript source, `include_str!`d from `node-lib/` so the sources
//! are versioned with the binary. The prelude's `node-builtin-loader.js` reads
//! them through `__host_builtin_source(name)` and evaluates a module only when
//! it is first loaded; nothing here runs without `--node`. A module named
//! `internal/...` is reachable only from other builtins. See
//! `docs/adr/2026-10-26-0854-node-host-mode.md`.

use super::super::*;

macro_rules! builtin {
    ($name:literal) => {
        (
            $name,
            include_str!(concat!("../../../node-lib/", $name, ".js")),
        )
    };
}

const NODE_BUILTIN_MODULES: &[(&str, &str)] = &[
    builtin!("path"),
    builtin!("path/posix"),
    builtin!("path/win32"),
    builtin!("internal/constants"),
    builtin!("internal/errors"),
    builtin!("internal/util"),
    builtin!("internal/validators"),
    builtin!("internal/modules/cjs/loader"),
    builtin!("internal/modules/exports"),
    builtin!("internal/modules/package_json_reader"),
    builtin!("internal/path/common"),
    builtin!("internal/path/glob"),
    builtin!("internal/path/posix"),
    builtin!("internal/path/win32_a"),
    builtin!("internal/path/win32_b"),
    builtin!("internal/deps/minimatch/ast"),
    builtin!("internal/deps/minimatch/ast-maps"),
    builtin!("internal/deps/minimatch/assert-valid-pattern"),
    builtin!("internal/deps/minimatch/balanced-match"),
    builtin!("internal/deps/minimatch/brace-expansion"),
    builtin!("internal/deps/minimatch/brace-expressions"),
    builtin!("internal/deps/minimatch/escape"),
    builtin!("internal/deps/minimatch/index"),
    builtin!("internal/deps/minimatch/matchers"),
    builtin!("internal/deps/minimatch/minimatch"),
    builtin!("internal/deps/minimatch/optimize"),
    builtin!("internal/deps/minimatch/unescape"),
];

impl Interpreter {
    pub(super) fn setup_node_host_builtins(&mut self) {
        let source_fn = self.create_function(JsFunction::native(
            "__host_builtin_source".to_string(),
            1,
            |interp, _this, args| {
                let name = match interp
                    .to_string_lossy(args.first().unwrap_or(JsValue::undefined_ref()))
                {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                Completion::Normal(
                    NODE_BUILTIN_MODULES
                        .iter()
                        .find(|(n, _)| *n == name)
                        .map_or(JsValue::UNDEFINED, |(_, src)| {
                            JsValue::string(JsString::from_str(src))
                        }),
                )
            },
        ));
        self.install_host_global("__host_builtin_source", source_fn);
    }
}
