//! Bodies that suspend their chunk: sync generators (`yield`) and async
//! functions (`await`), both lowered to `Op::Yield`.
//!
//! A `yield` is followed by a stub that a `return(v)` resumption runs; an
//! `await` has no stub and a zero skip offset, which is how a driver tells the
//! two apart (`GeneratorFrame::suspended_at_await`). An `await` resumes only
//! with a value (`Next`) or a rejection (`Throw`).

use super::{
    AstNode, CompileError, Compiler, FrameShape, HeadBinding, Terminal, body_has_poison,
    compile_function_body_as, declares_using, expr_has_poison, pattern_has_poison,
};
use crate::ast::Expression;
use crate::interpreter::bytecode::chunk::Chunk;
use crate::interpreter::bytecode::op::Op;

/// What a function body being compiled may suspend on.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum BodyKind {
    Plain,
    Generator,
    Async,
    AsyncGenerator,
}

fn is_yield_expression(expr: &Expression) -> bool {
    matches!(expr, Expression::Yield(..))
}

fn is_await_expression(expr: &Expression) -> bool {
    matches!(expr, Expression::Await(..))
}

fn node_contains(node: &AstNode, pred: fn(&Expression) -> bool) -> bool {
    match node {
        AstNode::Expr(expr) => expr_has_poison(expr, pred),
        AstNode::Stmt(stmt) => body_has_poison(std::slice::from_ref(stmt.as_ref()), pred),
    }
}

fn head_binding_contains(head: &HeadBinding, pred: fn(&Expression) -> bool) -> bool {
    match head {
        HeadBinding::Declare { pattern, .. } | HeadBinding::Assign { pattern } => {
            pattern_has_poison(pattern, pred)
        }
    }
}

impl Compiler {
    /// A compiler for `kind`; a suspending body has no tail position
    /// (sec-isintailposition).
    pub(super) fn for_body(kind: BodyKind) -> Self {
        let mut c = Self::new(super::CompileGoal::Function);
        c.generator = matches!(kind, BodyKind::Generator | BodyKind::AsyncGenerator);
        c.async_body = matches!(kind, BodyKind::Async | BodyKind::AsyncGenerator);
        c.no_tail = u32::from(kind != BodyKind::Plain);
        c
    }

    /// Whether `node`, which would run on the tree-walker, holds a suspension
    /// point of this body (and so could not suspend).
    pub(super) fn suspends_in(&self, node: &AstNode) -> bool {
        (self.generator && node_contains(node, is_yield_expression))
            || (self.async_body && node_contains(node, is_await_expression))
    }

    pub(super) fn suspends_in_head(&self, head: &HeadBinding) -> bool {
        (self.generator && head_binding_contains(head, is_yield_expression))
            || (self.async_body && head_binding_contains(head, is_await_expression))
    }

    /// `yield argument`: `[..., value]` → `[..., sent]`, or `[...]` when the
    /// sent value is `discard`ed (an expression statement). A `next(v)`
    /// resumption pushes `v` and jumps over the stub that follows; `return(v)`
    /// resumes at the stub with the operand stack cut back to the persistent
    /// height, so the stub is the same exit a `return v` statement emits
    /// (running `finally` blocks and closing iterators on the way out);
    /// `throw(e)` resumes at the enclosing handler like a throw raised by the
    /// op itself. In an async generator the operand is awaited before the
    /// yield and the `return(v)` stub awaits `v` before it unwinds
    /// (AsyncGeneratorYield, AsyncGeneratorUnwrapYieldResumption).
    pub(super) fn compile_yield(
        &mut self,
        argument: Option<&Expression>,
        discard: bool,
    ) -> Result<(), CompileError> {
        match argument {
            Some(argument) => self.compile_expr(argument)?,
            None => {
                self.emit(Op::LoadUndefined);
                self.push_n(1);
            }
        }
        if self.async_body {
            self.emit_await(false)?;
        }
        self.emit(Op::Yield);
        self.emit_u16(self.stack_base);
        self.code.push(u8::from(discard));
        let skip = self.code.len();
        self.code.extend_from_slice(&[0, 0]);
        let (stack, refs) = (self.current_stack, self.current_refs);
        self.current_stack = self.stack_base + 1;
        self.current_refs = 0;
        self.max_stack = self.max_stack.max(self.current_stack);
        if self.async_body {
            self.emit_await(false)?;
        }
        self.compile_jump(Terminal::Return, true)?;
        self.current_stack = if discard { stack - 1 } else { stack };
        self.current_refs = refs;
        self.patch_jump(skip)
    }

    /// `await argument`: `[..., value]` → `[..., settled]`, or `[...]` when
    /// the result is `discard`ed (an expression statement). Suspends the chunk
    /// with `value`; the driver resumes it with the fulfilment value or raises
    /// the rejection at this op, where an enclosing `try` can catch it.
    pub(super) fn compile_await(
        &mut self,
        argument: &Expression,
        discard: bool,
    ) -> Result<(), CompileError> {
        self.compile_expr(argument)?;
        self.emit_await(discard)
    }

    /// The suspension op of an `await` over the value on top of the stack.
    pub(super) fn emit_await(&mut self, discard: bool) -> Result<(), CompileError> {
        self.emit(Op::Yield);
        self.emit_u16(self.stack_base);
        self.code.push(u8::from(discard));
        let skip = self.code.len();
        self.code.extend_from_slice(&[0, 0]);
        self.patch_jump(skip)?;
        if discard {
            self.pop_n(1);
        }
        Ok(())
    }
}

/// [`compile_function_body_as`] for an async function body, whose `await`s
/// suspend the chunk. `for await` and `using` declarations, and an `await`
/// the compiler would hand to the tree-walker, are `Ineligible` and keep the
/// state machine.
pub(crate) fn compile_async_body(
    body: &[crate::ast::Statement],
    shape: &FrameShape<'_>,
) -> Result<Chunk, CompileError> {
    if super::stmts_contain(body, &declares_using) {
        return Err(CompileError::Ineligible(
            "construct needs the async state machine",
        ));
    }
    compile_function_body_as(body, shape, BodyKind::Async)
}

/// [`compile_async_body`] for an async generator body: `yield` and `await`
/// both suspend, `return expr` awaits `expr`, and `yield*` (and what an async
/// function cannot host) keeps the state machine.
pub(crate) fn compile_async_generator_body(
    body: &[crate::ast::Statement],
    shape: &FrameShape<'_>,
) -> Result<Chunk, CompileError> {
    if super::stmts_contain(body, &declares_using) {
        return Err(CompileError::Ineligible(
            "construct needs the async generator state machine",
        ));
    }
    compile_function_body_as(body, shape, BodyKind::AsyncGenerator)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::bytecode::tests::{decode_ops, work_ops};
    use crate::parser::Parser;

    fn compile_async(source: &str) -> Result<Chunk, CompileError> {
        let wrapped = format!("async function f(a) {{ {source}\n}}");
        let mut parser = Parser::new(&wrapped).expect("parser init");
        let program = parser.parse_program().expect("parse");
        let Some(crate::ast::Statement::FunctionDeclaration(decl)) =
            program.body.as_slice().first()
        else {
            panic!("function declaration");
        };
        compile_async_body(
            decl.body.statements.as_slice(),
            &FrameShape {
                arguments_slot: false,
                simple_params: &["a".to_string()],
                separate_body_env: false,
            },
        )
    }

    #[test]
    fn await_compiles_to_a_suspension_with_no_return_stub() {
        let chunk = compile_async("await a; return 1;").expect("compile");
        let ops = decode_ops(&chunk);
        let at = ops
            .iter()
            .position(|(op, _)| *op == Op::Yield)
            .expect("suspension op");
        let (persistent, discard, skip) = (ops[at].1[0], ops[at].1[1], ops[at].1[2]);
        assert_eq!((persistent, discard, skip), (0, 1, 0), "discarded, no stub");
        assert_ne!(ops[at + 1].0, Op::Return, "nothing follows as a stub");
    }

    #[test]
    fn await_with_a_used_result_keeps_the_value_on_the_stack() {
        let chunk = compile_async("var x = await a; return x;").expect("compile");
        let (_, operands) = decode_ops(&chunk)
            .into_iter()
            .find(|(op, _)| *op == Op::Yield)
            .expect("suspension op");
        assert_eq!(operands[1], 0, "the settled value is the op's result");
    }

    #[test]
    fn await_inside_a_for_of_records_the_iterator_as_persistent() {
        let chunk = compile_async("for (const x of a) { await x; }").expect("compile");
        let (_, operands) = decode_ops(&chunk)
            .into_iter()
            .find(|(op, _)| *op == Op::Yield)
            .expect("suspension op");
        assert_eq!(operands[0], 1, "the iterator slot stays on the stack");
    }

    #[test]
    fn async_chunk_accepts_awaits_in_every_position_it_hosts() {
        for source in [
            "await a;",
            "var x = await a;",
            "x = (await a) + (await a);",
            "try { await 1; } catch (e) { await 2; } finally { await 3; }",
            "for (;;) { switch (await a) { case await a: break; } }",
            "while (await a) { if (await a) continue; }",
            "a.b = await a;",
            "return f(1, await a, 3);",
            "return f(...[await a]);",
            "const { p, q } = await a; return p;",
            "label: for (const x of await a) { try { await x; } catch (e) { break label; } }",
            "return a ? await a : await a;",
        ] {
            let chunk = compile_async(source).unwrap_or_else(|e| panic!("{source}: {e:?}"));
            assert!(work_ops(&chunk).contains(&Op::Yield), "{source}");
        }
    }

    #[test]
    fn async_chunk_rejects_what_a_suspending_frame_cannot_host() {
        for source in [
            "for await (const x of a) {}",
            "await using r = a;",
            "using r = a;",
            "var [x = await a] = a;",
            "var { x = await a } = a;",
            "return `${await a}`;",
            "return { [await a]: 1 };",
            "class C extends (await a) {}",
            "for (const [x = await a] of a) {}",
            "try {} catch ([x = await a]) {}",
            "with (a) { await 1; }",
            "eval('1'); await 1;",
            "return new.target;",
        ] {
            assert!(
                compile_async(source).is_err(),
                "expected {source} to stay on the state machine"
            );
        }
    }

    fn compile_async_generator(source: &str) -> Result<Chunk, CompileError> {
        let wrapped = format!("async function* g(a) {{ {source}\n}}");
        let mut parser = Parser::new(&wrapped).expect("parser init");
        let program = parser.parse_program().expect("parse");
        let Some(crate::ast::Statement::FunctionDeclaration(decl)) =
            program.body.as_slice().first()
        else {
            panic!("function declaration");
        };
        compile_async_generator_body(
            decl.body.statements.as_slice(),
            &FrameShape {
                arguments_slot: false,
                simple_params: &["a".to_string()],
                separate_body_env: false,
            },
        )
    }

    fn suspensions(chunk: &Chunk) -> Vec<(bool, bool)> {
        decode_ops(chunk)
            .into_iter()
            .filter(|(op, _)| *op == Op::Yield)
            .map(|(_, operands)| (operands[2] == 0, operands[1] != 0))
            .collect()
    }

    #[test]
    fn async_generator_yield_awaits_its_operand_and_its_return_stub_awaits() {
        let chunk = compile_async_generator("yield a;").expect("compile");
        // await a; yield; the stub's await of a return(v) value.
        assert_eq!(
            suspensions(&chunk),
            [(true, false), (false, true), (true, false)]
        );
        let ops = work_ops(&chunk);
        let stub = ops
            .iter()
            .rposition(|op| *op == Op::Yield)
            .expect("stub await");
        assert_eq!(ops[stub + 1], Op::Return, "the unwind follows the await");
    }

    #[test]
    fn async_generator_return_with_a_value_awaits_it_but_a_bare_return_does_not() {
        let with_value = compile_async_generator("return a;").expect("compile");
        assert_eq!(suspensions(&with_value), [(true, false)]);
        let bare = compile_async_generator("return;").expect("compile");
        assert!(suspensions(&bare).is_empty());
    }

    #[test]
    fn async_generator_chunk_rejects_yield_star_and_for_await() {
        for source in [
            "yield* a;",
            "for await (const x of a) {}",
            "await using r = a;",
            "var [x = yield] = a;",
        ] {
            assert!(
                compile_async_generator(source).is_err(),
                "expected {source} to stay on the state machine"
            );
        }
    }
}
