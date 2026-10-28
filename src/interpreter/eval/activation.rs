//! The activation record of a user-function call.

use super::*;

impl Interpreter {
    /// Runs `run` as the activation of `func_obj_id` in `exec_env`, visible to
    /// `fn.arguments`/`fn.caller` and the collector; resources declared in the
    /// body are disposed on exit.
    #[inline(always)]
    pub(super) fn run_activation(
        &mut self,
        func_obj_id: u64,
        arguments: CallFrameArguments,
        exec_env: &EnvRef,
        arrow_this: Option<&JsValue>,
        run: impl FnOnce(&mut Self) -> Completion,
    ) -> Completion {
        let caller_pos = self.take_call_pos();
        self.call_stack_frames.push(CallFrame {
            func_obj_id,
            arguments,
            is_eval: false,
            caller_pos,
            env_index: self.call_stack_envs.len(),
        });
        let frame_depth = self.call_stack_frames.len();
        let noted = match arrow_this {
            Some(this) if self.node_host.stack.installed => {
                self.node_note_arrow_receiver(this);
                true
            }
            _ => false,
        };
        self.call_stack_envs.push(exec_env.clone());
        self.in_tail_position = false;
        let saved_tco_suppress = std::mem::take(&mut self.tco_suppress_depth);
        let result = run(self);
        self.tco_suppress_depth = saved_tco_suppress;
        if noted {
            self.node_host.stack.arrow_receivers.pop();
        }
        self.call_stack_envs.pop();
        debug_assert_eq!(self.call_stack_frames.len(), frame_depth);
        self.pop_call_frame();
        if exec_env.borrow().has_dispose_stack() {
            self.dispose_resources(exec_env, result)
        } else {
            result
        }
    }
}
