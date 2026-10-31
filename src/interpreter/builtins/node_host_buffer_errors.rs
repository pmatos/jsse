//! V8's range errors for typed array and DataView construction, under
//! `--node` only (#1532): they print the offending offset or length.

use super::super::*;

#[derive(Clone, Copy)]
pub(crate) enum BufferRange {
    TypedArrayOffset,
    TypedArrayLength,
    DataViewOffset,
    DataViewLength,
}

impl Interpreter {
    pub(crate) fn buffer_range_error(&mut self, which: BufferRange, value: usize) -> JsValue {
        let (engine, v8) = match which {
            BufferRange::TypedArrayOffset => (
                "start offset is outside the bounds of the buffer",
                format!("Start offset {value} is outside the bounds of the buffer"),
            ),
            BufferRange::TypedArrayLength => (
                "invalid typed array length",
                format!("Invalid typed array length: {value}"),
            ),
            BufferRange::DataViewOffset => (
                "offset is outside the bounds of the buffer",
                format!("Start offset {value} is outside the bounds of the buffer"),
            ),
            BufferRange::DataViewLength => (
                "invalid DataView length",
                format!("Invalid DataView length {value}"),
            ),
        };
        let text = self.v8_message(engine, |_| v8);
        self.create_range_error(&text)
    }
}
