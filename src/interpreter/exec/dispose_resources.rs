use super::*;

impl Interpreter {
    pub(crate) fn dispose_resources(&mut self, env: &EnvRef, completion: Completion) -> Completion {
        match self.take_dispose_stack(env) {
            Some(stack) => {
                let completion = self.settle_tail_call(&stack, completion);
                self.run_dispose_cursor_blocking(DisposeCursor::new(stack, completion))
            }
            None => completion,
        }
    }
}
