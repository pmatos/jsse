//! Errors made while `import()` loads a module (issue #1483). Node resolves,
//! reads and parses the module graph inside the `import()` call, so a
//! resolution or syntax error is made with the caller's frames on the stack
//! (`at loadHerebyfile (...)`, then the `at async` chain). jsse loads in a
//! microtask when the call is made during module evaluation, where those
//! frames are gone, so the call site's frames are captured when `import()` runs
//! and stand in for the live stack of any error made while that job loads.

use super::*;

impl Interpreter {
    /// The frames of the code calling `import()`, as an array to root until the
    /// load is done; `undefined` before the stack hooks are installed.
    pub(crate) fn node_import_site(&mut self) -> JsValue {
        if !self.node_host.stack.installed {
            return JsValue::UNDEFINED;
        }
        self.capture_frames_array(0, None)
    }

    /// The current stack as the array of frame records `stack` is formatted
    /// from; `undefined` when `Error.stackTraceLimit` is not a number.
    pub(super) fn capture_frames_array(
        &mut self,
        skip_top: usize,
        until: Option<Until>,
    ) -> JsValue {
        match self.stack_trace_limit() {
            Some(limit) => {
                let record = self.capture_records(skip_top, until, limit);
                self.create_array(record)
            }
            None => JsValue::UNDEFINED,
        }
    }

    /// Runs `load` with errors made inside it taking `site`'s frames.
    pub(crate) fn node_at_import_site<T>(
        &mut self,
        site: &JsValue,
        load: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let site = site.as_object_id().map(|_| site.clone());
        let saved = std::mem::replace(&mut self.node_host.stack.import_site, site);
        let result = load(self);
        self.node_host.stack.import_site = saved;
        result
    }
}
