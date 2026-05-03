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
    Ineligible,
    Compiled(Rc<chunk::Chunk>),
}
