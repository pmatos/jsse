use crate::interpreter::{Atom, GlobalHint};
use crate::types::{JsString, JsValue};

#[derive(Debug, Clone)]
pub(crate) enum Constant {
    Number(f64),
    String(JsString),
}

impl Constant {
    pub(crate) fn to_value(&self) -> JsValue {
        match self {
            Constant::Number(n) => JsValue::number(*n),
            Constant::String(s) => JsValue::string(s.clone()),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Chunk {
    pub(crate) code: Vec<u8>,
    pub(crate) constants: Vec<Constant>,
    pub(crate) names: Vec<Atom>,
    /// One remembered global-object property slot per `names` entry, consulted
    /// by the name ops (`LoadName`, `ResolveName`, ...). Verified by key on
    /// every use, so it is never an invalidation concern. May be shorter than
    /// `names` (hand-built chunks): a missing entry means no hint.
    pub(crate) global_hints: Vec<GlobalHint>,
    pub(crate) var_names: Vec<u16>,
    pub(crate) max_stack: u16,
    pub(crate) max_refs: u16,
}

impl Chunk {
    #[inline]
    pub(crate) fn global_hint(&self, name_idx: u16) -> Option<&GlobalHint> {
        self.global_hints.get(usize::from(name_idx))
    }
}
