//! Block-scope environments the compiler opens with `PushEnv`: what a scope
//! declares, entering and leaving it, and the slot hints of its bindings.

use super::{BlockScope, CompileError, Compiler, NO_INIT_SLOT, ScopeDecls};
use crate::ast::{Statement, VarKind};
use crate::interpreter::bytecode::op::Op;
use crate::interpreter::hoisting::block_declares_lexical;

impl Compiler {
    /// The slot of `name` in the current environment's binding vector for an
    /// `InitLexical`: the innermost open block environment's declaration
    /// order, or the frame layout when no block environment is open. Only a
    /// hint (the op verifies it by atom identity and falls back to by-name).
    pub(super) fn init_slot(&self, name: &str) -> u16 {
        let position = match self.scopes.last() {
            Some(names) => names.iter().position(|n| n == name),
            None => self.locals.position(name),
        };
        position
            .and_then(|i| u16::try_from(i).ok())
            .unwrap_or(NO_INIT_SLOT)
    }

    /// What BlockDeclarationInstantiation declares for the statement lists
    /// of one block (or all `case` clauses of a `switch`): block-level
    /// function declarations first, then `let`/`const`/class bindings in TDZ,
    /// the order `exec_statements` instantiates them in. `None` when a list
    /// holds a `using` declaration, whose environment owns a disposal stack
    /// the VM does not model.
    pub(super) fn scope_declarations(&self, lists: &[&[Statement]]) -> Option<ScopeDecls> {
        let mut decls = ScopeDecls::default();
        for stmts in lists {
            for stmt in *stmts {
                if let Some(f) = crate::interpreter::hoisting::unwrap_labeled_function(stmt) {
                    decls.functions.push(stmt.clone());
                    decls.function_names.push(f.name.clone());
                }
            }
        }
        for stmts in lists {
            let has_using = stmts.iter().any(|stmt| {
                matches!(
                    stmt,
                    Statement::Variable(decl)
                        if matches!(decl.kind, VarKind::Using | VarKind::AwaitUsing)
                )
            });
            if has_using {
                return None;
            }
            decls
                .lexicals
                .extend(crate::interpreter::hoisting::collect_lexical_names(stmts));
        }
        Some(decls)
    }

    /// Emits `PushEnv` for `decls` and makes its names shadow the enclosing
    /// scopes' for everything compiled until the matching `close_scope`.
    pub(super) fn open_scope(&mut self, decls: ScopeDecls) -> Result<(), CompileError> {
        let index = self.block_scopes.len();
        let operand = u16::try_from(index)
            .map_err(|_| CompileError::Ineligible("block scope pool overflow"))?;
        let mut names: Vec<String> = Vec::new();
        for name in &decls.function_names {
            if !names.contains(name) {
                names.push(name.clone());
            }
        }
        let mut lexicals = Vec::with_capacity(decls.lexicals.len());
        for (name, is_const) in &decls.lexicals {
            if !names.contains(name) {
                names.push(name.clone());
                lexicals.push((self.add_name(name)?, *is_const));
            }
        }
        self.block_scopes.push(BlockScope {
            functions: decls.functions,
            lexicals,
            simple_catch: decls.simple_catch,
        });
        self.emit(Op::PushEnv);
        self.emit_u16(operand);
        self.scopes.push(names);
        Ok(())
    }

    pub(super) fn close_scope(&mut self) {
        self.emit_pop_env(1);
        self.scopes.pop();
    }

    /// `PopEnv` for `count` environments, split to fit its u8 operand. The
    /// compile-time scope stack is the caller's to maintain: a `break` or
    /// `continue` leaves environments without closing them.
    pub(super) fn emit_pop_env(&mut self, mut count: usize) {
        while count > 0 {
            let n = count.min(usize::from(u8::MAX));
            self.emit(Op::PopEnv);
            self.code.push(n as u8);
            count -= n;
        }
    }

    /// A statement list that runs in its own environment when it declares a
    /// binding (a block, a `try`/`catch`/`finally` block).
    pub(super) fn compile_scoped_block(&mut self, body: &[Statement]) -> Result<(), CompileError> {
        if !block_declares_lexical(body) {
            return self.compile_statement_list(body);
        }
        let decls = self
            .scope_declarations(&[body])
            .ok_or(CompileError::Unsupported("block-scoped declaration"))?;
        self.open_scope(decls)?;
        self.compile_statements(body, false, true)?;
        self.close_scope();
        Ok(())
    }
}
