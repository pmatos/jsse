//! V8's function name inference for `--node` stack frames (issue #1379). An
//! anonymous function, arrow or class assigned to a member target
//! (`exports.run = function () {}`) has no `name`, but V8 prints the target's
//! text for its frames. Once the node stack hooks are installed the parser
//! records that text per function literal, so default mode and the prelude's
//! own scripts are untouched.

use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Called by the stack prelude's install hook; process-wide like the call
/// positions, since scripts parse lazily.
pub(crate) fn enable() {
    ENABLED.store(true, Ordering::Relaxed);
}

type Entry = (std::rc::Weak<str>, Rc<str>);

thread_local! {
    static NAMES: std::cell::RefCell<HashMap<(usize, usize), Entry>> = Default::default();
    static PURGE_AT: std::cell::Cell<usize> = const { std::cell::Cell::new(1024) };
}

fn key(text: &SourceText) -> (usize, usize) {
    let (source, start) = text.origin();
    (Rc::as_ptr(source).cast::<u8>() as usize, start)
}

/// The inferred name of a function whose source is `text`. A key is its
/// source's address, which cannot be reused while the entry's `Weak` lives.
pub(crate) fn inferred_name(text: &SourceText) -> Option<Rc<str>> {
    NAMES.with_borrow(|names| names.get(&key(text)).map(|(_, name)| name.clone()))
}

/// Names the function after `prefix`; one already named by an inner
/// assignment of a chain (`a.x = b.y = function () {}`) is appended.
fn set_inferred_name(text: &SourceText, prefix: &str) {
    let name = match inferred_name(text) {
        Some(inner) => format!("{prefix}.{inner}"),
        None => prefix.to_string(),
    };
    NAMES.with_borrow_mut(|names| {
        if names.len() >= PURGE_AT.get() {
            names.retain(|_, (source, _)| source.strong_count() > 0);
            PURGE_AT.set((names.len() * 2).max(1024));
        }
        let weak = Rc::downgrade(text.origin().0);
        names.insert(key(text), (weak, Rc::from(name)));
    });
}

/// The names V8's `FuncNameInferrer` collects for a target: identifiers and
/// dotted properties as written, string-literal keys by value, any other
/// computed key as `<computed>`, `this` and `prototype` not at all.
fn target_names(target: &Expression, out: &mut Vec<String>) -> bool {
    match target {
        Expression::Identifier(name) => {
            if name != "undefined" {
                out.push(name.clone());
            }
            true
        }
        Expression::This => true,
        Expression::Member(object, property, _) => {
            if !target_names(object, out) {
                out.clear();
            }
            let name = match property {
                MemberProperty::Dot(name) => name.clone(),
                MemberProperty::Computed(key) => match &**key {
                    Expression::Literal(Literal::String(units)) => {
                        let text = String::from_utf16_lossy(units);
                        if is_array_index(&text) {
                            "<computed>".to_string()
                        } else {
                            text
                        }
                    }
                    _ => "<computed>".to_string(),
                },
                MemberProperty::Private(_) => return false,
            };
            if name != "prototype" {
                out.push(name);
            }
            true
        }
        _ => false,
    }
}

fn is_array_index(text: &str) -> bool {
    text.parse::<u32>()
        .is_ok_and(|n| n != u32::MAX && n.to_string() == text)
}

fn name_function(expr: &Expression, prefix: &str) {
    let source = match expr {
        Expression::Function(f) if f.name.as_ref().is_none_or(|n| n.is_empty()) => {
            f.source_text.as_ref()
        }
        Expression::ArrowFunction(a) => a.source_text.as_ref(),
        Expression::Class(c) if c.name.as_ref().is_none_or(|n| n.is_empty()) => {
            c.source_text.as_ref()
        }
        Expression::Conditional(_, consequent, alternate) => {
            name_function(consequent, prefix);
            name_function(alternate, prefix);
            return;
        }
        Expression::Logical(_, left, right) => {
            name_function(left, prefix);
            name_function(right, prefix);
            return;
        }
        Expression::Assign(_, _, right) => {
            name_function(right, prefix);
            return;
        }
        _ => return,
    };
    if let Some(source) = source {
        set_inferred_name(source, prefix);
    }
}

impl Parser<'_> {
    /// Called with a parsed `left op= right`; names the anonymous functions
    /// `right` evaluates to after the member `left`.
    pub(super) fn infer_assigned_name(
        &self,
        op: AssignOp,
        left: &Expression,
        right: &Expression,
        right_start: usize,
    ) {
        if !ENABLED.load(Ordering::Relaxed)
            || !matches!(left, Expression::Member(..))
            || !matches!(
                op,
                AssignOp::Assign
                    | AssignOp::LogicalAndAssign
                    | AssignOp::LogicalOrAssign
                    | AssignOp::NullishAssign
            )
        {
            return;
        }
        // V8 leaves a directly parenthesized function or arrow unnamed.
        let literal = match right {
            Expression::Function(f) => f.source_text.as_ref(),
            Expression::ArrowFunction(a) => a.source_text.as_ref(),
            _ => None,
        };
        if literal.is_some_and(|text| text.origin().1 > right_start) {
            return;
        }
        let mut names = Vec::new();
        if target_names(left, &mut names) && !names.is_empty() {
            name_function(right, &names.join("."));
        }
    }
}
