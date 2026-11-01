//! ES modules that do not parse, reported as Node reports the entry point's
//! (issue #1484): the source excerpt, a blank line, then the `SyntaxError`.

use super::link_errors::ARROW;
use super::*;
use crate::interpreter::ModuleKey;
use crate::parser::{ParseError, Parser};

fn is_terminator(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}')
}

/// The line of `source` that holds `offset`, and `width` carets under it
/// (tabs kept in the padding), each ending in a newline.
pub(super) fn arrow_lines(source: &str, offset: usize, width: usize) -> String {
    let before = &source[..offset];
    let start = before.rfind(is_terminator).map_or(0, |n| {
        n + before[n..].chars().next().map_or(1, char::len_utf8)
    });
    let end = source[offset..]
        .find(is_terminator)
        .map_or(source.len(), |n| offset + n);
    let padding: String = source[start..offset]
        .chars()
        .map(|c| if c == '\t' { '\t' } else { ' ' })
        .collect();
    format!("{}\n{padding}{}\n", &source[start..end], "^".repeat(width))
}

fn line_number(source: &str, offset: usize) -> usize {
    let mut line = 1;
    let mut previous = '\0';
    for c in source[..offset].chars() {
        if is_terminator(c) && !(c == '\n' && previous == '\r') {
            line += 1;
        }
        previous = c;
    }
    line
}

fn excerpt(file: &std::path::Path, source: &str, (start, end): (usize, usize)) -> String {
    let start = start.min(source.len());
    let end = end.clamp(start, source.len());
    let width = if start == source.len() {
        0
    } else {
        let line_end = source[start..]
            .find(is_terminator)
            .map_or(end, |n| start + n);
        source[start..end.min(line_end)].chars().count().max(1)
    };
    format!(
        "{}:{}\n{}\n",
        super::super::node_host_stack_sources::file_url(&file.to_string_lossy()),
        line_number(source, start),
        arrow_lines(source, start, width)
    )
}

impl Interpreter {
    /// The `SyntaxError` for `error`, which stopped `parser` on the module
    /// `module`. Under `--node` it carries the excerpt, unless an `import()` is
    /// loading it: Node's `stack` has none there.
    pub(crate) fn parse_failure(
        &mut self,
        module: &ModuleKey,
        source: &str,
        parser: &Parser,
        error: &ParseError,
    ) -> JsValue {
        let value = self.create_error("SyntaxError", &error.js_bare());
        let installed = self.node_host.stack.installed;
        if let Some(file) = module.file_path().filter(|_| installed)
            && self.node_host.stack.import_site.is_none()
            && let Some(handle) = value.as_object_id().and_then(|id| self.get_object_cell(id))
        {
            let header = excerpt(file, source, parser.failure_span());
            handle
                .borrow_mut()
                .set_hidden_slot(ARROW, string_value(&header));
        }
        value
    }

    /// Reports the main module `path`, whose `source` failed to parse, as Node
    /// does. False (nothing done) unless the host stack hooks and the ES module
    /// loader are installed.
    pub fn node_entry_syntax_error(
        &mut self,
        path: &std::path::Path,
        source: &str,
        parser: &Parser,
        error: &ParseError,
    ) -> bool {
        if !self.node_host.stack.installed || !self.node_host.esm.is_active() {
            return false;
        }
        let key = ModuleKey::for_file(path.to_path_buf());
        let value = self.parse_failure(&key, source, parser, error);
        self.node_esm_enter_main(&key);
        self.with_module_key(Some(key), |interp| {
            interp.drain_after_run(&Completion::Throw(value));
        });
        true
    }
}
