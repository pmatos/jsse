//! ArrayBuffer, SharedArrayBuffer and view cloning for `structuredClone`.

use super::super::super::*;
use super::Cloner;
use crate::interpreter::types::{
    ArrayBufferData, BufferData, DataViewInfo, ObjectKind, TypedArrayInfo,
    is_typed_array_out_of_bounds,
};

impl Cloner {
    pub(super) fn clone_buffer(
        &mut self,
        interp: &mut Interpreter,
        id: u64,
    ) -> Result<u64, JsValue> {
        let data = {
            let cell = interp.get_object_cell_expect(id).borrow();
            let Some(ab) = cell.arraybuffer() else {
                drop(cell);
                return Err(interp.uncloneable("ArrayBuffer"));
            };
            ab.clone()
        };
        let clone_data = if data.is_shared {
            data
        } else {
            if data.detached.as_ref().is_some_and(|d| d.get()) {
                return Err(
                    interp.data_clone_error("An ArrayBuffer is detached and could not be cloned.")
                );
            }
            if self.transfer.contains(&id) && data.data.borrow().is_foreign() {
                return Err(interp.data_clone_error(
                    "An ArrayBuffer of a WebAssembly.Memory cannot be transferred.",
                ));
            }
            let bytes = if self.transfer.contains(&id) {
                Vec::new()
            } else {
                data.data.borrow().to_vec()
            };
            interp.gc_track_external_bytes(bytes.len());
            ArrayBufferData {
                data: Rc::new(RefCell::new(BufferData::Owned(bytes))),
                detached: Some(Rc::new(Cell::new(false))),
                ..data
            }
        };
        Ok(self.install_buffer(interp, id, clone_data))
    }

    /// Move the final bytes of a transferred `source` into its `clone`: a
    /// getter may have written to the buffer after the walk reached it.
    pub(super) fn move_bytes(interp: &mut Interpreter, source: &JsValue, clone: u64) {
        let Some(src) = source
            .as_object_id()
            .and_then(|id| interp.get_object_cell(id))
        else {
            return;
        };
        let bytes = {
            let src = src.borrow();
            let Some(ab) = src.arraybuffer() else { return };
            if ab.detached.as_ref().is_some_and(|d| d.get()) {
                return;
            }
            let BufferData::Owned(bytes) = &mut *ab.data.borrow_mut() else {
                return;
            };
            std::mem::take(bytes)
        };
        interp.gc_track_external_bytes(bytes.len());
        if let Some(dst) = interp
            .get_object_cell_expect(clone)
            .borrow()
            .arraybuffer_data()
        {
            *dst.borrow_mut() = BufferData::Owned(bytes);
        }
    }

    fn install_buffer(&mut self, interp: &mut Interpreter, id: u64, data: ArrayBufferData) -> u64 {
        let (class_name, proto) = if data.is_shared {
            (
                "SharedArrayBuffer",
                interp.realm().shared_arraybuffer_prototype,
            )
        } else {
            ("ArrayBuffer", interp.realm().arraybuffer_prototype)
        };
        let clone =
            interp.new_object_with(class_name, proto, ObjectKind::ArrayBuffer(Box::new(data)));
        self.register(interp, id, clone);
        clone
    }

    /// Clone the ArrayBuffer behind a view (through the memory map, so views of
    /// one buffer share one clone) and return it with its storage handles.
    fn clone_view_buffer(
        &mut self,
        interp: &mut Interpreter,
        buffer_id: Option<u64>,
    ) -> Result<(u64, Rc<RefCell<BufferData>>, Rc<Cell<bool>>), JsValue> {
        let Some(buffer_id) = buffer_id else {
            return Err(interp.uncloneable("Object"));
        };
        let cloned = self.clone_value(interp, &JsValue::object(buffer_id))?;
        let clone_id = cloned.as_object_id().unwrap_or(buffer_id);
        let cell = interp.get_object_cell_expect(clone_id).borrow();
        let data = cell.arraybuffer_data().cloned();
        let detached = cell.arraybuffer_detached().cloned();
        drop(cell);
        match data {
            Some(data) => Ok((
                clone_id,
                data,
                detached.unwrap_or_else(|| Rc::new(Cell::new(false))),
            )),
            None => Err(interp.uncloneable("Object")),
        }
    }

    pub(super) fn clone_typed_array(
        &mut self,
        interp: &mut Interpreter,
        id: u64,
        mut info: TypedArrayInfo,
    ) -> Result<u64, JsValue> {
        if is_typed_array_out_of_bounds(&info) {
            return Err(interp.uncloneable("Object"));
        }
        let (buffer_id, data, detached) = self.clone_view_buffer(interp, info.buffer_object_id)?;
        info.buffer = data;
        info.is_detached = detached;
        let clone = interp.create_typed_array_object(info, JsValue::object(buffer_id));
        self.register(interp, id, clone);
        Ok(clone)
    }

    pub(super) fn clone_data_view(
        &mut self,
        interp: &mut Interpreter,
        id: u64,
        mut info: DataViewInfo,
    ) -> Result<u64, JsValue> {
        let buf_len = info.buffer.borrow().len();
        let end = if info.is_length_tracking {
            info.byte_offset
        } else {
            info.byte_offset + info.byte_length
        };
        if info.is_detached.get() || end > buf_len {
            return Err(interp.uncloneable("Object"));
        }
        let (buffer_id, data, detached) = self.clone_view_buffer(interp, info.buffer_object_id)?;
        info.buffer = data;
        info.is_detached = detached;
        info.buffer_object_id = Some(buffer_id);
        let proto = interp.realm().dataview_prototype;
        let clone = interp.new_object_with("DataView", proto, ObjectKind::DataView(Box::new(info)));
        self.register(interp, id, clone);
        Ok(clone)
    }
}
