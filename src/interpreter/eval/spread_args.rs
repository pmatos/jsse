use super::*;

impl Interpreter {
    pub(super) fn eval_spread_args(
        &mut self,
        args: &[Expression],
        env: &EnvRef,
    ) -> Result<Vec<JsValue>, JsValue> {
        let mut out = Vec::new();
        for arg in args {
            if let Expression::Spread(inner, at) = arg {
                let val = match self.eval_expr(inner, env) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::UNDEFINED,
                };
                if args.len() > 1 {
                    self.node_note_statement_pos(*at);
                }
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
