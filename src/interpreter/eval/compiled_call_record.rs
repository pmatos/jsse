//! Publishing a literal's compiled-call record from the first call of any
//! of its closures.

use super::*;

impl Interpreter {
    /// Installs `frame`'s compiled-call record when a function of the shape
    /// the hot call path serves is called through the general path, and returns
    /// the cache state the body will run with. A closure starts `Untried` and
    /// shares its literal's memoised verdict (#900), so a literal whose
    /// closures are each called once must resolve that verdict here: waiting
    /// for a closure to be called again would leave every one of them on the
    /// general path.
    pub(super) fn install_compiled_call(
        &mut self,
        func_obj_id: u64,
        frame: &LiteralFrame,
        params: &Rc<Vec<Pattern>>,
        body: &Body,
        flags: u8,
        bytecode_cache: BytecodeCacheState,
    ) -> BytecodeCacheState {
        let has_simple_params = flags & CALL_SIMPLE_PARAMS != 0;
        let uses_arguments = flags & CALL_USES_ARGUMENTS != 0;
        if frame.compiled_call().is_some()
            || !self.bytecode_enabled
            || !(has_simple_params || (frame.named_param_plan(params).is_some() && !uses_arguments))
            || self
                .get_object_cell(func_obj_id)
                .is_none_or(|callee| callee.borrow().is_class_constructor())
        {
            return bytecode_cache;
        }
        let bytecode_cache = match bytecode_cache {
            BytecodeCacheState::Untried => self.shared_bytecode_verdict(func_obj_id, body),
            known => known,
        };
        if let BytecodeCacheState::Compiled(chunk) = &bytecode_cache {
            let is_arrow = flags & CALL_ARROW != 0;
            frame.install_compiled_call(|| CompiledCall {
                params: params.clone(),
                shape: if !has_simple_params {
                    CallShape::Defaults
                } else if uses_arguments && !is_arrow {
                    CallShape::Arguments
                } else {
                    CallShape::Plain
                },
                body: body.clone(),
                chunk: chunk.clone(),
                is_arrow,
                is_strict: flags & CALL_STRICT != 0,
            });
        }
        bytecode_cache
    }
}
