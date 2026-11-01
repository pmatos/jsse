use super::*;

impl Compiler {
    pub(super) fn compile_array_literal(
        &mut self,
        elements: &[Option<Expression>],
    ) -> Result<(), CompileError> {
        self.check_literal_stack()?;
        if let Some(template) = constant_array_template(elements) {
            let idx = u16::try_from(self.array_templates.len())
                .map_err(|_| CompileError::Ineligible("array template pool overflow"))?;
            self.array_templates.push(template);
            self.emit(Op::NewArrayConst);
            self.emit_u16(idx);
            self.push_n(1);
            return Ok(());
        }
        if elements.iter().any(Option::is_none) {
            return Err(CompileError::Unsupported("array literal: elision"));
        }
        if elements
            .iter()
            .flatten()
            .any(|e| matches!(e, Expression::Spread(..)))
        {
            return Err(CompileError::Unsupported("array literal: spread"));
        }
        // The first chunk of values becomes the array; later chunks are
        // appended to it, so the operand stack never holds more than one
        // chunk of pending elements above the array.
        let mut created = false;
        let mut pending: u16 = 0;
        for element in elements.iter().flatten() {
            self.compile_expr(element)?;
            pending += 1;
            if usize::from(pending) == LITERAL_VALUE_CHUNK {
                self.flush_array_values(created, pending);
                created = true;
                pending = 0;
            }
        }
        if pending > 0 || !created {
            self.flush_array_values(created, pending);
        }
        Ok(())
    }
}
