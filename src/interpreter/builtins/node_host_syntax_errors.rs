//! `SyntaxError` messages under `--node` (issues #1484, #1523): the parser and
//! lexer word them like V8 where they can, `static_text` covers the ones raised
//! without parser state, and `eval`, `Function` and the module loader hand the
//! parser's `Display` text (`SyntaxError: ...`) to `create_error`, which would
//! print the prefix twice. Default mode keeps its own text.

use super::super::*;
use std::borrow::Cow;

impl Interpreter {
    /// `create_error`'s message for an error named `name`.
    pub(crate) fn node_error_message_for<'a>(&self, name: &str, msg: &'a str) -> Cow<'a, str> {
        if name != "SyntaxError" || !self.node_host.stack.installed {
            return self.node_error_message(msg);
        }
        let bare = msg.strip_prefix("SyntaxError: ").unwrap_or(msg);
        match crate::parser::v8_messages::static_text(bare) {
            Some(text) => Cow::Owned(text.into_owned()),
            None => self.node_error_message(bare),
        }
    }
}
