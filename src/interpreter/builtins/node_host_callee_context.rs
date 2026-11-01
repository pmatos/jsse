//! V8's wording for a call that is not callable and is itself the operand of
//! an iteration, under `--node` only (#1557): `for (x of o.m())`, `[...o.m()]`,
//! `var [a] = o.m()` and `[a] = o.m()` are `o.m is not a function or its
//! return value is not iterable`, and `for await (x of o.m())` is `o.m(...) is
//! not a function or its return value is not async iterable`. The call is read
//! back from the source like the callee (`node_host_callee.rs`): what comes
//! before the callee and after the call tells the context.

use super::super::Interpreter;
use super::node_host_callee::{callee_site, skip_ws_back, skip_ws_fwd, word_before};

#[derive(Clone, Copy)]
enum Operand {
    Sync,
    Async,
    Yield,
}

/// Whether the `of` at `of_at` belongs to `for (` or `for await (`.
fn for_head(src: &str, of_at: usize) -> Option<Operand> {
    let b = src.as_bytes();
    let mut depth = 0usize;
    let mut open = of_at;
    loop {
        open = open.checked_sub(1)?;
        match b[open] {
            b')' => depth += 1,
            b'(' if depth == 0 => break,
            b'(' => depth -= 1,
            _ => {}
        }
    }
    let before = skip_ws_back(b, open);
    match word_before(src, before) {
        "for" => Some(Operand::Sync),
        "await" => {
            let before = skip_ws_back(b, before - "await".len());
            (word_before(src, before) == "for").then_some(Operand::Async)
        }
        _ => None,
    }
}

fn operand_kind(src: &str, start: usize, after: usize) -> Option<Operand> {
    let b = src.as_bytes();
    let before = skip_ws_back(b, start);
    let next = b.get(skip_ws_fwd(b, after)).copied();
    if word_before(src, before) == "of" {
        return (next == Some(b')'))
            .then(|| for_head(src, before - 2))
            .flatten()
            .filter(|kind| matches!(kind, Operand::Async));
    }
    if src[..before].ends_with('*') {
        let star = skip_ws_back(b, before - 1);
        let delegating = word_before(src, star) == "yield"
            && matches!(next, None | Some(b';' | b'}' | b')' | b']' | b','));
        return delegating.then_some(Operand::Yield);
    }
    None
}

impl Interpreter {
    /// The message for the call being dispatched when it is an iteration
    /// operand whose callee is not callable.
    pub(super) fn iterable_call_message(&self) -> Option<String> {
        if !self.node_host.stack.installed || self.top_native().is_some() {
            return None;
        }
        let src = self.node_caller_source()?;
        let (text, start, after) = callee_site(&src, self.node_host.call_pos)?;
        Some(match operand_kind(&src, start, after?)? {
            Operand::Sync => {
                format!("{text} is not a function or its return value is not iterable")
            }
            Operand::Async => {
                format!("{text}(...) is not a function or its return value is not async iterable")
            }
            Operand::Yield if self.caller_is_async() => {
                "yield* (intermediate value) is not async iterable".to_string()
            }
            Operand::Yield => "yield* (intermediate value) is not iterable".to_string(),
        })
    }

    fn caller_is_async(&self) -> bool {
        let func = self.call_stack_frames.last().map(|frame| frame.func_obj_id);
        func.and_then(|id| self.get_object(id)).is_some_and(|obj| {
            matches!(
                obj.borrow().callable.as_deref(),
                Some(crate::interpreter::types::JsFunction::User { is_async: true, .. })
            )
        })
    }
}
