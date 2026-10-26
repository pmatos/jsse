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
    builtin!("events"),
    builtin!("internal/abort_controller"),
    builtin!("internal/event_target"),
    builtin!("internal/event_target_event"),
    builtin!("internal/events/abort_listener"),
    builtin!("internal/events/statics"),
    builtin!("internal/events/symbols"),
    builtin!("internal/fixed_queue"),
    builtin!("internal/per_context/domexception"),
    builtin!("internal/perf/utils"),
    builtin!("internal/process/warning"),
    builtin!("internal/webidl"),
    builtin!("internal/constants"),
    builtin!("internal/errors"),
    builtin!("internal/tty"),
    builtin!("internal/util"),
    builtin!("internal/validators"),
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
    builtin!("util"),
    builtin!("internal/assert"),
    builtin!("internal/buffer_inspect"),
    builtin!("internal/binding/types"),
    builtin!("internal/binding/util"),
    builtin!("internal/mime"),
    builtin!("internal/util/colors"),
    builtin!("internal/util/comparisons"),
    builtin!("internal/util/comparisons_sets"),
    builtin!("internal/util/debuglog"),
    builtin!("internal/util/dotenv"),
    builtin!("internal/util/inspect"),
    builtin!("internal/util/inspect_collections"),
    builtin!("internal/util/inspect_error"),
    builtin!("internal/util/inspect_format"),
    builtin!("internal/util/inspect_highlight"),
    builtin!("internal/util/inspect_property"),
    builtin!("internal/util/inspect_raw"),
    builtin!("internal/util/inspect_text"),
    builtin!("internal/util/inspect_value"),
    builtin!("internal/util/parse_args/parse_args"),
    builtin!("internal/util/parse_args/utils"),
    builtin!("internal/util/style_text"),
    builtin!("internal/util/types"),
    builtin!("internal/util/uv_errmap"),
    builtin!("internal/assert/myers_diff"),
    builtin!("internal/util/diff"),
    builtin!("internal/util/signals"),
    builtin!("internal/util/trace_sigint"),
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
