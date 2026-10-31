//! `switch` (sec-switch-statement-runtime-semantics-evaluation,
//! sec-runtime-semantics-caseblockevaluation): the discriminant is evaluated
//! once; each case selector is evaluated lazily, in source order, and compared
//! with `===`; execution starts at the first match (else the `default`
//! clause) and falls through the later clauses.
//!
//! General form, a linear chain:
//!
//!   <discriminant>
//!   per case:  <selector>; CaseStrictEq match_i
//!   Pop; Jump default_body | end
//!   match_i:   Pop; Jump body_i            (one trampoline per case)
//!   body_0 ... body_n                      (source order)
//!   end:
//!
//! When every selector is a number or string literal (at least
//! `MIN_TABLE_CASES` of them) no selector can have an effect, so the chain is
//! one lookup instead (#1491):
//!
//!   <discriminant>
//!   SwitchTable t                          (pops it, jumps to body_i | default | end)
//!   body_0 ... body_n
//!   end:

use super::{CompileError, CompileGoal, Compiler, LoopFrame};
use crate::ast::{Expression, Literal, Statement, SwitchStatement, UnaryOp};
use crate::interpreter::bytecode::op::Op;
use crate::interpreter::bytecode::switch_table::{CaseKey, SwitchTable};
use crate::interpreter::hoisting::block_declares_lexical;

/// Below this many literal cases the chain is as cheap as a lookup.
const MIN_TABLE_CASES: usize = 3;

/// The selector of a literal case clause (`-1` is a negated number literal).
fn literal_key(test: &Expression) -> Option<CaseKey> {
    match test {
        Expression::Literal(Literal::Number(n)) => Some(CaseKey::Number(*n)),
        Expression::Literal(Literal::String(units)) => Some(CaseKey::String(units.clone())),
        Expression::Unary(UnaryOp::Minus, operand) => match operand.as_ref() {
            Expression::Literal(Literal::Number(n)) => Some(CaseKey::Number(-*n)),
            _ => None,
        },
        _ => None,
    }
}

/// Each clause's selector (`None` for `default`), when all of them are literals.
fn literal_keys(switch: &SwitchStatement) -> Option<Vec<Option<CaseKey>>> {
    let keys = switch
        .cases
        .iter()
        .map(|case| match &case.test {
            Some(test) => literal_key(test).map(Some),
            None => Some(None),
        })
        .collect::<Option<Vec<_>>>()?;
    (keys.iter().flatten().count() >= MIN_TABLE_CASES).then_some(keys)
}

/// How control reaches a clause body, fixed up once the bodies are placed.
enum Dispatch {
    Table(usize),
    Chain {
        no_match_site: usize,
        trampoline_sites: Vec<Option<usize>>,
    },
}

impl Compiler {
    pub(super) fn compile_switch(
        &mut self,
        switch: &SwitchStatement,
        labels: Vec<String>,
    ) -> Result<(), CompileError> {
        if self.goal != CompileGoal::Function {
            return Err(CompileError::Unsupported("statement:Switch"));
        }
        // Case bodies share one block scope, entered after the discriminant
        // is evaluated; the selectors run inside it
        // (sec-switch-statement-runtime-semantics-evaluation). The tree-walker
        // hoists a case clause's function declarations as `var`-kind
        // bindings with its own rules, so those keep the whole statement
        // there.
        let scoped = switch
            .cases
            .iter()
            .any(|case| block_declares_lexical(&case.consequent));
        let decls = if scoped {
            let lists: Vec<&[Statement]> = switch
                .cases
                .iter()
                .map(|case| case.consequent.as_slice())
                .collect();
            Some(
                self.scope_declarations(&lists)
                    .filter(|decls| decls.functions.is_empty())
                    .ok_or(CompileError::Unsupported("block-scoped declaration"))?,
            )
        } else {
            None
        };
        self.compile_expr(&switch.discriminant)?;
        if let Some(decls) = decls {
            self.open_scope(decls)?;
        }
        let keys = literal_keys(switch);
        let dispatch = if keys.is_some() {
            self.emit_table_dispatch()?
        } else {
            self.emit_case_chain(switch)?
        };
        let mut frame = LoopFrame::new(
            labels,
            None,
            self.scopes.len(),
            self.cleanups.len(),
            self.stack_base,
        );
        frame.is_switch = true;
        self.loop_frames.push(frame);
        let mut body_starts: Vec<usize> = Vec::with_capacity(switch.cases.len());
        for case in &switch.cases {
            body_starts.push(self.code.len());
            self.compile_statements(&case.consequent, false, scoped)?;
        }
        let frame = self
            .loop_frames
            .pop()
            .expect("pushed above, not re-entrant");
        let default_start = switch
            .cases
            .iter()
            .position(|case| case.test.is_none())
            .map(|index| body_starts[index]);
        match dispatch {
            Dispatch::Table(index) => {
                let end = self.code.len();
                self.max_jump_target = self.max_jump_target.max(end);
                let offset = |at: usize| u32::try_from(at).map_err(|_| CompileError::JumpOverflow);
                let mut clauses = Vec::new();
                for (key, start) in keys.unwrap_or_default().into_iter().zip(&body_starts) {
                    if let Some(key) = key {
                        clauses.push((key, offset(*start)?));
                    }
                }
                self.switch_tables[index] =
                    SwitchTable::build(&clauses, offset(default_start.unwrap_or(end))?);
            }
            Dispatch::Chain {
                no_match_site,
                trampoline_sites,
            } => {
                for (site, start) in trampoline_sites.iter().zip(&body_starts) {
                    if let Some(site) = site {
                        self.patch_jump_to(*site, *start)?;
                    }
                }
                match default_start {
                    Some(start) => self.patch_jump_to(no_match_site, start)?,
                    None => self.patch_jump(no_match_site)?,
                }
            }
        }
        for site in frame.break_sites {
            self.patch_jump(site)?;
        }
        self.resolve_escape_targets(&frame.escape_breaks);
        if scoped {
            self.close_scope();
        }
        Ok(())
    }

    fn emit_table_dispatch(&mut self) -> Result<Dispatch, CompileError> {
        let index = self.switch_tables.len();
        let operand = u16::try_from(index)
            .map_err(|_| CompileError::Ineligible("switch table pool overflow"))?;
        self.switch_tables.push(SwitchTable::default());
        self.emit(Op::SwitchTable);
        self.emit_u16(operand);
        self.pop_n(1);
        Ok(Dispatch::Table(index))
    }

    fn emit_case_chain(&mut self, switch: &SwitchStatement) -> Result<Dispatch, CompileError> {
        let mut match_sites: Vec<Option<usize>> = Vec::with_capacity(switch.cases.len());
        for case in &switch.cases {
            match &case.test {
                Some(test) => {
                    self.compile_expr(test)?;
                    self.pop_n(1);
                    match_sites.push(Some(self.emit_jump(Op::CaseStrictEq)));
                }
                None => match_sites.push(None),
            }
        }
        // No selector matched: drop the discriminant and go to `default`,
        // or past the switch when there is none.
        self.emit(Op::Pop);
        self.pop_n(1);
        let no_match_site = self.emit_jump(Op::Jump);
        let mut trampoline_sites: Vec<Option<usize>> = Vec::with_capacity(switch.cases.len());
        for match_site in &match_sites {
            match match_site {
                Some(site) => {
                    self.patch_jump(*site)?;
                    self.emit(Op::Pop);
                    trampoline_sites.push(Some(self.emit_jump(Op::Jump)));
                }
                None => trampoline_sites.push(None),
            }
        }
        Ok(Dispatch::Chain {
            no_match_site,
            trampoline_sites,
        })
    }
}
