//! A binary operator expression (§13.15.3 ApplyStringOrNumericBinaryOperator
//! and the relational operators), and V8's error position for it (#1511).

use super::*;

impl Interpreter {
    /// `pos` is the operator's source offset plus one (0 outside `--node`):
    /// the frame's position becomes it before the operator runs, so a throw
    /// from the operator, or from a `valueOf` it calls, reports where V8 does.
    #[inline]
    pub(super) fn eval_binary_expr(
        &mut self,
        op: BinaryOp,
        left: &Expression,
        right: &Expression,
        pos: u32,
        env: &EnvRef,
    ) -> Completion {
        if op == BinaryOp::In
            && let Expression::PrivateIdentifier(name) = left
        {
            let branded = self.resolve_private_name(name, env);
            let rval = match self.eval_expr(right, env) {
                Completion::Normal(v) => v,
                other => return other,
            };
            return self.private_in(branded, &rval);
        }
        let lval = match self.eval_expr(left, env) {
            Completion::Normal(v) => v,
            other => return other,
        };
        // EvaluateStringOrNumericBinaryExpression retains lVal while
        // evaluating rVal, then retains both values while applying the
        // operator. Either phase can run user code and reach a GC
        // safepoint, so keep object operands rooted until the operation
        // has completed.
        self.gc_root_value(&lval);
        let rval = match self.eval_expr(right, env) {
            Completion::Normal(v) => v,
            other => {
                self.gc_unroot_value(&lval);
                return other;
            }
        };
        self.gc_root_value(&rval);
        let numeric = lval.is_number() && rval.is_number();
        if pos != 0 && (!numeric || matches!(op, BinaryOp::In | BinaryOp::Instanceof)) {
            self.node_host.call_pos = pos;
        }
        let result = if op == BinaryOp::Instanceof {
            self.eval_instanceof(&lval, &rval)
        // Fast path for string + on owned primitive values:
        // skip eval_binary → to_primitive → js_value_to_code_units clone chain.
        } else if op == BinaryOp::Add && is_primitive_string_concat(&lval, &rval) {
            self.concat_primitives(&lval, &rval)
        } else {
            self.eval_binary(op, &lval, &rval)
        };
        self.gc_unroot_value(&rval);
        self.gc_unroot_value(&lval);
        result
    }
}
