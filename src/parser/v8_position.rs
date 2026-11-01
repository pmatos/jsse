//! The source position V8 reports for a dynamic `import()` under `--node`
//! (issue #1524). The call has no position of its own: a stack trace shows
//! the last position V8's bytecode generator recorded before the instruction,
//! so the parser follows that generator's evaluation order and keeps the
//! latest one. Two rules matter (Node 26.8, ~260 probed shapes, see
//! `docs/agents/library-tests-typescript-conformance-import-positions-1524.md`):
//!
//! - A *statement* position (the start of a statement, of a declarator's
//!   initializer, a loop condition, a comma operator's right operand) is
//!   pending until the first instruction is emitted.
//! - An *expression* position set while a statement position is pending is
//!   dropped; otherwise it replaces the last one. Instructions without a
//!   position (a literal, a store) only end the pending state.
//!
//! A condition, a comma operand or an arrow's body takes the position of its
//! *node* (the operator of a binary, the callee name of a call), which is only
//! known once it is parsed. An `import()` evaluated before anything else in
//! one of those reads its position through a slot filled in at that point.
//!
//! The state is per thread, like `inferred_names`, and only touched once the
//! stack hooks are installed. Positions are source offsets plus one.

use super::*;
use std::cell::{Cell, RefCell};

#[derive(Clone, Copy, Default)]
pub(super) struct Track {
    last: u32,
    pending: bool,
}

/// `last` while the position of a node being parsed is not known yet: the
/// bits below are the depth of its frame.
const FRAME: u32 = 1 << 30;
/// An `import()` position that is an index into `RESOLVED`.
const SLOT: u32 = 1 << 31;

thread_local! {
    static TRACK: Cell<Track> = const { Cell::new(Track { last: 0, pending: false }) };
    static BEFORE_EVENT: Cell<Track> = const { Cell::new(Track { last: 0, pending: false }) };
    static NODE: Cell<u32> = const { Cell::new(0) };
    static FRAMES: RefCell<Vec<Vec<usize>>> = const { RefCell::new(Vec::new()) };
    static RESOLVED: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

fn update(change: impl FnOnce(&mut Track)) {
    let mut track = TRACK.get();
    change(&mut track);
    TRACK.set(track);
}

pub(super) fn at(offset: usize) -> u32 {
    u32::try_from(offset + 1).unwrap_or(0)
}

/// A statement position: pending until an instruction is emitted.
pub(super) fn statement(position: u32) {
    if inferred_names::enabled() {
        update(|t| {
            *t = Track {
                last: position,
                pending: true,
            }
        });
    }
}

/// An expression position set before its operand is evaluated: dropped while
/// a statement position is pending, which stays pending.
pub(super) fn early_expression(position: u32) {
    if inferred_names::enabled() {
        update(|t| {
            if !t.pending {
                t.last = position;
            }
        });
    }
}

/// An expression position, dropped while a statement position is pending.
pub(super) fn expression(position: u32) {
    if inferred_names::enabled() {
        BEFORE_EVENT.set(TRACK.get());
        NODE.set(position);
        update(|t| {
            if t.pending {
                t.pending = false;
            } else {
                t.last = position;
            }
        });
    }
}

/// The position of the node just completed, for the places that take it as
/// their statement position.
pub(super) fn node(position: u32) {
    if inferred_names::enabled() {
        NODE.set(position);
    }
}

/// An instruction that carries no position.
pub(super) fn instruction() {
    if inferred_names::enabled() {
        update(|t| t.pending = false);
    }
}

pub(super) fn snapshot() -> Track {
    TRACK.get()
}

pub(super) fn restore(track: Track) {
    if inferred_names::enabled() {
        TRACK.set(track);
    }
}

/// Starts a function: nothing recorded yet. Returns the enclosing state.
pub(super) fn enter_function() -> Track {
    let outer = TRACK.get();
    if inferred_names::enabled() {
        TRACK.set(Track::default());
    }
    outer
}

/// The state of a function whose parameter list starts at `position`.
pub(super) fn start_at(position: u32) {
    if inferred_names::enabled() {
        update(|t| t.last = position);
    }
}

/// `target = value`: the target is stored, not read. An identifier or a
/// pattern leaves no trace; a member access keeps the reads of its object
/// and key but not its own property load, the event just recorded.
pub(super) fn store_target(target: &Expression, op: AssignOp, before: Track) {
    if !inferred_names::enabled() || op != AssignOp::Assign {
        return;
    }
    match target {
        Expression::Member(..) => TRACK.set(BEFORE_EVENT.get()),
        _ => TRACK.set(before),
    }
}

/// A statement position for the node about to be parsed, known afterwards.
pub(super) fn begin_node() -> usize {
    if !inferred_names::enabled() {
        return 0;
    }
    let depth = FRAMES.with_borrow_mut(|frames| {
        frames.push(Vec::new());
        frames.len()
    });
    statement(FRAME | u32::try_from(depth).unwrap_or(0));
    depth
}

/// The node parsed since `begin_node` has its position now.
pub(super) fn end_node(depth: usize) {
    if depth == 0 {
        return;
    }
    let position = NODE.get();
    let waiting = FRAMES.with_borrow_mut(|frames| {
        let waiting = frames.get_mut(depth - 1).map(std::mem::take);
        frames.truncate(depth - 1);
        waiting.unwrap_or_default()
    });
    RESOLVED.with_borrow_mut(|slots| {
        for slot in waiting {
            slots[slot] = position;
        }
    });
    update(|t| {
        if t.last == FRAME | u32::try_from(depth).unwrap_or(0) {
            t.last = position;
        }
    });
}

/// The position to report for an `import()` whose keyword is at `keyword`.
pub(super) fn reported(keyword: u32) -> u32 {
    let last = TRACK.get().last;
    if !inferred_names::enabled() || last == 0 {
        return keyword;
    }
    if last & FRAME == 0 {
        return last;
    }
    let depth = usize::try_from(last & !FRAME).unwrap_or(0);
    let slot = RESOLVED.with_borrow_mut(|slots| {
        slots.push(keyword);
        slots.len() - 1
    });
    FRAMES.with_borrow_mut(|frames| {
        if let Some(frame) = frames.get_mut(depth.wrapping_sub(1)) {
            frame.push(slot);
        }
    });
    SLOT | u32::try_from(slot).unwrap_or(0)
}

/// The position an `Expression::Import` carries, once its parse is over.
pub(crate) fn resolve(position: u32) -> u32 {
    if position & SLOT == 0 {
        return position;
    }
    let slot = usize::try_from(position & !SLOT).unwrap_or(usize::MAX);
    RESOLVED.with_borrow(|slots| slots.get(slot).copied().unwrap_or(0))
}

impl Parser<'_> {
    /// A statement, or the part of one V8 records a position for, starts here.
    pub(super) fn track_statement(&self) {
        statement(at(self.current_token_start));
    }

    /// An expression V8 gives its own statement position, taken from its
    /// node: a loop condition, a `case` test, a loop's subject.
    pub(super) fn parse_cond(&mut self) -> Result<Expression, ParseError> {
        let depth = begin_node();
        let result = self.parse_expression();
        end_node(depth);
        result
    }

    /// The parameters of a function, which V8 compiles inside the function.
    pub(super) fn parse_formal_parameters(&mut self) -> Result<Vec<Pattern>, ParseError> {
        let outer = enter_function();
        start_at(at(self.current_token_start));
        let result = self.parse_param_list();
        restore(outer);
        result
    }

    /// `=> expression`: V8 returns the expression, a statement of the arrow.
    pub(super) fn parse_arrow_concise_body(&mut self) -> Result<Expression, ParseError> {
        let outer = enter_function();
        let depth = begin_node();
        let result = self.parse_assignment_expression();
        end_node(depth);
        restore(outer);
        result
    }

    /// The right operand of a comma operator.
    pub(super) fn parse_comma_operand(&mut self) -> Result<Expression, ParseError> {
        let depth = begin_node();
        let result = self.parse_assignment_expression();
        end_node(depth);
        result
    }

    /// `key: value`: V8 records the value's start, except for the constants of
    /// a boilerplate object literal.
    pub(super) fn track_property_value(&self) {
        if !matches!(
            self.current,
            Token::NumericLiteral(_)
                | Token::StringLiteral(_)
                | Token::BooleanLiteral(_)
                | Token::NullLiteral
                | Token::LeftBrace
                | Token::LeftBracket
        ) {
            expression(at(self.current_token_start));
        }
    }

    /// The variable or constant the current token names is about to be read.
    pub(super) fn track_primary(&self) -> Option<(Track, u32)> {
        if !inferred_names::enabled() {
            return None;
        }
        let before = snapshot();
        let start = at(self.current_token_start);
        let spread_first = matches!(self.current, Token::LeftBracket | Token::LeftBrace)
            && self.lookahead(|tokens| {
                tokens.advance().is_ok() && tokens.current() == &Token::Ellipsis
            });
        match &self.current {
            Token::LeftParen => {}
            Token::TemplateHead(head, _) if head.as_ref().is_none_or(|h| h.is_empty()) => {}
            _ if spread_first => {}
            Token::Identifier(_) | Token::IdentifierWithEscape(_) => expression(start),
            Token::Keyword(Keyword::Yield | Keyword::Await | Keyword::Let | Keyword::Static) => {
                expression(start)
            }
            _ => instruction(),
        }
        let node_start = if self.current == Token::LeftParen {
            0
        } else {
            start
        };
        Some((before, node_start))
    }

    /// An arrow function reads none of the names of its parameter list.
    pub(super) fn track_primary_end(&self, begun: Option<(Track, u32)>, result: &Expression) {
        let Some((before, start)) = begun else { return };
        if matches!(result, Expression::ArrowFunction(_)) {
            restore(before);
            instruction();
        }
        if start != 0 {
            node(start);
        }
    }
}
