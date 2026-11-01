//! ES modules that do not parse, reported as Node reports the entry point's
//! (issue #1484): `file:///f.mjs:LINE`, the source line, carets under the token
//! V8 stopped at, a blank line, then the `SyntaxError`. The launcher parses the
//! main module itself, so its failure never reached the module loader and
//! printed the parser's `Debug` text; a dependency's failure had no excerpt.
//! The loader's internal frames are not reproduced.

use super::link_errors::ARROW;
use super::*;
use crate::interpreter::ModuleKey;
use crate::parser::{ParseError, Parser};
use std::path::Path;

fn excerpt(path: &Path, source: &str, span: (usize, usize)) -> String {
    let start = span.0.min(source.len());
    let end = span.1.clamp(start, source.len());
    let line_start = source[..start].rfind(['\n', '\r']).map_or(0, |n| n + 1);
    let line_end = source[start..]
        .find(['\n', '\r'])
        .map_or(source.len(), |n| start + n);
    let line = source[..line_start].matches('\n').count() + 1;
    let padding: String = source[line_start..start]
        .chars()
        .map(|c| if c == '\t' { '\t' } else { ' ' })
        .collect();
    let width = if start == source.len() {
        0
    } else {
        source[start..end.min(line_end)].chars().count().max(1)
    };
    format!(
        "{}:{line}\n{}\n{padding}{}\n\n",
        super::super::node_host_stack_sources::file_url(&path.to_string_lossy()),
        &source[line_start..line_end],
        "^".repeat(width)
    )
}

impl Interpreter {
    /// The `SyntaxError` for `error`, which stopped `parser` on the module
    /// `path`. Under `--node` it carries the excerpt, unless an `import()` is
    /// loading it: Node's `stack` has none there, and the report finds the
    /// caller's frames.
    pub(crate) fn parse_failure(
        &mut self,
        path: &Path,
        source: &str,
        parser: &Parser,
        error: &ParseError,
    ) -> JsValue {
        let value = self.create_error("SyntaxError", &error.js_bare());
        if self.node_host.stack.installed && self.node_host.stack.import_site.is_none() {
            let header = excerpt(path, source, parser.failure_span());
            self.note_excerpt(&value, &header);
        }
        value
    }

    /// Reports the main module `path`, whose `source` failed to parse, as Node
    /// does. False (nothing done) unless the host stack hooks and the ES module
    /// loader are installed.
    pub fn node_entry_syntax_error(
        &mut self,
        path: &Path,
        source: &str,
        parser: &Parser,
        error: &ParseError,
    ) -> bool {
        if !self.node_host.stack.installed || !self.node_host.esm.is_active() {
            return false;
        }
        let value = self.parse_failure(path, source, parser, error);
        self.node_esm_enter_main(&ModuleKey::for_file(path.to_path_buf()));
        self.drain_after_run(&Completion::Throw(value));
        true
    }

    fn note_excerpt(&mut self, error: &JsValue, header: &str) {
        if let Some(handle) = error.as_object_id().and_then(|id| self.get_object_cell(id)) {
            handle
                .borrow_mut()
                .set_hidden_slot(ARROW, string_value(header));
        }
    }
}
