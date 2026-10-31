//! Whether the node stack hooks are installed, as seen by the bytecode
//! compiler. The flag is per thread rather than per process: an interpreter is
//! confined to the thread that built it and compiles lazily on that thread, so
//! one interpreter's prelude must not change what another thread compiles
//! (the unit tests share a process, one thread per test; #1403).

use std::cell::Cell;

thread_local! {
    static CALL_POSITIONS: Cell<bool> = const { Cell::new(false) };
}

/// Whether the bytecode compiler should record call positions on this thread.
pub(crate) fn call_positions_enabled() -> bool {
    CALL_POSITIONS.get()
}

pub(super) fn enable_call_positions() {
    CALL_POSITIONS.set(true);
}
