//! The operator of `target op= value`.

use super::*;

impl Interpreter {
    pub(in crate::interpreter) fn apply_compound_assign(
        &mut self,
        op: AssignOp,
        lval: JsValue,
        rval: JsValue,
    ) -> Completion {
        match op {
            AssignOp::AddAssign => {
                // Fast path: both primitives and at least one is a string — avoid
                // to_primitive/js_value_to_code_units clones in eval_binary.
                if is_primitive_string_concat(&lval, &rval) {
                    return self.concat_primitives(&lval, &rval);
                }
                self.eval_binary(BinaryOp::Add, &lval, &rval)
            }
            AssignOp::SubAssign => self.eval_binary(BinaryOp::Sub, &lval, &rval),
            AssignOp::MulAssign => self.eval_binary(BinaryOp::Mul, &lval, &rval),
            AssignOp::DivAssign => self.eval_binary(BinaryOp::Div, &lval, &rval),
            AssignOp::ModAssign => self.eval_binary(BinaryOp::Mod, &lval, &rval),
            AssignOp::ExpAssign => self.eval_binary(BinaryOp::Exp, &lval, &rval),
            AssignOp::LShiftAssign => self.eval_binary(BinaryOp::LShift, &lval, &rval),
            AssignOp::RShiftAssign => self.eval_binary(BinaryOp::RShift, &lval, &rval),
            AssignOp::URShiftAssign => self.eval_binary(BinaryOp::URShift, &lval, &rval),
            AssignOp::BitAndAssign => self.eval_binary(BinaryOp::BitAnd, &lval, &rval),
            AssignOp::BitOrAssign => self.eval_binary(BinaryOp::BitOr, &lval, &rval),
            AssignOp::BitXorAssign => self.eval_binary(BinaryOp::BitXor, &lval, &rval),
            _ => Completion::Normal(rval),
        }
    }
}
