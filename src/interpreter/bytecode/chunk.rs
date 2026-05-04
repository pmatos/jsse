use crate::types::{JsString, JsValue};
use std::rc::Rc;

#[derive(Debug, Clone)]
pub(crate) enum Constant {
    Number(f64),
    String(Rc<str>),
}

impl Constant {
    pub(crate) fn to_value(&self) -> JsValue {
        match self {
            Constant::Number(n) => JsValue::Number(*n),
            Constant::String(s) => JsValue::String(JsString::from_str(s)),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct Chunk {
    pub(crate) code: Vec<u8>,
    pub(crate) constants: Vec<Constant>,
    pub(crate) names: Vec<Rc<str>>,
    pub(crate) max_stack: u16,
    pub(crate) num_params: u16,
}
