use super::*;

impl Interpreter {
    pub(super) fn eval_spread_args(
        &mut self,
        args: &[Expression],
        env: &EnvRef,
        call_pos: u32,
    ) -> Result<Vec<JsValue>, JsValue> {
        let mut out = Vec::new();
        for arg in args {
            if let Expression::Spread(inner, at) = arg {
                let val = match self.eval_expr(inner, env) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::UNDEFINED,
                };
                // V8 reports a lone spread at the call, any other at the operand.
                self.node_note_statement_pos(if args.len() > 1 { *at } else { call_pos });
                let items = self.call_spread_items(inner, &val, args)?;
                for item in &items {
                    self.gc_root_value(item);
                }
                out.extend(items);
            } else {
                let val = match self.eval_expr(arg, env) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::UNDEFINED,
                };
                self.gc_root_value(&val);
                out.push(val);
            }
        }
        Ok(out)
    }
}
