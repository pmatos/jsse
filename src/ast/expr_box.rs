//! `ExprBox`, the owning link to a child `Expression`.

use super::Expression;
use std::fmt;

/// Owning link to a child `Expression`, used everywhere the AST used to hold
/// a plain `Box<Expression>`.
///
/// `Expression` itself must never implement `Drop`: two parser productions
/// (binary/logical operator precedence, and the member/call/optional-chain
/// continuation loop) build chains iteratively specifically so they are not
/// bounded by `MAX_PARSE_DEPTH`, so these chains can reach millions of levels
/// deep. The compiler's derived drop glue walks such a chain with one native
/// stack frame per level, which overflows the stack long before any other
/// limit fires (jsse#614). Boxing the recursive link behind `ExprBox` instead
/// lets `Drop` be hand-written as an iterative worklist walk, while
/// `Expression` stays `Drop`-free so existing by-value matches on it keep
/// compiling (`impl Drop for Expression` would make those an `E0509`).
pub(crate) struct ExprBox(Box<Expression>);

impl ExprBox {
    pub(crate) fn new(expr: Expression) -> Self {
        Self(Box::new(expr))
    }

    /// Extracts the inner `Expression`, leaving `self` holding a childless
    /// placeholder so dropping it (at the end of this call) is O(1).
    ///
    /// This is the only legal way to move an `Expression` out of an
    /// `ExprBox` by value: `*self.0` cannot be moved directly because `self`
    /// implements `Drop`, but swapping through the mutable reference the
    /// `Box` gives us is not a partial move of `self` and is always allowed.
    pub(crate) fn into_expression(mut self) -> Expression {
        std::mem::replace(&mut *self.0, Expression::This)
    }
}

impl Clone for ExprBox {
    fn clone(&self) -> Self {
        // Recurses per nesting level; jsse#614 is about Drop only.
        Self(self.0.clone())
    }
}

impl std::ops::Deref for ExprBox {
    type Target = Expression;

    fn deref(&self) -> &Expression {
        &self.0
    }
}

impl std::ops::DerefMut for ExprBox {
    fn deref_mut(&mut self) -> &mut Expression {
        &mut self.0
    }
}

impl AsRef<Expression> for ExprBox {
    fn as_ref(&self) -> &Expression {
        &self.0
    }
}

impl fmt::Debug for ExprBox {
    /// Delegates to the inner `Expression`'s `Debug`, matching how
    /// `Box<Expression>` printed before this wrapper existed (`Box<T>`'s own
    /// `Debug` impl is transparent) — so debug output is unchanged.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.0, f)
    }
}

impl Drop for ExprBox {
    fn drop(&mut self) {
        let mut stack = Vec::new();
        push_children(&mut self.0, &mut stack);
        while let Some(mut node) = stack.pop() {
            push_children(&mut node, &mut stack);
        }
    }
}

/// Steals every `ExprBox` child directly owned by `node`, replacing each with
/// a childless placeholder, and pushes the extracted `Expression`s onto
/// `stack`. Called repeatedly by `ExprBox`'s `Drop` to turn what would
/// otherwise be per-level native recursion into a heap-backed loop.
///
/// Exhaustive with no wildcard arm: a future `Expression` variant that adds
/// an `ExprBox` field must be handled here explicitly, or it silently
/// reintroduces native recursion for that variant's children — mirroring the
/// discipline `gc::trace_object_fields` already applies to `ObjectKind`.
/// `Vec<Expression>` payloads (`Call`/`New` arguments, `Array`, `Sequence`,
/// `Comma`, `Object` property values) are deliberately not drained here: they
/// are not built by the unbounded continuation loops this fix targets, and
/// each element still safely self-flattens through its own `ExprBox` fields
/// when the `Vec`'s ordinary drop reaches it.
fn push_children(node: &mut Expression, stack: &mut Vec<Expression>) {
    fn take(child: &mut ExprBox) -> Expression {
        std::mem::replace(&mut *child.0, Expression::This)
    }

    match node {
        Expression::Literal(_)
        | Expression::Identifier(_, _)
        | Expression::This
        | Expression::Super
        | Expression::Array(_, _)
        | Expression::Object(_, _)
        | Expression::Function(_)
        | Expression::ArrowFunction(_)
        | Expression::Class(_)
        | Expression::Comma(_)
        | Expression::Template(_)
        | Expression::Sequence(_)
        | Expression::ImportMeta
        | Expression::NewTarget
        | Expression::PrivateIdentifier(_) => {}
        Expression::Unary(_, e)
        | Expression::Update(.., e, _)
        | Expression::Spread(e, _)
        | Expression::PreIterated(e)
        | Expression::ClassProtoParent(e, _)
        | Expression::Await(e, _)
        | Expression::Typeof(e)
        | Expression::Void(e)
        | Expression::Delete(e)
        | Expression::TaggedTemplate(e, _) => {
            stack.push(take(e));
        }
        Expression::Binary(_, l, r, _)
        | Expression::Logical(_, l, r)
        | Expression::Assign(_, l, r, _)
        | Expression::BoundCallee(l, r)
        | Expression::ResolvedHeritage(l, r)
        | Expression::OptionalChain(l, r) => {
            stack.push(take(l));
            stack.push(take(r));
        }
        Expression::Conditional(a, b, c) => {
            stack.push(take(a));
            stack.push(take(b));
            stack.push(take(c));
        }
        Expression::Call(callee, _, _) | Expression::New(callee, _, _) => {
            stack.push(take(callee));
        }
        Expression::Member(obj, _, _) => {
            stack.push(take(obj));
        }
        Expression::Yield(arg, _) => {
            if let Some(arg) = arg {
                stack.push(take(arg));
            }
        }
        Expression::Import(spec, opts, _)
        | Expression::ImportDefer(spec, opts)
        | Expression::ImportSource(spec, opts) => {
            stack.push(take(spec));
            if let Some(opts) = opts {
                stack.push(take(opts));
            }
        }
    }
}
