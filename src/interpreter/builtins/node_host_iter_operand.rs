//! V8's wording for an operand of an iteration that fails while it is being
//! evaluated, under `--node` only (#1557). V8 words an error by the
//! expression at the error position; when that is the operand of a for-of,
//! an array spread or an array pattern's initializer it prints the operand:
//!
//! - a call that is not callable: `o.m is not a function or its return value
//!   is not iterable`;
//! - an array assignment that cannot iterate its source: `[b] is not
//!   iterable`.
//!
//! The throw sites cannot tell, so `eval_iter` evaluates such an operand and
//! rewords what its own top-level operation threw.

use super::super::*;
use super::node_host_expr_text::expr_text;
use crate::ast::{AssignOp, ForOfStatement};

/// What the iteration operand being evaluated is.
#[derive(Default)]
pub(crate) struct IterOperand {
    /// The not-callable error made last: its object id, the call depth,
    /// the position of the call and the callee text.
    not_callable: Option<(u64, usize, u32, String)>,
    /// The address and call depth of the target of the array assignment
    /// that is the operand under evaluation.
    target: Option<(usize, usize)>,
}

/// `expr` without the parentheses the parser keeps around an assignment.
pub(crate) fn unparenthesized(expr: &Expression) -> &Expression {
    match expr {
        Expression::Sequence(items) if items.len() == 1 => &items[0],
        expr => expr,
    }
}

pub(crate) fn operand_call_pos(expr: &Expression) -> Option<u32> {
    match expr {
        Expression::Call(_, _, site) => Some(site.1),
        Expression::TaggedTemplate(_, template) => u32::try_from(template.id >> 32).ok(),
        _ => None,
    }
    .filter(|pos| *pos != 0)
}

impl Interpreter {
    /// The TypeError for calling `value`, which is not callable.
    pub(crate) fn not_callable_error(&mut self, value: &JsValue) -> JsValue {
        let shown = match self.node_callee_text(false) {
            Some(shown) => shown,
            None if self.node_host.stack.installed
                && self.top_native().is_some()
                && !value.is_object() =>
            {
                self.v8_call_site_text(value)
            }
            None => self.describe_non_callable(value),
        };
        let error = self.create_type_error(&format!("{shown} is not a function"));
        if self.node_host.stack.installed
            && let Some(id) = error.as_object_id()
        {
            let note = (id, self.call_depth, self.node_host.call_pos, shown);
            self.node_host.iter_operand.not_callable = Some(note);
        }
        error
    }

    /// Evaluates the operand of an iteration.
    pub(crate) fn eval_iter(&mut self, expr: &Expression, env: &EnvRef) -> Completion {
        if !self.node_host.stack.installed {
            return self.eval_expr(expr, env);
        }
        self.node_host.iter_operand.not_callable = None;
        let saved = self.node_host.iter_operand.target;
        if let Expression::Assign(AssignOp::Assign, target, ..) = unparenthesized(expr)
            && let Expression::Array(elements, _) = &**target
        {
            let at = std::ptr::from_ref(elements) as usize;
            self.node_host.iter_operand.target = Some((at, self.call_depth));
        }
        let done = self.eval_expr(expr, env);
        self.node_host.iter_operand.target = saved;
        match done {
            Completion::Throw(error) => Completion::Throw(self.reword_operand_call(error, expr)),
            done => done,
        }
    }

    /// The source of the pattern assignment `target = source`; an array
    /// pattern iterates it.
    pub(crate) fn eval_source(
        &mut self,
        target: &Expression,
        source: &Expression,
        env: &EnvRef,
    ) -> Completion {
        if matches!(target, Expression::Array(..)) {
            self.eval_iter(source, env)
        } else {
            self.eval_expr(source, env)
        }
    }

    /// `eval_iter` for the head of a for-of; `for await` words it differently.
    pub(crate) fn eval_for_head(&mut self, fo: &ForOfStatement, env: &EnvRef) -> Completion {
        if fo.is_await {
            return self.eval_expr(&fo.right, env);
        }
        self.eval_iter(&fo.right, env)
    }

    /// The initializer of a declarator; an array pattern iterates it.
    pub(crate) fn eval_decl(
        &mut self,
        declarator: &crate::ast::VariableDeclarator,
        env: &EnvRef,
    ) -> Completion {
        let Some(init) = &declarator.init else {
            return Completion::Normal(JsValue::UNDEFINED);
        };
        if matches!(declarator.pattern, Pattern::Array(_)) {
            self.eval_iter(init, env)
        } else {
            self.eval_expr(init, env)
        }
    }

    fn reword_operand_call(&mut self, error: JsValue, operand: &Expression) -> JsValue {
        match operand_call_pos(operand) {
            Some(pos) => self.reword_not_callable(error, |at| at == pos),
            None => error,
        }
    }

    /// `error`, when it is the not-callable error of a call at a position
    /// `iterated` accepts, made by a call one level below this one, with V8's
    /// iteration suffix.
    pub(crate) fn reword_not_callable(
        &mut self,
        error: JsValue,
        iterated: impl Fn(u32) -> bool,
    ) -> JsValue {
        let Some((id, depth, pos, shown)) = self.node_host.iter_operand.not_callable.take() else {
            return error;
        };
        let plain = format!("{shown} is not a function");
        if error.as_object_id() != Some(id)
            || depth != self.call_depth + 1
            || !iterated(pos)
            || self.own_data_string(id, "message").as_deref() != Some(plain.as_str())
        {
            self.node_host.iter_operand.not_callable = Some((id, depth, pos, shown));
            return error;
        }
        let text = format!("{shown} is not a function or its return value is not iterable");
        self.create_type_error(&text)
    }

    /// The bytecode's call that threw `abrupt`, in a chunk that notes which
    /// call positions are iteration operands.
    pub(crate) fn call_abrupt(&mut self, abrupt: Completion, calls: &[u32]) -> Completion {
        match abrupt {
            Completion::Throw(error) if !calls.is_empty() => {
                Completion::Throw(self.reword_not_callable(error, |at| calls.contains(&at)))
            }
            abrupt => abrupt,
        }
    }

    /// `done` from the array assignment `target = source`, reworded when it
    /// is the operand of an iteration: V8 prints the target.
    pub(crate) fn reword_array_operand(
        &mut self,
        done: Completion,
        target: &Expression,
        value: &JsValue,
    ) -> Completion {
        let (Completion::Throw(error), Expression::Array(elements, _)) = (&done, target) else {
            return done;
        };
        let at = std::ptr::from_ref(elements) as usize;
        if self.node_host.iter_operand.target != Some((at, self.call_depth))
            || !self.iterable_error_was_for(value)
            || !self.iterable_error_here(error)
        {
            return done;
        }
        Completion::Throw(self.create_type_error(&format!("{} is not iterable", expr_text(target))))
    }
}
