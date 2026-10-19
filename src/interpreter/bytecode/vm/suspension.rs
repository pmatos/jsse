//! Reading what a suspended frame is waiting on.

use super::{Chunk, GeneratorFrame, YIELD_SKIP_BACK, decode_i16};

impl GeneratorFrame {
    /// Whether the frame is parked at an `await` rather than a `yield`: an
    /// `await` has no return stub, so its skip offset is zero.
    pub(crate) fn suspended_at_await(&self, chunk: &Chunk) -> bool {
        decode_i16(chunk, self.control.pc - YIELD_SKIP_BACK) == 0
    }
}
