//! ArrayBuffer, SharedArrayBuffer and view cloning for `structuredClone`.

use super::super::super::*;
use super::Cloner;
use crate::interpreter::types::{
    ArrayBufferData, BufferData, DataViewInfo, ObjectKind, TypedArrayInfo, TypedArrayKind,
};

fn typed_array_prototype(interp: &Interpreter, kind: TypedArrayKind) -> Option<u64> {
    let realm = interp.realm();
    match kind {
        TypedArrayKind::Int8 => realm.int8array_prototype,
        TypedArrayKind::Uint8 => realm.uint8array_prototype,
        TypedArrayKind::Uint8Clamped => realm.uint8clampedarray_prototype,
        TypedArrayKind::Int16 => realm.int16array_prototype,
        TypedArrayKind::Uint16 => realm.uint16array_prototype,
        TypedArrayKind::Int32 => realm.int32array_prototype,
        TypedArrayKind::Uint32 => realm.uint32array_prototype,
        TypedArrayKind::Float16 => realm.float16array_prototype,
        TypedArrayKind::Float32 => realm.float32array_prototype,
        TypedArrayKind::Float64 => realm.float64array_prototype,
        TypedArrayKind::BigInt64 => realm.bigint64array_prototype,
        TypedArrayKind::BigUint64 => realm.biguint64array_prototype,
    }
}

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
                return Err(interp.data_clone_error("#<ArrayBuffer> could not be cloned."));
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
            let bytes = match &*data.data.borrow() {
                BufferData::Owned(v) => v.clone(),
                BufferData::Shared(_) => Vec::new(),
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
            return Err(interp.data_clone_error("#<Object> could not be cloned."));
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
            None => Err(interp.data_clone_error("#<Object> could not be cloned.")),
        }
    }

    pub(super) fn clone_typed_array(
        &mut self,
        interp: &mut Interpreter,
        id: u64,
        mut info: TypedArrayInfo,
    ) -> Result<u64, JsValue> {
        let (buffer_id, data, detached) = self.clone_view_buffer(interp, info.buffer_object_id)?;
        let proto = typed_array_prototype(interp, info.kind);
        info.buffer = data;
        info.is_detached = detached;
        let Some(proto) = proto else {
            return Err(interp.data_clone_error("#<Object> could not be cloned."));
        };
        let clone =
            interp.create_typed_array_object_with_proto(info, JsValue::object(buffer_id), proto);
        self.register(interp, id, clone);
        Ok(clone)
    }

    pub(super) fn clone_data_view(
        &mut self,
        interp: &mut Interpreter,
        id: u64,
        mut info: DataViewInfo,
    ) -> Result<u64, JsValue> {
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
