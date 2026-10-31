use super::*;

impl Interpreter {
    /// Runs a native function with its operands rooted. While the `--node`
    /// stack hooks are installed, a builtin that is visible to V8's stack
    /// traces is on the native frame list for the duration of the call.
    pub(super) fn run_native_call(
        &mut self,
        func_id: u64,
        f: &dyn Fn(&mut Interpreter, &JsValue, &[JsValue]) -> Completion,
        this_val: &JsValue,
        args: &[JsValue],
        construct: bool,
    ) -> Completion {
        #[cfg(feature = "perf-counters")]
        {
            self.perf.calls_to_native += 1;
        }
        let operands_base = self.gc_root_frame();
        self.gc_root_value(this_val);
        for a in args.iter() {
            self.gc_root_value(a);
        }
        let operands_depth = self.gc_root_frame();
        let saved_this = self.last_call_this_value.take();
        let native_mark = self.node_enter_native(func_id, this_val, construct);
        let result = f(self, this_val, args);
        self.node_leave_native(native_mark);
        self.last_call_this_value = saved_this;
        self.last_call_had_explicit_return = true;
        self.gc_assert_root_depth(operands_depth, "a native call");
        // Unroot the native call operands after the call returns.
        for a in args.iter().rev() {
            self.gc_unroot_value(a);
        }
        self.gc_unroot_value(this_val);
        self.gc_assert_root_depth(operands_base, "a native call's operands");
        result
    }
}
