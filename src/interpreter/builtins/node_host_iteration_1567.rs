//! V8's messages for a `for await` head and a `yield*` operand that are not
//! (async) iterable, under `--node` only (#1567). The text is not the sync
//! one: `for await` reads `Symbol.asyncIterator` (a null or undefined operand
//! is a failed property read), words a call as `o.m(...) is not a function or
//! its return value is not async iterable` and anything else as `x is not
//! async iterable`. `yield*` prints `yield* (intermediate value)` for the
//! operands whose position `CallPrinter` finds (calls, members, operators)
//! and keeps the value form for names, literals and `this`.

use super::super::*;
use super::node_host_expr_text::{expr_text, is_folded, optional_call_callee};
use crate::ast::{Expression, LogicalOp};

const ASYNC_KEY: &str = "Symbol(Symbol.asyncIterator)";

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Delegation {
    ForAwait,
    Yield,
    YieldInAsync,
}

fn peel(expr: &Expression) -> &Expression {
    match expr {
        Expression::Sequence(items) if items.len() == 1 => peel(&items[0]),
        other => other,
    }
}

/// The temporary the generator lowering keeps an operand with an `await` in.
fn is_lowered_temp(expr: &Expression) -> bool {
    matches!(expr, Expression::Identifier(name)
        if name.starts_with("$forof_iterable_") || name.starts_with("$yield_val_"))
}

/// Whether `CallPrinter` finds the operand of a `yield*`.
fn yield_operand_printed(expr: &Expression) -> bool {
    match peel(expr) {
        other if is_lowered_temp(other) => true,
        Expression::Call(..)
        | Expression::New(..)
        | Expression::Member(..)
        | Expression::TaggedTemplate(..)
        | Expression::Assign(..)
        | Expression::Update(..)
        | Expression::Await(..) => true,
        Expression::Sequence(items) | Expression::Comma(items) => items.len() == 2,
        other @ (Expression::Unary(..) | Expression::Binary(..)) => !is_folded(other),
        _ => false,
    }
}

/// `for await (x of operand)` where V8 finds no text for the operand.
fn for_await_missed(expr: &Expression) -> bool {
    match peel(expr) {
        Expression::Conditional(..)
        | Expression::OptionalChain(..)
        | Expression::Logical(LogicalOp::And | LogicalOp::Or, ..) => true,
        Expression::Sequence(items) | Expression::Comma(items) => items.len() > 2,
        _ => false,
    }
}

impl Interpreter {
    /// `error` from the iterator of a `for await` head or a `yield*`, worded
    /// as V8 does when it is the builtin not-iterable error of this frame.
    pub(crate) fn reword_delegation(
        &mut self,
        error: JsValue,
        value: &JsValue,
        source: Option<&Expression>,
        kind: Delegation,
    ) -> JsValue {
        if !self.node_host.stack.installed {
            return error;
        }
        let asynchronous = kind != Delegation::Yield;
        if asynchronous && let Some(text) = self.method_not_callable(&error, source) {
            return self.create_type_error(&text);
        }
        if !self.iterable_error_here(&error) {
            return error;
        }
        let text = if asynchronous && value.is_nullish() {
            let what = if value.is_null() { "null" } else { "undefined" };
            format!("Cannot read properties of {what} (reading '{ASYNC_KEY}')")
        } else {
            let Some(source) = source else { return error };
            let source = peel(source);
            if let Some(callee) = optional_call_callee(source) {
                let text = match kind {
                    Delegation::Yield => format!(
                        "{callee} is not iterable (cannot read property Symbol(Symbol.iterator))"
                    ),
                    _ => format!("{callee} is not a function"),
                };
                return self.create_type_error(&text);
            }
            match kind {
                Delegation::Yield if yield_operand_printed(source) => {
                    "yield* (intermediate value) is not iterable".to_string()
                }
                Delegation::Yield => return error,
                Delegation::YieldInAsync if yield_operand_printed(source) => {
                    "yield* (intermediate value) is not async iterable".to_string()
                }
                Delegation::YieldInAsync => "undefined is not a function".to_string(),
                Delegation::ForAwait => match source {
                    Expression::Call(callee, ..) | Expression::TaggedTemplate(callee, _) => {
                        format!(
                            "{}(...) is not a function or its return value is not async iterable",
                            expr_text(callee)
                        )
                    }
                    Expression::New(..) => "(intermediate value) is not a function or its \
                         return value is not async iterable"
                        .to_string(),
                    _ if is_lowered_temp(source) => {
                        "(intermediate value) is not async iterable".to_string()
                    }
                    _ if for_await_missed(source) => "undefined is not a function".to_string(),
                    _ => format!("{} is not async iterable", expr_text(source)),
                },
            }
        };
        self.create_type_error(&text)
    }
}

impl Interpreter {
    /// `GetIterator` for the head of a `for await`, worded as V8 does.
    pub(crate) fn await_iterator(
        &mut self,
        value: &JsValue,
        source: &Expression,
    ) -> Result<JsValue, JsValue> {
        self.get_async_iterator(value)
            .map_err(|e| self.reword_delegation(e, value, Some(source), Delegation::ForAwait))
    }

    /// `GetIterator` (`GetAsyncIterator` in an async generator) for the
    /// operand of a `yield*`.
    pub(crate) fn yield_iterator(
        &mut self,
        value: &JsValue,
        source: Option<&Expression>,
        in_async: bool,
    ) -> Result<JsValue, JsValue> {
        let (got, kind) = if in_async {
            (self.get_async_iterator(value), Delegation::YieldInAsync)
        } else {
            (self.get_iterator(value), Delegation::Yield)
        };
        got.map_err(|e| self.reword_delegation(e, value, source, kind))
    }

    /// The error of the iterator of a `for` head.
    pub(crate) fn reword_head(
        &mut self,
        error: JsValue,
        value: &JsValue,
        head: &crate::ast::ForOfStatement,
    ) -> JsValue {
        if head.is_await {
            self.reword_delegation(error, value, Some(&head.right), Delegation::ForAwait)
        } else if let Some(callee) = optional_call_callee(peel(&head.right)) {
            if !self.iterable_error_here(&error) {
                return error;
            }
            let text =
                format!("{callee} is not iterable (cannot read property Symbol(Symbol.iterator))");
            self.create_type_error(&text)
        } else {
            self.reword_iterable(error, &head.right)
        }
    }
}

impl Interpreter {
    /// The `Symbol.asyncIterator` method of a `for await` head or a `yield*`
    /// operand is not callable (`{ [Symbol.asyncIterator]: 1 }`): V8 words the
    /// operand when it prints one (`obj.ag is not async iterable`) and the
    /// method otherwise (`number 1 is not a function`).
    fn method_not_callable(
        &mut self,
        error: &JsValue,
        source: Option<&Expression>,
    ) -> Option<String> {
        let id = error.as_object_id()?;
        let (made, depth, ..) = self.node_host.iter_operand.not_callable.as_ref()?;
        if *made != id || *depth != self.call_depth + 1 {
            return None;
        }
        let method = self.node_host.iter_operand.method.take()?;
        self.node_host.iter_operand.not_callable = None;
        let printable = source.map(peel).filter(|source| {
            matches!(
                source,
                Expression::Identifier(_) | Expression::Member(..) | Expression::This
            )
        });
        Some(match printable {
            Some(source) => format!("{} is not async iterable", expr_text(source)),
            None => format!("{method} is not a function"),
        })
    }
}

impl Interpreter {
    /// V8's text for a for-of head that is not iterable: an optional chain
    /// that ends in a call prints its callee.
    pub(crate) fn head_message(source: &Expression) -> Option<String> {
        match optional_call_callee(peel(source)) {
            Some(callee) => Some(format!(
                "{callee} is not iterable (cannot read property Symbol(Symbol.iterator))"
            )),
            None => Self::iterable_message(source),
        }
    }
}
