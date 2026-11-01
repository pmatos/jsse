//! V8's error positions of assignments, updates and member reads that fail
//! after a sub-expression ran (#1511 stage 2), under `--node` only. The parser
//! records the `=` or `++` offset on `Expression::Assign`/`Update` (0 in
//! default mode, so every helper here is one compare there).

use super::super::eval::IdentifierRef;
use super::super::{EnvRef, Interpreter};
use crate::ast::{Expression, MemberProperty, PropSiteId};
use crate::interpreter::BindingName;
use crate::interpreter::types::Completion;
use crate::types::JsValue;

impl Interpreter {
    /// `put_value_by_ref`, reporting a constant assignment at `at`.
    pub(in crate::interpreter) fn put_ref_at<N: BindingName + ?Sized>(
        &mut self,
        at: u32,
        key: &N,
        value: JsValue,
        id_ref: &IdentifierRef,
        env: &EnvRef,
    ) -> Completion {
        if at == 0 {
            return self.put_value_by_ref(key, value, id_ref, env);
        }
        self.node_host.assign_pos = at;
        let done = self.put_value_by_ref(key, value, id_ref, env);
        self.node_host.assign_pos = 0;
        done
    }

    /// `env_set`, reporting a constant assignment at `at`.
    pub(crate) fn env_set_at<N: BindingName + ?Sized>(
        &mut self,
        at: u32,
        env: &EnvRef,
        key: &N,
        value: JsValue,
    ) -> Result<(), JsValue> {
        if at == 0 {
            return self.env_set(env, key, value);
        }
        self.node_host.assign_pos = at;
        let done = self.env_set(env, key, value);
        self.node_host.assign_pos = 0;
        done
    }

    /// `x++` and friends on an identifier, reporting a constant assignment at `at`.
    pub(crate) fn update_identifier_at<N: BindingName + ?Sized>(
        &mut self,
        at: u32,
        (op, prefix): (crate::ast::UpdateOp, bool),
        key: &N,
        env: &EnvRef,
    ) -> Completion {
        if at == 0 {
            return self.eval_identifier_update(op, prefix, key, env);
        }
        self.node_host.assign_pos = at;
        let done = self.eval_identifier_update(op, prefix, key, env);
        self.node_host.assign_pos = 0;
        done
    }

    /// The reading of a nullish base failed in a compound assignment, an
    /// update or a `delete`: V8 reports the last position emitted by the
    /// operands, the key's if it has one, else the base's, else (as it
    /// reports the statement start, which the AST does not carry) the access's.
    pub(crate) fn note_last_pos(
        &mut self,
        base: &Expression,
        prop: &MemberProperty,
        site: PropSiteId,
    ) {
        let at = last_member_pos(base, prop, site);
        if at != 0 {
            self.node_host.call_pos = at;
        }
    }
}

/// The position V8 reports for a failed read of `base[prop]` that is the
/// target of a compound assignment or update.
pub(crate) fn last_member_pos(base: &Expression, prop: &MemberProperty, site: PropSiteId) -> u32 {
    let key = match prop {
        MemberProperty::Computed(key) => last_pos(key),
        _ => 0,
    };
    [key, last_pos(base), site.1]
        .into_iter()
        .find(|at| *at != 0)
        .unwrap_or(0)
}

/// The position V8 emits last while evaluating `expr`, 0 when it emits none.
pub(crate) fn last_pos(expr: &Expression) -> u32 {
    match expr {
        Expression::Member(_, _, site) => site.1,
        Expression::Call(_, _, site) | Expression::New(_, _, site) => site.1,
        Expression::Binary(_, _, _, at) => *at,
        _ => 0,
    }
}
