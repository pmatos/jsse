pub(crate) mod chunk;
pub(crate) mod compiler;
pub(crate) mod op;
pub(crate) mod vm;

#[cfg(test)]
mod tests;

use std::rc::Rc;

#[derive(Debug, Clone, Default)]
pub(crate) enum BytecodeCacheState {
    #[default]
    Untried,
    /// The compile-bail reason, kept so a counters build can label the BODY
    /// row of every closure that shares the verdict, not just the first.
    #[cfg_attr(not(feature = "perf-counters"), allow(dead_code))]
    Ineligible(&'static str),
    Compiled(Rc<chunk::Chunk>),
}
