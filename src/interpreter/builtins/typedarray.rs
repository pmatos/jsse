use super::super::*;
use crate::types::{JsBigInt, JsObject, JsString, JsValue};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

fn to_int32_modular(n: f64) -> i32 {
    if n.is_nan() || n.is_infinite() || n == 0.0 {
        return 0;
    }
    let n = n.trunc();
    let n = n % 4294967296.0; // 2^32
    let n = if n < 0.0 { n + 4294967296.0 } else { n };
    if n >= 2147483648.0 {
        (n - 4294967296.0) as i32
    } else {
        n as i32
    }
}

impl Interpreter {
    pub(crate) fn setup_typedarray_builtins(&mut self) {
        self.setup_arraybuffer();
        self.setup_shared_arraybuffer();
        self.setup_typed_array_base_prototype();
        self.setup_typed_array_constructors();
        self.setup_uint8array_base64_hex();
        self.setup_dataview();
    }

    fn setup_arraybuffer(&mut self) {
        let ab_proto = self.create_object();
        ab_proto.borrow_mut().class_name = "ArrayBuffer".to_string();
        self.realm_mut().arraybuffer_prototype = Some(ab_proto.clone());

        // byteLength getter
        let byte_length_getter = self.create_function(JsFunction::native(
            "get byteLength".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if let Some(ref buf) = obj_ref.arraybuffer_data {
                        if let Some(ref det) = obj_ref.arraybuffer_detached
                            && det.get()
                        {
                            return Completion::Normal(JsValue::Number(0.0));
                        }
                        return Completion::Normal(JsValue::Number(buf.borrow().len() as f64));
                    }
                }
                Completion::Throw(interp.create_type_error("not an ArrayBuffer"))
            },
        ));
        ab_proto.borrow_mut().insert_property(
            "byteLength".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(byte_length_getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );

        // detached getter
        let detached_getter = self.create_function(JsFunction::native(
            "get detached".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if obj_ref.arraybuffer_data.is_some() {
                        let detached = obj_ref
                            .arraybuffer_detached
                            .as_ref()
                            .is_some_and(|d| d.get());
                        return Completion::Normal(JsValue::Boolean(detached));
                    }
                }
                Completion::Throw(interp.create_type_error("not an ArrayBuffer"))
            },
        ));
        ab_proto.borrow_mut().insert_property(
            "detached".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(detached_getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );

        // resizable getter
        let resizable_getter = self.create_function(JsFunction::native(
            "get resizable".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if obj_ref.arraybuffer_data.is_some() {
                        let is_resizable = obj_ref.arraybuffer_max_byte_length.is_some();
                        return Completion::Normal(JsValue::Boolean(is_resizable));
                    }
                }
                Completion::Throw(interp.create_type_error("not an ArrayBuffer"))
            },
        ));
        ab_proto.borrow_mut().insert_property(
            "resizable".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(resizable_getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );

        // maxByteLength getter
        let max_byte_length_getter = self.create_function(JsFunction::native(
            "get maxByteLength".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if obj_ref.arraybuffer_data.is_some() {
                        if let Some(ref det) = obj_ref.arraybuffer_detached
                            && det.get()
                        {
                            return Completion::Normal(JsValue::Number(0.0));
                        }
                        let max = obj_ref.arraybuffer_max_byte_length.unwrap_or_else(|| {
                            obj_ref.arraybuffer_data.as_ref().unwrap().borrow().len()
                        });
                        return Completion::Normal(JsValue::Number(max as f64));
                    }
                }
                Completion::Throw(interp.create_type_error("not an ArrayBuffer"))
            },
        ));
        ab_proto.borrow_mut().insert_property(
            "maxByteLength".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(max_byte_length_getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );

        // slice
        let slice_fn = self.create_function(JsFunction::native(
            "slice".to_string(),
            2,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let (buf_data, buf_len) = {
                        let obj_ref = obj.borrow();
                        if let Some(ref det) = obj_ref.arraybuffer_detached
                            && det.get()
                        {
                            return Completion::Throw(
                                interp.create_type_error("ArrayBuffer is detached"),
                            );
                        }
                        if let Some(ref buf) = obj_ref.arraybuffer_data {
                            let b = buf.borrow();
                            (b.clone(), b.len())
                        } else {
                            return Completion::Throw(
                                interp.create_type_error("not an ArrayBuffer"),
                            );
                        }
                    };
                    let len = buf_len as f64;
                    let start_arg = if let Some(a) = args.first() {
                        match interp.to_number_value(a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        }
                    } else {
                        0.0
                    };
                    let start = if start_arg < 0.0 {
                        ((len + start_arg) as isize).max(0) as usize
                    } else {
                        (start_arg as usize).min(buf_len)
                    };
                    let end_arg = if args.len() > 1 && !matches!(args[1], JsValue::Undefined) {
                        match interp.to_number_value(&args[1]) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        }
                    } else {
                        len
                    };
                    let end = if end_arg < 0.0 {
                        ((len + end_arg) as isize).max(0) as usize
                    } else {
                        (end_arg as usize).min(buf_len)
                    };
                    let new_len = end.saturating_sub(start);
                    let new_buf: Vec<u8> = buf_data[start..start + new_len].to_vec();
                    let new_ab = interp.create_arraybuffer(new_buf);
                    let id = new_ab.borrow().id.unwrap();
                    return Completion::Normal(JsValue::Object(JsObject { id }));
                }
                Completion::Throw(interp.create_type_error("not an ArrayBuffer"))
            },
        ));
        ab_proto
            .borrow_mut()
            .insert_builtin("slice".to_string(), slice_fn);

        // transfer
        let transfer_fn = self.create_function(JsFunction::native(
            "transfer".to_string(),
            0,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let (is_ab, is_detached, old_len, max_byte_length) = {
                        let obj_ref = obj.borrow();
                        let is_ab = obj_ref.arraybuffer_data.is_some();
                        let is_detached = obj_ref
                            .arraybuffer_detached
                            .as_ref()
                            .is_some_and(|d| d.get());
                        let old_len = obj_ref
                            .arraybuffer_data
                            .as_ref()
                            .map(|b| b.borrow().len())
                            .unwrap_or(0);
                        let max = obj_ref.arraybuffer_max_byte_length;
                        (is_ab, is_detached, old_len, max)
                    };
                    if !is_ab {
                        return Completion::Throw(interp.create_type_error("not an ArrayBuffer"));
                    }
                    if is_detached {
                        return Completion::Throw(
                            interp.create_type_error("ArrayBuffer is detached"),
                        );
                    }
                    let new_len_arg = args.first().unwrap_or(&JsValue::Undefined);
                    let new_len = if matches!(new_len_arg, JsValue::Undefined) {
                        old_len
                    } else {
                        match interp.to_index(new_len_arg) {
                            Completion::Normal(JsValue::Number(n)) => n as usize,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => 0,
                        }
                    };
                    if let Some(max) = max_byte_length
                        && new_len > max
                    {
                        return Completion::Throw(
                            interp.create_error(
                                "RangeError",
                                "new byte length exceeds maxByteLength",
                            ),
                        );
                    }
                    let old_data = {
                        let obj_ref = obj.borrow();
                        obj_ref.arraybuffer_data.as_ref().unwrap().borrow().clone()
                    };
                    let mut new_data = vec![0u8; new_len];
                    let copy_len = old_len.min(new_len);
                    new_data[..copy_len].copy_from_slice(&old_data[..copy_len]);
                    {
                        let mut obj_ref = obj.borrow_mut();
                        if let Some(ref det) = obj_ref.arraybuffer_detached {
                            det.set(true);
                        }
                        obj_ref.arraybuffer_data = Some(Rc::new(RefCell::new(Vec::new())));
                    }
                    let new_ab = interp.create_arraybuffer_resizable(new_data, max_byte_length);
                    let id = new_ab.borrow().id.unwrap();
                    return Completion::Normal(JsValue::Object(JsObject { id }));
                }
                Completion::Throw(interp.create_type_error("not an ArrayBuffer"))
            },
        ));
        ab_proto
            .borrow_mut()
            .insert_builtin("transfer".to_string(), transfer_fn);

        // transferToFixedLength (identical to transfer for non-resizable buffers)
        let transfer_fixed_fn = self.create_function(JsFunction::native(
            "transferToFixedLength".to_string(),
            0,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let (is_ab, is_detached, old_len) = {
                        let obj_ref = obj.borrow();
                        let is_ab = obj_ref.arraybuffer_data.is_some();
                        let is_detached = obj_ref
                            .arraybuffer_detached
                            .as_ref()
                            .is_some_and(|d| d.get());
                        let old_len = obj_ref
                            .arraybuffer_data
                            .as_ref()
                            .map(|b| b.borrow().len())
                            .unwrap_or(0);
                        (is_ab, is_detached, old_len)
                    };
                    if !is_ab {
                        return Completion::Throw(interp.create_type_error("not an ArrayBuffer"));
                    }
                    if is_detached {
                        return Completion::Throw(
                            interp.create_type_error("ArrayBuffer is detached"),
                        );
                    }
                    let new_len_arg = args.first().unwrap_or(&JsValue::Undefined);
                    let new_len = if matches!(new_len_arg, JsValue::Undefined) {
                        old_len
                    } else {
                        match interp.to_index(new_len_arg) {
                            Completion::Normal(JsValue::Number(n)) => n as usize,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => 0,
                        }
                    };
                    let old_data = {
                        let obj_ref = obj.borrow();
                        obj_ref.arraybuffer_data.as_ref().unwrap().borrow().clone()
                    };
                    let mut new_data = vec![0u8; new_len];
                    let copy_len = old_len.min(new_len);
                    new_data[..copy_len].copy_from_slice(&old_data[..copy_len]);
                    {
                        let mut obj_ref = obj.borrow_mut();
                        if let Some(ref det) = obj_ref.arraybuffer_detached {
                            det.set(true);
                        }
                        obj_ref.arraybuffer_data = Some(Rc::new(RefCell::new(Vec::new())));
                    }
                    let new_ab = interp.create_arraybuffer(new_data);
                    let id = new_ab.borrow().id.unwrap();
                    return Completion::Normal(JsValue::Object(JsObject { id }));
                }
                Completion::Throw(interp.create_type_error("not an ArrayBuffer"))
            },
        ));
        ab_proto
            .borrow_mut()
            .insert_builtin("transferToFixedLength".to_string(), transfer_fixed_fn);

        // resize
        let resize_fn = self.create_function(JsFunction::native(
            "resize".to_string(),
            1,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let (is_ab, is_detached, max_byte_length) = {
                        let obj_ref = obj.borrow();
                        let is_ab = obj_ref.arraybuffer_data.is_some();
                        let is_detached = obj_ref
                            .arraybuffer_detached
                            .as_ref()
                            .is_some_and(|d| d.get());
                        let max = obj_ref.arraybuffer_max_byte_length;
                        (is_ab, is_detached, max)
                    };
                    if !is_ab {
                        return Completion::Throw(interp.create_type_error("not an ArrayBuffer"));
                    }
                    if max_byte_length.is_none() {
                        return Completion::Throw(
                            interp.create_type_error("ArrayBuffer is not resizable"),
                        );
                    }
                    if is_detached {
                        return Completion::Throw(
                            interp.create_type_error("ArrayBuffer is detached"),
                        );
                    }
                    let new_len_val = args.first().unwrap_or(&JsValue::Undefined);
                    let new_len = match interp.to_index(new_len_val) {
                        Completion::Normal(JsValue::Number(n)) => n as usize,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => 0,
                    };
                    if new_len > max_byte_length.unwrap() {
                        return Completion::Throw(
                            interp.create_error(
                                "RangeError",
                                "new byte length exceeds maxByteLength",
                            ),
                        );
                    }
                    let obj_ref = obj.borrow();
                    let buf = obj_ref.arraybuffer_data.as_ref().unwrap();
                    buf.borrow_mut().resize(new_len, 0u8);
                    return Completion::Normal(JsValue::Undefined);
                }
                Completion::Throw(interp.create_type_error("not an ArrayBuffer"))
            },
        ));
        ab_proto
            .borrow_mut()
            .insert_builtin("resize".to_string(), resize_fn);

        // @@toStringTag
        let tag = JsValue::String(JsString::from_str("ArrayBuffer"));
        let sym_key = "Symbol(Symbol.toStringTag)".to_string();
        ab_proto
            .borrow_mut()
            .insert_property(sym_key, PropertyDescriptor::data(tag, false, false, true));

        // ArrayBuffer constructor
        let ab_proto_clone = ab_proto.clone();
        let ctor = self.create_function(JsFunction::constructor(
            "ArrayBuffer".to_string(),
            1,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("Constructor ArrayBuffer requires 'new'"),
                    );
                }
                let len_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let len = match interp.to_index(&len_val) {
                    Completion::Normal(JsValue::Number(n)) => n as usize,
                    Completion::Throw(e) => return Completion::Throw(e),
                    _ => 0,
                };
                let max_byte_length = if args.len() > 1 {
                    if let JsValue::Object(opts_o) = &args[1] {
                        if let Some(opts_obj) = interp.get_object(opts_o.id) {
                            let max_val = opts_obj.borrow().get_property("maxByteLength");
                            if !matches!(max_val, JsValue::Undefined) {
                                let max = match interp.to_index(&max_val) {
                                    Completion::Normal(JsValue::Number(n)) => n as usize,
                                    Completion::Throw(e) => return Completion::Throw(e),
                                    _ => 0,
                                };
                                if max < len {
                                    return Completion::Throw(interp.create_error(
                                        "RangeError",
                                        "maxByteLength must be at least as large as byteLength",
                                    ));
                                }
                                Some(max)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };
                let buf = if let Some(max_len) = max_byte_length {
                    let mut v = Vec::with_capacity(max_len);
                    v.resize(len, 0u8);
                    v
                } else {
                    vec![0u8; len]
                };
                let buf_rc = Rc::new(RefCell::new(buf));
                let detached = Rc::new(Cell::new(false));
                let proto = if let Some(ref nt) = interp.new_target {
                    if let JsValue::Object(o) = nt
                        && let Some(nt_obj) = interp.get_object(o.id)
                    {
                        let proto_val = nt_obj.borrow().get_property("prototype");
                        if let JsValue::Object(po) = &proto_val {
                            if let Some(p) = interp.get_object(po.id) {
                                Some(p.clone())
                            } else {
                                Some(ab_proto_clone.clone())
                            }
                        } else {
                            Some(ab_proto_clone.clone())
                        }
                    } else {
                        Some(ab_proto_clone.clone())
                    }
                } else {
                    Some(ab_proto_clone.clone())
                };
                let obj = interp.create_object();
                {
                    let mut o = obj.borrow_mut();
                    o.class_name = "ArrayBuffer".to_string();
                    o.prototype = proto;
                    o.arraybuffer_data = Some(buf_rc);
                    o.arraybuffer_detached = Some(detached);
                    o.arraybuffer_max_byte_length = max_byte_length;
                }
                let id = obj.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(JsObject { id }))
            },
        ));

        // ArrayBuffer.isView
        let is_view_fn = self.create_function(JsFunction::native(
            "isView".to_string(),
            1,
            |interp, _this, args| {
                let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = &arg
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if obj_ref.typed_array_info.is_some() || obj_ref.data_view_info.is_some() {
                        return Completion::Normal(JsValue::Boolean(true));
                    }
                }
                Completion::Normal(JsValue::Boolean(false))
            },
        ));
        // Wire ArrayBuffer.prototype to the proto object with all the methods
        let ab_proto_val = {
            let id = ab_proto.borrow().id.unwrap();
            JsValue::Object(crate::types::JsObject { id })
        };
        if let JsValue::Object(o) = &ctor
            && let Some(obj) = self.get_object(o.id)
        {
            obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(ab_proto_val, false, false, false),
            );
            obj.borrow_mut()
                .insert_builtin("isView".to_string(), is_view_fn);

            // ArrayBuffer[Symbol.species] getter
            let species_getter = self.create_function(JsFunction::native(
                "get [Symbol.species]".to_string(),
                0,
                |_interp, this_val, _args| Completion::Normal(this_val.clone()),
            ));
            obj.borrow_mut().insert_property(
                "Symbol(Symbol.species)".to_string(),
                PropertyDescriptor {
                    value: None,
                    writable: None,
                    get: Some(species_getter),
                    set: None,
                    enumerable: Some(false),
                    configurable: Some(true),
                },
            );
        }
        ab_proto.borrow_mut().insert_property(
            "constructor".to_string(),
            PropertyDescriptor::data(ctor.clone(), true, false, true),
        );

        self.realm().global_env
            .borrow_mut()
            .declare("ArrayBuffer", BindingKind::Var);
        let _ = self.realm().global_env.borrow_mut().set("ArrayBuffer", ctor);
    }

    pub(crate) fn create_arraybuffer(&mut self, data: Vec<u8>) -> Rc<RefCell<JsObjectData>> {
        self.create_arraybuffer_resizable(data, None)
    }

    pub(crate) fn create_arraybuffer_resizable(
        &mut self,
        data: Vec<u8>,
        max_byte_length: Option<usize>,
    ) -> Rc<RefCell<JsObjectData>> {
        let buf_rc = Rc::new(RefCell::new(data));
        let detached = Rc::new(Cell::new(false));
        let obj = self.create_object();
        {
            let mut o = obj.borrow_mut();
            o.class_name = "ArrayBuffer".to_string();
            o.prototype = self.realm().arraybuffer_prototype.clone();
            o.arraybuffer_data = Some(buf_rc);
            o.arraybuffer_detached = Some(detached);
            o.arraybuffer_max_byte_length = max_byte_length;
        }
        obj
    }

    pub(crate) fn detach_arraybuffer(&mut self, ab_val: &JsValue) -> Completion {
        if let JsValue::Object(o) = ab_val
            && let Some(obj) = self.get_object(o.id)
        {
            let mut obj_ref = obj.borrow_mut();
            if obj_ref.arraybuffer_data.is_some() {
                if obj_ref.arraybuffer_is_shared {
                    return Completion::Throw(
                        self.create_type_error("Cannot detach a SharedArrayBuffer"),
                    );
                }
                if let Some(ref det) = obj_ref.arraybuffer_detached {
                    det.set(true);
                }
                obj_ref.arraybuffer_data = Some(Rc::new(RefCell::new(Vec::new())));
                return Completion::Normal(JsValue::Undefined);
            }
        }
        Completion::Throw(self.create_type_error("not an ArrayBuffer"))
    }

    pub(crate) fn create_shared_arraybuffer(
        &mut self,
        data: Vec<u8>,
        max_byte_length: Option<usize>,
    ) -> Rc<RefCell<JsObjectData>> {
        let buf_rc = Rc::new(RefCell::new(data));
        let obj = self.create_object();
        {
            let mut o = obj.borrow_mut();
            o.class_name = "SharedArrayBuffer".to_string();
            o.prototype = self.realm().shared_arraybuffer_prototype.clone();
            o.arraybuffer_data = Some(buf_rc);
            o.arraybuffer_detached = None;
            o.arraybuffer_max_byte_length = max_byte_length;
            o.arraybuffer_is_shared = true;
        }
        obj
    }

    fn setup_shared_arraybuffer(&mut self) {
        let sab_proto = self.create_object();
        sab_proto.borrow_mut().class_name = "SharedArrayBuffer".to_string();
        self.realm_mut().shared_arraybuffer_prototype = Some(sab_proto.clone());

        // byteLength getter
        let byte_length_getter = self.create_function(JsFunction::native(
            "get byteLength".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if obj_ref.arraybuffer_is_shared
                        && let Some(ref buf) = obj_ref.arraybuffer_data
                    {
                        return Completion::Normal(JsValue::Number(buf.borrow().len() as f64));
                    }
                }
                Completion::Throw(interp.create_type_error("not a SharedArrayBuffer"))
            },
        ));
        sab_proto.borrow_mut().insert_property(
            "byteLength".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(byte_length_getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );

        // maxByteLength getter
        let max_byte_length_getter = self.create_function(JsFunction::native(
            "get maxByteLength".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if obj_ref.arraybuffer_is_shared
                        && let Some(ref buf) = obj_ref.arraybuffer_data
                    {
                        let max = obj_ref
                            .arraybuffer_max_byte_length
                            .unwrap_or_else(|| buf.borrow().len());
                        return Completion::Normal(JsValue::Number(max as f64));
                    }
                }
                Completion::Throw(interp.create_type_error("not a SharedArrayBuffer"))
            },
        ));
        sab_proto.borrow_mut().insert_property(
            "maxByteLength".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(max_byte_length_getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );

        // growable getter
        let growable_getter = self.create_function(JsFunction::native(
            "get growable".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if obj_ref.arraybuffer_is_shared {
                        return Completion::Normal(JsValue::Boolean(
                            obj_ref.arraybuffer_max_byte_length.is_some(),
                        ));
                    }
                }
                Completion::Throw(interp.create_type_error("not a SharedArrayBuffer"))
            },
        ));
        sab_proto.borrow_mut().insert_property(
            "growable".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(growable_getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );

        // grow(newLength)
        let grow_fn = self.create_function(JsFunction::native(
            "grow".to_string(),
            1,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let (is_shared, max_byte_length) = {
                        let obj_ref = obj.borrow();
                        (
                            obj_ref.arraybuffer_is_shared,
                            obj_ref.arraybuffer_max_byte_length,
                        )
                    };
                    if !is_shared {
                        return Completion::Throw(
                            interp.create_type_error("not a SharedArrayBuffer"),
                        );
                    }
                    if max_byte_length.is_none() {
                        return Completion::Throw(
                            interp.create_type_error("SharedArrayBuffer is not growable"),
                        );
                    }
                    let new_len_val = args.first().unwrap_or(&JsValue::Undefined);
                    let new_len = match interp.to_index(new_len_val) {
                        Completion::Normal(JsValue::Number(n)) => n as usize,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => 0,
                    };
                    if new_len > max_byte_length.unwrap() {
                        return Completion::Throw(
                            interp.create_error(
                                "RangeError",
                                "new byte length exceeds maxByteLength",
                            ),
                        );
                    }
                    let obj_ref = obj.borrow();
                    let buf = obj_ref.arraybuffer_data.as_ref().unwrap();
                    let current_len = buf.borrow().len();
                    if new_len < current_len {
                        return Completion::Throw(
                            interp.create_error("RangeError", "SharedArrayBuffer cannot shrink"),
                        );
                    }
                    buf.borrow_mut().resize(new_len, 0u8);
                    return Completion::Normal(JsValue::Undefined);
                }
                Completion::Throw(interp.create_type_error("not a SharedArrayBuffer"))
            },
        ));
        sab_proto
            .borrow_mut()
            .insert_builtin("grow".to_string(), grow_fn);

        // slice(start, end)
        let slice_fn = self.create_function(JsFunction::native(
            "slice".to_string(),
            2,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let (is_shared, buf_len, buf_data) = {
                        let obj_ref = obj.borrow();
                        if !obj_ref.arraybuffer_is_shared {
                            return Completion::Throw(
                                interp.create_type_error("not a SharedArrayBuffer"),
                            );
                        }
                        if let Some(ref buf) = obj_ref.arraybuffer_data {
                            let b = buf.borrow();
                            (true, b.len(), b.clone())
                        } else {
                            return Completion::Throw(
                                interp.create_type_error("not a SharedArrayBuffer"),
                            );
                        }
                    };
                    let _ = is_shared;
                    let len = buf_len as f64;
                    let start_arg = if let Some(a) = args.first() {
                        match interp.to_number_value(a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        }
                    } else {
                        0.0
                    };
                    let start = if start_arg.is_nan() {
                        0
                    } else if start_arg < 0.0 {
                        ((len + start_arg) as isize).max(0) as usize
                    } else {
                        (start_arg as usize).min(buf_len)
                    };
                    let end_arg = if args.len() > 1 && !matches!(args[1], JsValue::Undefined) {
                        match interp.to_number_value(&args[1]) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        }
                    } else {
                        len
                    };
                    let end = if end_arg.is_nan() {
                        0
                    } else if end_arg < 0.0 {
                        ((len + end_arg) as isize).max(0) as usize
                    } else {
                        (end_arg as usize).min(buf_len)
                    };
                    let new_len = end.saturating_sub(start);
                    let new_buf: Vec<u8> = buf_data[start..start + new_len].to_vec();
                    let new_sab = interp.create_shared_arraybuffer(new_buf, None);
                    let id = new_sab.borrow().id.unwrap();
                    return Completion::Normal(JsValue::Object(JsObject { id }));
                }
                Completion::Throw(interp.create_type_error("not a SharedArrayBuffer"))
            },
        ));
        sab_proto
            .borrow_mut()
            .insert_builtin("slice".to_string(), slice_fn);

        // @@toStringTag
        {
            let tag = JsValue::String(JsString::from_str("SharedArrayBuffer"));
            let sym_key = "Symbol(Symbol.toStringTag)".to_string();
            let desc = PropertyDescriptor::data(tag, false, false, true);
            sab_proto.borrow_mut().property_order.push(sym_key.clone());
            sab_proto.borrow_mut().properties.insert(sym_key, desc);
        }

        // SharedArrayBuffer constructor
        let sab_proto_clone = sab_proto.clone();
        let ctor = self.create_function(JsFunction::constructor(
            "SharedArrayBuffer".to_string(),
            1,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("Constructor SharedArrayBuffer requires 'new'"),
                    );
                }
                let len_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let len = match interp.to_index(&len_val) {
                    Completion::Normal(JsValue::Number(n)) => n as usize,
                    Completion::Throw(e) => return Completion::Throw(e),
                    _ => 0,
                };
                let max_byte_length = if args.len() > 1 {
                    if let JsValue::Object(opts_o) = &args[1] {
                        if let Some(opts_obj) = interp.get_object(opts_o.id) {
                            let max_val = opts_obj.borrow().get_property("maxByteLength");
                            if !matches!(max_val, JsValue::Undefined) {
                                let max = match interp.to_index(&max_val) {
                                    Completion::Normal(JsValue::Number(n)) => n as usize,
                                    Completion::Throw(e) => return Completion::Throw(e),
                                    _ => 0,
                                };
                                if max < len {
                                    return Completion::Throw(interp.create_error(
                                        "RangeError",
                                        "maxByteLength must be at least as large as byteLength",
                                    ));
                                }
                                Some(max)
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };
                let buf = vec![0u8; len];
                let buf_rc = Rc::new(RefCell::new(buf));
                let proto = if let Some(ref nt) = interp.new_target {
                    if let JsValue::Object(o) = nt
                        && let Some(nt_obj) = interp.get_object(o.id)
                    {
                        let proto_val = nt_obj.borrow().get_property("prototype");
                        if let JsValue::Object(po) = &proto_val {
                            if let Some(p) = interp.get_object(po.id) {
                                Some(p.clone())
                            } else {
                                Some(sab_proto_clone.clone())
                            }
                        } else {
                            Some(sab_proto_clone.clone())
                        }
                    } else {
                        Some(sab_proto_clone.clone())
                    }
                } else {
                    Some(sab_proto_clone.clone())
                };
                let obj = interp.create_object();
                {
                    let mut o = obj.borrow_mut();
                    o.class_name = "SharedArrayBuffer".to_string();
                    o.prototype = proto;
                    o.arraybuffer_data = Some(buf_rc);
                    o.arraybuffer_detached = None;
                    o.arraybuffer_max_byte_length = max_byte_length;
                    o.arraybuffer_is_shared = true;
                }
                let id = obj.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(JsObject { id }))
            },
        ));

        // Wire SharedArrayBuffer.prototype
        let sab_proto_val = {
            let id = sab_proto.borrow().id.unwrap();
            JsValue::Object(crate::types::JsObject { id })
        };
        if let JsValue::Object(o) = &ctor
            && let Some(obj) = self.get_object(o.id)
        {
            obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(sab_proto_val, false, false, false),
            );

            // SharedArrayBuffer[Symbol.species] getter
            let species_getter = self.create_function(JsFunction::native(
                "get [Symbol.species]".to_string(),
                0,
                |_interp, this_val, _args| Completion::Normal(this_val.clone()),
            ));
            obj.borrow_mut().insert_property(
                "Symbol(Symbol.species)".to_string(),
                PropertyDescriptor {
                    value: None,
                    writable: None,
                    get: Some(species_getter),
                    set: None,
                    enumerable: Some(false),
                    configurable: Some(true),
                },
            );
        }
        sab_proto.borrow_mut().insert_property(
            "constructor".to_string(),
            PropertyDescriptor::data(ctor.clone(), true, false, true),
        );

        self.realm().global_env
            .borrow_mut()
            .declare("SharedArrayBuffer", BindingKind::Var);
        let _ = self.realm().global_env.borrow_mut().set("SharedArrayBuffer", ctor);
    }

    fn setup_typed_array_base_prototype(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "TypedArray".to_string();
        self.realm_mut().typed_array_prototype = Some(proto.clone());

        // byteOffset getter
        let byte_offset_getter = self.create_function(JsFunction::native(
            "get byteOffset".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if let Some(ref ta) = obj_ref.typed_array_info {
                        if ta.is_detached.get() || is_typed_array_out_of_bounds(ta) {
                            return Completion::Normal(JsValue::Number(0.0));
                        }
                        return Completion::Normal(JsValue::Number(ta.byte_offset as f64));
                    }
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto.borrow_mut().insert_property(
            "byteOffset".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(byte_offset_getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );
        // byteLength getter
        let byte_length_getter = self.create_function(JsFunction::native(
            "get byteLength".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if let Some(ref ta) = obj_ref.typed_array_info {
                        if ta.is_detached.get() || is_typed_array_out_of_bounds(ta) {
                            return Completion::Normal(JsValue::Number(0.0));
                        }
                        return Completion::Normal(JsValue::Number(
                            typed_array_byte_length(ta) as f64
                        ));
                    }
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto.borrow_mut().insert_property(
            "byteLength".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(byte_length_getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );
        // length getter
        let length_getter = self.create_function(JsFunction::native(
            "get length".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if let Some(ref ta) = obj_ref.typed_array_info {
                        if ta.is_detached.get() || is_typed_array_out_of_bounds(ta) {
                            return Completion::Normal(JsValue::Number(0.0));
                        }
                        return Completion::Normal(JsValue::Number(typed_array_length(ta) as f64));
                    }
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto.borrow_mut().insert_property(
            "length".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(length_getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );

        // buffer getter (returns the ArrayBuffer object - we need to find it)
        let buffer_getter = self.create_function(JsFunction::native(
            "get buffer".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if obj_ref.typed_array_info.is_some() {
                        if let Some(buf_id) = obj_ref.view_buffer_object_id {
                            return Completion::Normal(JsValue::Object(JsObject { id: buf_id }));
                        }
                    }
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto.borrow_mut().insert_property(
            "buffer".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(buffer_getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );

        // [Symbol.iterator] = values
        self.setup_ta_values_method(&proto);

        // entries, keys, values
        self.setup_ta_iterator_methods(&proto);

        // at
        let at_fn = self.create_function(JsFunction::native(
            "at".to_string(),
            1,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info {
                            if ta.is_detached.get() || is_typed_array_out_of_bounds(ta) {
                                return Completion::Throw(
                                    interp.create_type_error("typed array is detached"),
                                );
                            }
                            ta.clone()
                        } else {
                            return Completion::Throw(interp.create_type_error("not a TypedArray"));
                        }
                    };
                    // Capture len BEFORE argument coercion (may resize buffer)
                    let len = typed_array_length(&ta) as i64;
                    let idx_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let idx = match interp.to_number_value(&idx_val) {
                        Ok(n) => to_integer(n) as i64,
                        Err(e) => return Completion::Throw(e),
                    };
                    let actual = if idx < 0 { len + idx } else { idx };
                    if actual < 0 || actual >= len {
                        return Completion::Normal(JsValue::Undefined);
                    }
                    // Use Get semantics (checks OOB post-resize via is_valid_integer_index)
                    let key = actual.to_string();
                    return interp.get_object_property(o.id, &key, this_val);
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto.borrow_mut().insert_builtin("at".to_string(), at_fn);

        // set
        let set_fn = self.create_function(JsFunction::native(
            "set".to_string(),
            1,
            |interp, this_val, args| {
                // Step 1: ValidateTypedArray(this)
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info {
                            ta.clone()
                        } else {
                            return Completion::Throw(interp.create_type_error("not a TypedArray"));
                        }
                    };

                    let source = args.first().cloned().unwrap_or(JsValue::Undefined);

                    // Step 2: ToIntegerOrInfinity(offset) BEFORE detach check
                    let offset_f = if args.len() > 1 {
                        match interp.to_number_value(&args[1]) {
                            Ok(n) => to_integer(n),
                            Err(e) => return Completion::Throw(e),
                        }
                    } else {
                        0.0
                    };
                    if offset_f < 0.0 || offset_f.is_infinite() {
                        return Completion::Throw(
                            interp.create_range_error("offset is out of bounds"),
                        );
                    }
                    let offset = offset_f as usize;

                    // Step 3: Check detach after offset coercion
                    if ta.is_detached.get() || is_typed_array_out_of_bounds(&ta) {
                        return Completion::Throw(
                            interp.create_type_error("typed array is detached"),
                        );
                    }

                    // Check if source is a TypedArray
                    if let JsValue::Object(src_o) = &source
                        && let Some(src_obj) = interp.get_object(src_o.id)
                    {
                        let is_ta = src_obj.borrow().typed_array_info.is_some();
                        if is_ta {
                            // TypedArray-arg path
                            let src_ta =
                                src_obj.borrow().typed_array_info.as_ref().unwrap().clone();
                            if src_ta.is_detached.get() || is_typed_array_out_of_bounds(&src_ta) {
                                return Completion::Throw(
                                    interp.create_type_error("source typed array is detached"),
                                );
                            }
                            if is_bigint_kind(ta.kind) != is_bigint_kind(src_ta.kind) {
                                return Completion::Throw(interp.create_type_error(
                                    "cannot mix BigInt and non-BigInt typed arrays",
                                ));
                            }
                            let src_len = typed_array_length(&src_ta);
                            if offset + src_len > typed_array_length(&ta) {
                                return Completion::Throw(
                                    interp.create_range_error("offset is out of bounds"),
                                );
                            }
                            // Same-type: byte copy. Different-type: element-by-element.
                            let same_buffer = Rc::ptr_eq(&ta.buffer, &src_ta.buffer);
                            if ta.kind == src_ta.kind {
                                let bpe = ta.kind.bytes_per_element();
                                let src_start = src_ta.byte_offset;
                                let dst_start = ta.byte_offset + offset * bpe;
                                let byte_count = src_len * bpe;
                                if same_buffer {
                                    // Clone source bytes first to handle overlap
                                    let src_bytes: Vec<u8> = {
                                        let buf = ta.buffer.borrow();
                                        buf[src_start..src_start + byte_count].to_vec()
                                    };
                                    let mut buf = ta.buffer.borrow_mut();
                                    buf[dst_start..dst_start + byte_count]
                                        .copy_from_slice(&src_bytes);
                                } else {
                                    let src_buf = src_ta.buffer.borrow();
                                    let mut dst_buf = ta.buffer.borrow_mut();
                                    dst_buf[dst_start..dst_start + byte_count].copy_from_slice(
                                        &src_buf[src_start..src_start + byte_count],
                                    );
                                }
                            } else if same_buffer {
                                // Clone all source values first
                                let values: Vec<JsValue> = (0..src_len)
                                    .map(|i| typed_array_get_index(&src_ta, i))
                                    .collect();
                                for (i, val) in values.iter().enumerate() {
                                    typed_array_set_index(&ta, offset + i, val);
                                }
                            } else {
                                for i in 0..src_len {
                                    let val = typed_array_get_index(&src_ta, i);
                                    typed_array_set_index(&ta, offset + i, &val);
                                }
                            }
                            return Completion::Normal(JsValue::Undefined);
                        }
                    }

                    // Array-like source path
                    // Capture target length BEFORE source length getter (may resize)
                    let target_len = typed_array_length(&ta);
                    let src_obj = match interp.to_object(&source) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    if let JsValue::Object(src_o) = &src_obj {
                        let len_val = match interp.get_object_property(src_o.id, "length", &src_obj)
                        {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let src_len = match interp.to_number_value(&len_val) {
                            Ok(n) => to_integer(n) as usize,
                            Err(e) => return Completion::Throw(e),
                        };
                        if offset + src_len > target_len {
                            return Completion::Throw(
                                interp.create_range_error("offset is out of bounds"),
                            );
                        }
                        for i in 0..src_len {
                            let val = match interp.get_object_property(
                                src_o.id,
                                &i.to_string(),
                                &src_obj,
                            ) {
                                Completion::Normal(v) => v,
                                other => return other,
                            };
                            // Coerce with proper ContentType
                            let coerced = match interp.typed_array_coerce_value(ta.kind, &val) {
                                Ok(v) => v,
                                Err(e) => return Completion::Throw(e),
                            };
                            // Use is_valid_integer_index to silently skip OOB writes (handles shrink mid-iteration)
                            if is_valid_integer_index(&ta, (offset + i) as f64) {
                                typed_array_set_index(&ta, offset + i, &coerced);
                            }
                        }
                    }
                    return Completion::Normal(JsValue::Undefined);
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto.borrow_mut().insert_builtin("set".to_string(), set_fn);

        // subarray
        let subarray_fn = self.create_function(JsFunction::native(
            "subarray".to_string(),
            2,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let (ta, buf_val) = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info {
                            let bv = obj_ref
                                .view_buffer_object_id
                                .map(|id| JsValue::Object(JsObject { id }))
                                .unwrap_or(JsValue::Undefined);
                            (ta.clone(), bv)
                        } else {
                            return Completion::Throw(interp.create_type_error("not a TypedArray"));
                        }
                    };
                    // Per spec: if OOB, srcArrayLength = 0 (no throw for subarray)
                    let src_len = if ta.is_detached.get() || is_typed_array_out_of_bounds(&ta) {
                        0i64
                    } else {
                        typed_array_length(&ta) as i64
                    };
                    let resolve_idx = |v: f64, len: i64| -> usize {
                        let vi = if v == f64::NEG_INFINITY || v <= -(len as f64) - 1.0 {
                            i64::MIN / 2
                        } else if v == f64::INFINITY || v > len as f64 {
                            i64::MAX / 2
                        } else {
                            v as i64
                        };
                        (if vi < 0 { (len + vi).max(0) } else { vi.min(len) }) as usize
                    };
                    let begin = {
                        let n = to_integer(if let Some(a) = args.first() {
                            match interp.to_number_value(a) {
                                Ok(n) => n,
                                Err(e) => return Completion::Throw(e),
                            }
                        } else {
                            0.0
                        });
                        resolve_idx(n, src_len)
                    };
                    let end_is_undefined = args.len() <= 1 || matches!(args.get(1), Some(JsValue::Undefined));
                    let end = {
                        let n = if !end_is_undefined {
                            to_integer(match interp.to_number_value(&args[1]) {
                                Ok(n) => n,
                                Err(e) => return Completion::Throw(e),
                            })
                        } else {
                            src_len as f64
                        };
                        resolve_idx(n, src_len)
                    };
                    let new_len = end.saturating_sub(begin);
                    let bpe = ta.kind.bytes_per_element();
                    let new_offset = ta.byte_offset + begin * bpe;

                    let length_arg = if end_is_undefined && ta.is_length_tracking {
                        JsValue::Undefined
                    } else {
                        JsValue::Number(new_len as f64)
                    };
                    let ctor_args = [buf_val, JsValue::Number(new_offset as f64), length_arg];
                    return match interp.typed_array_species_create(this_val, &ctor_args) {
                        Ok(v) => Completion::Normal(v),
                        Err(e) => Completion::Throw(e),
                    };
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("subarray".to_string(), subarray_fn);

        // slice
        let slice_fn = self.create_function(JsFunction::native(
            "slice".to_string(),
            2,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info {
                            if ta.is_detached.get() || is_typed_array_out_of_bounds(ta) {
                                return Completion::Throw(
                                    interp.create_type_error("typed array is detached"),
                                );
                            }
                            ta.clone()
                        } else {
                            return Completion::Throw(interp.create_type_error("not a TypedArray"));
                        }
                    };
                    let len = typed_array_length(&ta) as i64;
                    let resolve_idx = |v: f64, len: i64| -> usize {
                        let vi = if v == f64::NEG_INFINITY || v <= -(len as f64) - 1.0 {
                            i64::MIN / 2
                        } else if v == f64::INFINITY || v > len as f64 {
                            i64::MAX / 2
                        } else {
                            v as i64
                        };
                        (if vi < 0 { (len + vi).max(0) } else { vi.min(len) }) as usize
                    };
                    let begin = {
                        let n = to_integer(if let Some(a) = args.first() {
                            match interp.to_number_value(a) {
                                Ok(n) => n,
                                Err(e) => return Completion::Throw(e),
                            }
                        } else {
                            0.0
                        });
                        resolve_idx(n, len)
                    };
                    let end = {
                        let n = if args.len() > 1 && !matches!(args[1], JsValue::Undefined) {
                            to_integer(match interp.to_number_value(&args[1]) {
                                Ok(n) => n,
                                Err(e) => return Completion::Throw(e),
                            })
                        } else {
                            len as f64
                        };
                        resolve_idx(n, len)
                    };
                    let count = end.saturating_sub(begin);

                    // Use TypedArraySpeciesCreate
                    let new_ta_val = match interp
                        .typed_array_species_create(this_val, &[JsValue::Number(count as f64)])
                    {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    };

                    if count > 0 {
                        // Re-check detach and OOB after species create (user code may resize)
                        if ta.is_detached.get() {
                            return Completion::Throw(
                                interp.create_type_error("typed array is detached"),
                            );
                        }
                        if is_typed_array_out_of_bounds(&ta) {
                            return Completion::Throw(
                                interp.create_type_error("typed array is out of bounds"),
                            );
                        }
                        let new_len = typed_array_length(&ta);
                        let end = end.min(new_len);
                        let begin = begin.min(new_len);
                        let count = end.saturating_sub(begin);

                        if count > 0 {
                            if let JsValue::Object(new_o) = &new_ta_val
                                && let Some(new_obj) = interp.get_object(new_o.id)
                            {
                                let new_ta = {
                                    let obj_ref = new_obj.borrow();
                                    obj_ref.typed_array_info.as_ref().unwrap().clone()
                                };
                                if new_ta.kind == ta.kind {
                                    let bpe = ta.kind.bytes_per_element();
                                    let src_start = ta.byte_offset + begin * bpe;
                                    let dst_start = new_ta.byte_offset;
                                    let byte_count = count * bpe;
                                    let same_buf = Rc::ptr_eq(&ta.buffer, &new_ta.buffer);
                                    if same_buf {
                                        let mut buf = ta.buffer.borrow_mut();
                                        if src_start + byte_count <= buf.len()
                                            && dst_start + byte_count <= buf.len()
                                        {
                                            // Spec: copy byte-by-byte in forward order (overlapping-write semantics)
                                            for j in 0..byte_count {
                                                buf[dst_start + j] = buf[src_start + j];
                                            }
                                        }
                                    } else {
                                        let src_buf = ta.buffer.borrow();
                                        let mut dst_buf = new_ta.buffer.borrow_mut();
                                        if src_start + byte_count <= src_buf.len()
                                            && dst_start + byte_count <= dst_buf.len()
                                        {
                                            dst_buf[dst_start..dst_start + byte_count].copy_from_slice(
                                                &src_buf[src_start..src_start + byte_count],
                                            );
                                        }
                                    }
                                } else {
                                    for i in 0..count {
                                        let val = typed_array_get_index(&ta, begin + i);
                                        typed_array_set_index(&new_ta, i, &val);
                                    }
                                }
                            }
                        }
                    }
                    return Completion::Normal(new_ta_val);
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("slice".to_string(), slice_fn);

        // copyWithin
        let copy_within_fn = self.create_function(JsFunction::native(
            "copyWithin".to_string(),
            2,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info {
                            if ta.is_detached.get() || is_typed_array_out_of_bounds(ta) {
                                return Completion::Throw(
                                    interp.create_type_error("typed array is detached"),
                                );
                            }
                            ta.clone()
                        } else {
                            return Completion::Throw(interp.create_type_error("not a TypedArray"));
                        }
                    };
                    let len = typed_array_length(&ta) as i64;
                    let resolve_index = |v: f64, len: i64| -> usize {
                        let vi = if v == f64::NEG_INFINITY || v < -(len as f64) - 1.0 {
                            i64::MIN / 2
                        } else if v == f64::INFINITY || v > len as f64 {
                            i64::MAX / 2
                        } else {
                            v as i64
                        };
                        (if vi < 0 { (len + vi).max(0) } else { vi.min(len) }) as usize
                    };
                    let target = {
                        let n = match interp
                            .to_number_value(args.first().unwrap_or(&JsValue::Undefined))
                        {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        };
                        resolve_index(to_integer(n), len)
                    };
                    let start = {
                        let n = match interp
                            .to_number_value(args.get(1).unwrap_or(&JsValue::Undefined))
                        {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        };
                        resolve_index(to_integer(n), len)
                    };
                    let end = {
                        let n = if args.len() > 2 && !matches!(args[2], JsValue::Undefined) {
                            match interp.to_number_value(&args[2]) {
                                Ok(n) => to_integer(n),
                                Err(e) => return Completion::Throw(e),
                            }
                        } else {
                            len as f64
                        };
                        resolve_index(n, len)
                    };
                    // Re-check detach and OOB after coercion
                    if ta.is_detached.get() || is_typed_array_out_of_bounds(&ta) {
                        return Completion::Throw(
                            interp.create_type_error("typed array is detached"),
                        );
                    }
                    // Use original len for count (per spec), but bound copy by actual buffer
                    let cur_len = typed_array_length(&ta) as usize;
                    let count = if end <= start || target >= len as usize {
                        0
                    } else {
                        (end - start).min(len as usize - target)
                    };
                    if count > 0 {
                        let bpe = ta.kind.bytes_per_element();
                        let mut buf = ta.buffer.borrow_mut();
                        let buf_len = buf.len();
                        let src_byte_start = ta.byte_offset + start * bpe;
                        let dst_byte_start = ta.byte_offset + target * bpe;
                        let max_src_bytes = buf_len.saturating_sub(src_byte_start);
                        let max_dst_bytes = buf_len.saturating_sub(dst_byte_start);
                        let byte_count = (count * bpe).min(max_src_bytes).min(max_dst_bytes);
                        if byte_count > 0 {
                            let src: Vec<u8> =
                                buf[src_byte_start..src_byte_start + byte_count].to_vec();
                            buf[dst_byte_start..dst_byte_start + byte_count]
                                .copy_from_slice(&src);
                        }
                    }
                    return Completion::Normal(this_val.clone());
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("copyWithin".to_string(), copy_within_fn);

        // fill
        let fill_fn = self.create_function(JsFunction::native(
            "fill".to_string(),
            1,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info {
                            if ta.is_detached.get() || is_typed_array_out_of_bounds(ta) {
                                return Completion::Throw(
                                    interp.create_type_error("typed array is detached"),
                                );
                            }
                            ta.clone()
                        } else {
                            return Completion::Throw(interp.create_type_error("not a TypedArray"));
                        }
                    };
                    // Step 3: compute len BEFORE coercion
                    let len = typed_array_length(&ta);
                    // Per spec: coerce value BEFORE start/end
                    let raw_value = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let coerced = match interp.typed_array_coerce_value(ta.kind, &raw_value) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    };
                    let len_f = len as f64;
                    let start = {
                        let v = to_integer(if args.len() > 1 {
                            match interp.to_number_value(&args[1]) {
                                Ok(n) => n,
                                Err(e) => return Completion::Throw(e),
                            }
                        } else {
                            0.0
                        });
                        if v < 0.0 {
                            ((len_f + v) as isize).max(0) as usize
                        } else {
                            (v as usize).min(len)
                        }
                    };
                    let end = {
                        let v = if args.len() > 2 && !matches!(args[2], JsValue::Undefined) {
                            to_integer(match interp.to_number_value(&args[2]) {
                                Ok(n) => n,
                                Err(e) => return Completion::Throw(e),
                            })
                        } else {
                            len_f
                        };
                        if v < 0.0 {
                            ((len_f + v) as isize).max(0) as usize
                        } else {
                            (v as usize).min(len)
                        }
                    };
                    // Re-check detach and OOB after coercion (buffer may have been resized)
                    if ta.is_detached.get() {
                        return Completion::Throw(
                            interp.create_type_error("typed array is detached"),
                        );
                    }
                    if is_typed_array_out_of_bounds(&ta) {
                        return Completion::Throw(
                            interp.create_type_error("typed array is out of bounds"),
                        );
                    }
                    let new_len = typed_array_length(&ta);
                    let end = end.min(new_len);
                    for i in start..end {
                        typed_array_set_index(&ta, i, &coerced);
                    }
                    return Completion::Normal(this_val.clone());
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("fill".to_string(), fill_fn);

        // indexOf
        let index_of_fn = self.create_function(JsFunction::native(
            "indexOf".to_string(),
            1,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info {
                            if ta.is_detached.get() || is_typed_array_out_of_bounds(ta) {
                                return Completion::Throw(
                                    interp.create_type_error("typed array is detached"),
                                );
                            }
                            ta.clone()
                        } else {
                            return Completion::Throw(interp.create_type_error("not a TypedArray"));
                        }
                    };
                    let len = typed_array_length(&ta) as i64;
                    if len == 0 {
                        return Completion::Normal(JsValue::Number(-1.0));
                    }
                    let search = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let from = if args.len() > 1 {
                        to_integer(match interp.to_number_value(&args[1]) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        }) as i64
                    } else {
                        0
                    };
                    let start = if from < 0 {
                        (len + from).max(0) as usize
                    } else {
                        from as usize
                    };
                    for i in start..len as usize {
                        // indexOf uses HasProperty semantics: skip elements that are not valid
                        if !is_valid_integer_index(&ta, i as f64) {
                            continue;
                        }
                        let elem = typed_array_get_index(&ta, i);
                        if strict_eq(&elem, &search) {
                            return Completion::Normal(JsValue::Number(i as f64));
                        }
                    }
                    return Completion::Normal(JsValue::Number(-1.0));
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("indexOf".to_string(), index_of_fn);

        // lastIndexOf
        let last_index_of_fn = self.create_function(JsFunction::native(
            "lastIndexOf".to_string(),
            1,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info {
                            if ta.is_detached.get() || is_typed_array_out_of_bounds(ta) {
                                return Completion::Throw(
                                    interp.create_type_error("typed array is detached"),
                                );
                            }
                            ta.clone()
                        } else {
                            return Completion::Throw(interp.create_type_error("not a TypedArray"));
                        }
                    };
                    let len = typed_array_length(&ta) as i64;
                    if len == 0 {
                        return Completion::Normal(JsValue::Number(-1.0));
                    }
                    let search = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let from = if args.len() > 1 {
                        to_integer(match interp.to_number_value(&args[1]) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        }) as i64
                    } else {
                        len - 1
                    };
                    let start = if from < 0 {
                        (len + from).max(-1)
                    } else {
                        from.min(len - 1)
                    };
                    let mut i = start;
                    while i >= 0 {
                        // lastIndexOf uses HasProperty semantics: skip elements that are not valid
                        if is_valid_integer_index(&ta, i as f64) {
                            let elem = typed_array_get_index(&ta, i as usize);
                            if strict_eq(&elem, &search) {
                                return Completion::Normal(JsValue::Number(i as f64));
                            }
                        }
                        i -= 1;
                    }
                    return Completion::Normal(JsValue::Number(-1.0));
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("lastIndexOf".to_string(), last_index_of_fn);

        // includes
        let includes_fn = self.create_function(JsFunction::native(
            "includes".to_string(),
            1,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info {
                            if ta.is_detached.get() || is_typed_array_out_of_bounds(ta) {
                                return Completion::Throw(
                                    interp.create_type_error("typed array is detached"),
                                );
                            }
                            ta.clone()
                        } else {
                            return Completion::Throw(interp.create_type_error("not a TypedArray"));
                        }
                    };
                    let len = typed_array_length(&ta) as i64;
                    if len == 0 {
                        return Completion::Normal(JsValue::Boolean(false));
                    }
                    let search = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let from = if args.len() > 1 {
                        to_integer(match interp.to_number_value(&args[1]) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        }) as i64
                    } else {
                        0
                    };
                    let start = if from < 0 {
                        (len + from).max(0) as usize
                    } else {
                        from as usize
                    };
                    for i in start..len as usize {
                        let elem = typed_array_get_index(&ta, i);
                        if same_value_zero(&elem, &search) {
                            return Completion::Normal(JsValue::Boolean(true));
                        }
                    }
                    return Completion::Normal(JsValue::Boolean(false));
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("includes".to_string(), includes_fn);

        // Higher-order methods: find, findIndex, findLast, findLastIndex, forEach, map, filter,
        // every, some, reduce, reduceRight
        self.setup_ta_higher_order_methods(&proto);

        // reverse
        let reverse_fn = self.create_function(JsFunction::native(
            "reverse".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info {
                            if ta.is_detached.get() || is_typed_array_out_of_bounds(ta) {
                                return Completion::Throw(
                                    interp.create_type_error("typed array is detached"),
                                );
                            }
                            ta.clone()
                        } else {
                            return Completion::Throw(interp.create_type_error("not a TypedArray"));
                        }
                    };
                    let mut lo = 0usize;
                    let mut hi = typed_array_length(&ta);
                    while lo < hi {
                        hi -= 1;
                        let a = typed_array_get_index(&ta, lo);
                        let b = typed_array_get_index(&ta, hi);
                        typed_array_set_index(&ta, lo, &b);
                        typed_array_set_index(&ta, hi, &a);
                        lo += 1;
                    }
                    return Completion::Normal(this_val.clone());
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("reverse".to_string(), reverse_fn);

        // sort
        let sort_fn = self.create_function(JsFunction::native(
            "sort".to_string(),
            1,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info {
                            if ta.is_detached.get() || is_typed_array_out_of_bounds(ta) {
                                return Completion::Throw(
                                    interp.create_type_error("typed array is detached"),
                                );
                            }
                            ta.clone()
                        } else {
                            return Completion::Throw(interp.create_type_error("not a TypedArray"));
                        }
                    };
                    let comparefn = args.first().cloned();
                    if let Some(ref cmp) = comparefn
                        && !matches!(cmp, JsValue::Undefined)
                    {
                        let is_callable = if let JsValue::Object(co) = cmp {
                            interp
                                .get_object(co.id)
                                .is_some_and(|obj| obj.borrow().callable.is_some())
                        } else {
                            false
                        };
                        if !is_callable {
                            return Completion::Throw(
                                interp.create_type_error("comparefn is not a function"),
                            );
                        }
                    }
                    let mut elems: Vec<JsValue> = (0..typed_array_length(&ta))
                        .map(|i| typed_array_get_index(&ta, i))
                        .collect();
                    // Sort with comparison
                    let mut error: Option<JsValue> = None;
                    elems.sort_by(|a, b| {
                        if error.is_some() {
                            return std::cmp::Ordering::Equal;
                        }
                        if let Some(ref cmp) = comparefn
                            && !matches!(cmp, JsValue::Undefined)
                        {
                            match interp.call_function(
                                cmp,
                                &JsValue::Undefined,
                                &[a.clone(), b.clone()],
                            ) {
                                Completion::Normal(v) => match interp.to_number_value(&v) {
                                    Ok(n) => {
                                        if n.is_nan() {
                                            return std::cmp::Ordering::Equal;
                                        }
                                        if n < 0.0 {
                                            return std::cmp::Ordering::Less;
                                        }
                                        if n > 0.0 {
                                            return std::cmp::Ordering::Greater;
                                        }
                                        return std::cmp::Ordering::Equal;
                                    }
                                    Err(e) => {
                                        error = Some(e);
                                        return std::cmp::Ordering::Equal;
                                    }
                                },
                                Completion::Throw(e) => {
                                    error = Some(e);
                                    return std::cmp::Ordering::Equal;
                                }
                                _ => return std::cmp::Ordering::Equal,
                            }
                        }
                        // Default sort: numeric for Number types, BigInt comparison for BigInt types
                        match (a, b) {
                            (JsValue::BigInt(ba), JsValue::BigInt(bb)) => ba.value.cmp(&bb.value),
                            _ => {
                                let na = to_number(a);
                                let nb = to_number(b);
                                // Per spec: -0 < +0 is false, +0 < -0 is false
                                if na.is_nan() && nb.is_nan() {
                                    std::cmp::Ordering::Equal
                                } else if na.is_nan() {
                                    std::cmp::Ordering::Greater
                                } else if nb.is_nan() {
                                    std::cmp::Ordering::Less
                                } else if na == 0.0 && nb == 0.0 {
                                    // -0 comes before +0
                                    na.is_sign_negative().cmp(&nb.is_sign_negative()).reverse()
                                } else {
                                    na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal)
                                }
                            }
                        }
                    });
                    if let Some(e) = error {
                        return Completion::Throw(e);
                    }
                    for (i, val) in elems.iter().enumerate() {
                        if !is_valid_integer_index(&ta, i as f64) {
                            break;
                        }
                        typed_array_set_index(&ta, i, val);
                    }
                    return Completion::Normal(this_val.clone());
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("sort".to_string(), sort_fn);

        // join
        let join_fn = self.create_function(JsFunction::native(
            "join".to_string(),
            1,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info {
                            if ta.is_detached.get() || is_typed_array_out_of_bounds(ta) {
                                return Completion::Throw(
                                    interp.create_type_error("typed array is detached"),
                                );
                            }
                            ta.clone()
                        } else {
                            return Completion::Throw(interp.create_type_error("not a TypedArray"));
                        }
                    };
                    let len = typed_array_length(&ta);
                    let sep = if args.is_empty() || matches!(args[0], JsValue::Undefined) {
                        ",".to_string()
                    } else {
                        match interp.to_string_value(&args[0]) {
                            Ok(s) => s,
                            Err(e) => return Completion::Throw(e),
                        }
                    };
                    let mut parts: Vec<String> = Vec::with_capacity(len);
                    for i in 0..len {
                        let elem = typed_array_get_index(&ta, i);
                        let s = if matches!(elem, JsValue::Undefined | JsValue::Null) {
                            String::new()
                        } else {
                            match interp.to_string_value(&elem) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            }
                        };
                        parts.push(s);
                    }
                    Completion::Normal(JsValue::String(JsString::from_str(&parts.join(&sep))))
                } else {
                    Completion::Throw(interp.create_type_error("not a TypedArray"))
                }
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("join".to_string(), join_fn);

        // toString must be the same function object as Array.prototype.toString (spec §23.2.3.30)
        {
            let array_proto = self.realm().array_prototype.clone().unwrap();
            let tostring_val = array_proto.borrow().get_property("toString");
            proto.borrow_mut().insert_builtin("toString".to_string(), tostring_val);
        }

        // toLocaleString
        let to_locale_string_fn = self.create_function(JsFunction::native(
            "toLocaleString".to_string(),
            0,
            |interp, this_val, args| {
                // ValidateTypedArray: Type(O) must be Object
                if !matches!(this_val, JsValue::Object(_)) {
                    return Completion::Throw(interp.create_type_error("not a TypedArray"));
                }
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info {
                            if ta.is_detached.get() || is_typed_array_out_of_bounds(ta) {
                                return Completion::Throw(
                                    interp.create_type_error("typed array is detached"),
                                );
                            }
                            ta.clone()
                        } else {
                            // ValidateTypedArray: Must have [[TypedArrayName]] slot
                            return Completion::Throw(interp.create_type_error("not a TypedArray"));
                        }
                    };
                    let separator = ",";
                    let locales = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    let pass_args = vec![locales, options];
                    let len = typed_array_length(&ta);
                    let mut parts: Vec<String> = Vec::with_capacity(len);
                    for k in 0..len {
                        let next_element = typed_array_get_index(&ta, k);
                        if matches!(next_element, JsValue::Undefined | JsValue::Null) {
                            parts.push(String::new());
                        } else {
                            // Convert to object to get toLocaleString method
                            let element_obj = match interp.to_object(&next_element) {
                                Completion::Normal(v) => v,
                                other => return other,
                            };
                            if let JsValue::Object(ref elem_ref) = element_obj {
                                let to_locale_str_method = match interp.get_object_property(
                                    elem_ref.id,
                                    "toLocaleString",
                                    &element_obj,
                                ) {
                                    Completion::Normal(v) => v,
                                    other => return other,
                                };
                                if interp.is_callable(&to_locale_str_method) {
                                    match interp.call_function(
                                        &to_locale_str_method,
                                        &next_element,
                                        &pass_args,
                                    ) {
                                        Completion::Normal(v) => {
                                            let s = match interp.to_string_value(&v) {
                                                Ok(s) => s,
                                                Err(e) => return Completion::Throw(e),
                                            };
                                            parts.push(s);
                                        }
                                        other => return other,
                                    }
                                } else {
                                    let err = interp
                                        .create_type_error("toLocaleString is not a function");
                                    return Completion::Throw(err);
                                }
                            }
                        }
                    }
                    Completion::Normal(JsValue::String(JsString::from_str(&parts.join(separator))))
                } else {
                    Completion::Throw(interp.create_type_error("not a TypedArray"))
                }
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toLocaleString".to_string(), to_locale_string_fn);

        // toReversed
        let to_reversed_fn = self.create_function(JsFunction::native(
            "toReversed".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info {
                            if ta.is_detached.get() || is_typed_array_out_of_bounds(ta) {
                                return Completion::Throw(
                                    interp.create_type_error("typed array is detached"),
                                );
                            }
                            ta.clone()
                        } else {
                            return Completion::Throw(interp.create_type_error("not a TypedArray"));
                        }
                    };
                    let len = typed_array_length(&ta);
                    let bpe = ta.kind.bytes_per_element();
                    let new_buf = vec![0u8; len * bpe];
                    let new_buf_rc = Rc::new(RefCell::new(new_buf));
                    let new_detached = Rc::new(Cell::new(false));
                    let new_ta = TypedArrayInfo {
                        kind: ta.kind,
                        buffer: new_buf_rc.clone(),
                        byte_offset: 0,
                        byte_length: len * bpe,
                        array_length: len,
                        is_detached: new_detached.clone(),
                        is_length_tracking: false,
                    };
                    for i in 0..len {
                        let val = typed_array_get_index(&ta, len - 1 - i);
                        typed_array_set_index(&new_ta, i, &val);
                    }
                    let ab_obj = interp.create_object();
                    {
                        let mut ab = ab_obj.borrow_mut();
                        ab.class_name = "ArrayBuffer".to_string();
                        ab.prototype = interp.realm().arraybuffer_prototype.clone();
                        ab.arraybuffer_data = Some(new_buf_rc);
                        ab.arraybuffer_detached = Some(new_detached);
                    }
                    let ab_id = ab_obj.borrow().id.unwrap();
                    let buf_val = JsValue::Object(JsObject { id: ab_id });
                    let result = interp.create_typed_array_object(new_ta, buf_val);
                    let id = result.borrow().id.unwrap();
                    return Completion::Normal(JsValue::Object(JsObject { id }));
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toReversed".to_string(), to_reversed_fn);

        // toSorted
        let to_sorted_fn = self.create_function(JsFunction::native(
            "toSorted".to_string(),
            1,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info {
                            if ta.is_detached.get() || is_typed_array_out_of_bounds(ta) {
                                return Completion::Throw(
                                    interp.create_type_error("typed array is detached"),
                                );
                            }
                            ta.clone()
                        } else {
                            return Completion::Throw(interp.create_type_error("not a TypedArray"));
                        }
                    };
                    let comparefn = args.first().cloned();
                    if let Some(ref cmp) = comparefn
                        && !matches!(cmp, JsValue::Undefined)
                    {
                        let is_callable = if let JsValue::Object(co) = cmp {
                            interp
                                .get_object(co.id)
                                .is_some_and(|obj| obj.borrow().callable.is_some())
                        } else {
                            false
                        };
                        if !is_callable {
                            return Completion::Throw(
                                interp.create_type_error("comparefn is not a function"),
                            );
                        }
                    }
                    let mut elems: Vec<JsValue> = (0..typed_array_length(&ta))
                        .map(|i| typed_array_get_index(&ta, i))
                        .collect();
                    let mut error: Option<JsValue> = None;
                    elems.sort_by(|a, b| {
                        if error.is_some() {
                            return std::cmp::Ordering::Equal;
                        }
                        if let Some(ref cmp) = comparefn
                            && !matches!(cmp, JsValue::Undefined)
                        {
                            match interp.call_function(
                                cmp,
                                &JsValue::Undefined,
                                &[a.clone(), b.clone()],
                            ) {
                                Completion::Normal(v) => match interp.to_number_value(&v) {
                                    Ok(n) => {
                                        if n.is_nan() {
                                            return std::cmp::Ordering::Equal;
                                        }
                                        if n < 0.0 {
                                            return std::cmp::Ordering::Less;
                                        }
                                        if n > 0.0 {
                                            return std::cmp::Ordering::Greater;
                                        }
                                        return std::cmp::Ordering::Equal;
                                    }
                                    Err(e) => {
                                        error = Some(e);
                                        return std::cmp::Ordering::Equal;
                                    }
                                },
                                Completion::Throw(e) => {
                                    error = Some(e);
                                    return std::cmp::Ordering::Equal;
                                }
                                _ => return std::cmp::Ordering::Equal,
                            }
                        }
                        // Default sort: numeric for Number types, BigInt comparison for BigInt types
                        match (a, b) {
                            (JsValue::BigInt(ba), JsValue::BigInt(bb)) => ba.value.cmp(&bb.value),
                            _ => {
                                let na = to_number(a);
                                let nb = to_number(b);
                                if na.is_nan() && nb.is_nan() {
                                    std::cmp::Ordering::Equal
                                } else if na.is_nan() {
                                    std::cmp::Ordering::Greater
                                } else if nb.is_nan() {
                                    std::cmp::Ordering::Less
                                } else if na == 0.0 && nb == 0.0 {
                                    na.is_sign_negative().cmp(&nb.is_sign_negative()).reverse()
                                } else {
                                    na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal)
                                }
                            }
                        }
                    });
                    if let Some(e) = error {
                        return Completion::Throw(e);
                    }
                    let len = typed_array_length(&ta);
                    let bpe = ta.kind.bytes_per_element();
                    let new_buf = vec![0u8; len * bpe];
                    let new_buf_rc = Rc::new(RefCell::new(new_buf));
                    let new_detached = Rc::new(Cell::new(false));
                    let new_ta = TypedArrayInfo {
                        kind: ta.kind,
                        buffer: new_buf_rc.clone(),
                        byte_offset: 0,
                        byte_length: len * bpe,
                        array_length: len,
                        is_detached: new_detached.clone(),
                        is_length_tracking: false,
                    };
                    for (i, val) in elems.iter().enumerate() {
                        typed_array_set_index(&new_ta, i, val);
                    }
                    let ab_obj = interp.create_object();
                    {
                        let mut ab = ab_obj.borrow_mut();
                        ab.class_name = "ArrayBuffer".to_string();
                        ab.prototype = interp.realm().arraybuffer_prototype.clone();
                        ab.arraybuffer_data = Some(new_buf_rc);
                        ab.arraybuffer_detached = Some(new_detached);
                    }
                    let ab_id = ab_obj.borrow().id.unwrap();
                    let buf_val = JsValue::Object(JsObject { id: ab_id });
                    let result = interp.create_typed_array_object(new_ta, buf_val);
                    let id = result.borrow().id.unwrap();
                    return Completion::Normal(JsValue::Object(JsObject { id }));
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("toSorted".to_string(), to_sorted_fn);

        // with
        let with_fn = self.create_function(JsFunction::native(
            "with".to_string(),
            2,
            |interp, this_val, args| {
                // Helper: ToNumber that properly calls valueOf and propagates errors
                fn to_number_throwing(
                    interp: &mut Interpreter,
                    val: &JsValue,
                ) -> Result<f64, JsValue> {
                    match val {
                        JsValue::Object(o) => {
                            if let Some(obj) = interp.get_object(o.id) {
                                let method = {
                                    let borrow = obj.borrow();
                                    borrow
                                        .get_property_descriptor("valueOf")
                                        .and_then(|d| d.value)
                                };
                                if let Some(func) = method
                                    && interp.is_callable(&func)
                                {
                                    match interp.call_function(&func, val, &[]) {
                                        Completion::Normal(v)
                                            if !matches!(v, JsValue::Object(_)) =>
                                        {
                                            return to_number_throwing(interp, &v);
                                        }
                                        Completion::Normal(_) => {}
                                        Completion::Throw(e) => return Err(e),
                                        _ => {}
                                    }
                                }
                                let tostring_method = {
                                    let borrow = obj.borrow();
                                    borrow
                                        .get_property_descriptor("toString")
                                        .and_then(|d| d.value)
                                };
                                if let Some(func) = tostring_method
                                    && interp.is_callable(&func)
                                {
                                    match interp.call_function(&func, val, &[]) {
                                        Completion::Normal(v)
                                            if !matches!(v, JsValue::Object(_)) =>
                                        {
                                            return to_number_throwing(interp, &v);
                                        }
                                        Completion::Normal(_) => {}
                                        Completion::Throw(e) => return Err(e),
                                        _ => {}
                                    }
                                }
                            }
                            Ok(f64::NAN)
                        }
                        JsValue::Symbol(_) => {
                            Err(interp
                                .create_type_error("Cannot convert a Symbol value to a number"))
                        }
                        JsValue::BigInt(_) => {
                            Err(interp
                                .create_type_error("Cannot convert a BigInt value to a number"))
                        }
                        _ => Ok(to_number(val)),
                    }
                }

                // Helper: ToBigInt that properly calls valueOf and propagates errors
                fn to_bigint_throwing(
                    interp: &mut Interpreter,
                    val: &JsValue,
                ) -> Result<JsValue, JsValue> {
                    match val {
                        JsValue::BigInt(_) => Ok(val.clone()),
                        JsValue::Object(o) => {
                            if let Some(obj) = interp.get_object(o.id) {
                                let method = {
                                    let borrow = obj.borrow();
                                    borrow
                                        .get_property_descriptor("valueOf")
                                        .and_then(|d| d.value)
                                };
                                if let Some(func) = method
                                    && interp.is_callable(&func)
                                {
                                    match interp.call_function(&func, val, &[]) {
                                        Completion::Normal(v)
                                            if !matches!(v, JsValue::Object(_)) =>
                                        {
                                            return to_bigint_throwing(interp, &v);
                                        }
                                        Completion::Normal(_) => {}
                                        Completion::Throw(e) => return Err(e),
                                        _ => {}
                                    }
                                }
                            }
                            Err(interp.create_type_error("Cannot convert value to a BigInt"))
                        }
                        JsValue::Boolean(b) => Ok(JsValue::BigInt(JsBigInt {
                            value: num_bigint::BigInt::from(if *b { 1 } else { 0 }),
                        })),
                        JsValue::Number(n) => {
                            // ToBigInt throws TypeError for Number values
                            Err(interp
                                .create_type_error(&format!("Cannot convert {} to a BigInt", n)))
                        }
                        JsValue::String(s) => {
                            let text = s.to_rust_string().trim().to_string();
                            if text.is_empty() {
                                return Err(interp.create_error(
                                    "SyntaxError",
                                    "Cannot convert empty string to a BigInt",
                                ));
                            }
                            let parsed = if let Some(hex) =
                                text.strip_prefix("0x").or_else(|| text.strip_prefix("0X"))
                            {
                                num_bigint::BigInt::parse_bytes(hex.as_bytes(), 16)
                            } else if let Some(oct) =
                                text.strip_prefix("0o").or_else(|| text.strip_prefix("0O"))
                            {
                                num_bigint::BigInt::parse_bytes(oct.as_bytes(), 8)
                            } else if let Some(bin) =
                                text.strip_prefix("0b").or_else(|| text.strip_prefix("0B"))
                            {
                                num_bigint::BigInt::parse_bytes(bin.as_bytes(), 2)
                            } else {
                                text.parse::<num_bigint::BigInt>().ok()
                            };
                            match parsed {
                                Some(v) => Ok(JsValue::BigInt(JsBigInt { value: v })),
                                None => Err(interp.create_error(
                                    "SyntaxError",
                                    &format!("Cannot convert {} to a BigInt", text),
                                )),
                            }
                        }
                        _ => Err(interp.create_type_error("Cannot convert value to a BigInt")),
                    }
                }

                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info {
                            if ta.is_detached.get() || is_typed_array_out_of_bounds(ta) {
                                return Completion::Throw(
                                    interp.create_type_error("typed array is detached"),
                                );
                            }
                            ta.clone()
                        } else {
                            return Completion::Throw(interp.create_type_error("not a TypedArray"));
                        }
                    };
                    let len = typed_array_length(&ta) as i64;

                    // Step 4: ToIntegerOrInfinity(index) - must call valueOf on objects
                    let index_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let relative_index = match to_number_throwing(interp, &index_arg) {
                        Ok(n) => to_integer_or_infinity(n) as i64,
                        Err(e) => return Completion::Throw(e),
                    };
                    let actual_index = if relative_index >= 0 {
                        relative_index
                    } else {
                        len + relative_index
                    };

                    // Steps 7-8: Coerce value BEFORE checking index bounds
                    let value_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    let numeric_value = if ta.kind.is_bigint() {
                        match to_bigint_throwing(interp, &value_arg) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        }
                    } else {
                        match to_number_throwing(interp, &value_arg) {
                            Ok(n) => JsValue::Number(n),
                            Err(e) => return Completion::Throw(e),
                        }
                    };

                    // Step 9: Check IsValidIntegerIndex after coercions (uses current TA state)
                    if !is_valid_integer_index(&ta, actual_index as f64) {
                        return Completion::Throw(
                            interp
                                .create_range_error("Invalid index for TypedArray.prototype.with"),
                        );
                    }

                    let bpe = ta.kind.bytes_per_element();
                    let new_buf = vec![0u8; len as usize * bpe];
                    let new_buf_rc = Rc::new(RefCell::new(new_buf));
                    let new_detached = Rc::new(Cell::new(false));
                    let new_ta = TypedArrayInfo {
                        kind: ta.kind,
                        buffer: new_buf_rc.clone(),
                        byte_offset: 0,
                        byte_length: len as usize * bpe,
                        array_length: len as usize,
                        is_detached: new_detached.clone(),
                        is_length_tracking: false,
                    };
                    for k in 0..len as usize {
                        let elem = if k == actual_index as usize {
                            numeric_value.clone()
                        } else {
                            typed_array_get_index(&ta, k)
                        };
                        typed_array_set_index(&new_ta, k, &elem);
                    }
                    let ab_obj = interp.create_object();
                    {
                        let mut ab = ab_obj.borrow_mut();
                        ab.class_name = "ArrayBuffer".to_string();
                        ab.prototype = interp.realm().arraybuffer_prototype.clone();
                        ab.arraybuffer_data = Some(new_buf_rc);
                        ab.arraybuffer_detached = Some(new_detached);
                    }
                    let ab_id = ab_obj.borrow().id.unwrap();
                    let buf_val = JsValue::Object(JsObject { id: ab_id });
                    let result = interp.create_typed_array_object(new_ta, buf_val);
                    let id = result.borrow().id.unwrap();
                    return Completion::Normal(JsValue::Object(JsObject { id }));
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("with".to_string(), with_fn);

        // @@toStringTag getter
        let to_string_tag_getter = self.create_function(JsFunction::native(
            "get [Symbol.toStringTag]".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if let Some(ref ta) = obj_ref.typed_array_info {
                        return Completion::Normal(JsValue::String(JsString::from_str(
                            ta.kind.name(),
                        )));
                    }
                }
                Completion::Normal(JsValue::Undefined)
            },
        ));
        proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(to_string_tag_getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );
    }

    fn setup_ta_values_method(&mut self, proto: &Rc<RefCell<JsObjectData>>) {
        let values_fn = self.create_function(JsFunction::native(
            "values".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info {
                            if ta.is_detached.get() || is_typed_array_out_of_bounds(ta) {
                                return Completion::Throw(
                                    interp.create_type_error("typed array is detached"),
                                );
                            }
                        } else {
                            return Completion::Throw(interp.create_type_error("not a TypedArray"));
                        }
                    }
                    let iter = interp.create_typed_array_iterator(o.id, IteratorKind::Value);
                    return Completion::Normal(iter);
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("values".to_string(), values_fn.clone());
        proto
            .borrow_mut()
            .insert_builtin("Symbol(Symbol.iterator)".to_string(), values_fn);
    }

    fn setup_ta_iterator_methods(&mut self, proto: &Rc<RefCell<JsObjectData>>) {
        let entries_fn = self.create_function(JsFunction::native(
            "entries".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info {
                            if ta.is_detached.get() || is_typed_array_out_of_bounds(ta) {
                                return Completion::Throw(
                                    interp.create_type_error("typed array is detached"),
                                );
                            }
                        } else {
                            return Completion::Throw(interp.create_type_error("not a TypedArray"));
                        }
                    }
                    let iter = interp.create_typed_array_iterator(o.id, IteratorKind::KeyValue);
                    return Completion::Normal(iter);
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("entries".to_string(), entries_fn);

        let keys_fn = self.create_function(JsFunction::native(
            "keys".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info {
                            if ta.is_detached.get() || is_typed_array_out_of_bounds(ta) {
                                return Completion::Throw(
                                    interp.create_type_error("typed array is detached"),
                                );
                            }
                        } else {
                            return Completion::Throw(interp.create_type_error("not a TypedArray"));
                        }
                    }
                    let iter = interp.create_typed_array_iterator(o.id, IteratorKind::Key);
                    return Completion::Normal(iter);
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("keys".to_string(), keys_fn);
    }

    #[allow(dead_code)]
    fn create_array_from_ta(&mut self, ta: &TypedArrayInfo) -> Rc<RefCell<JsObjectData>> {
        let elems: Vec<JsValue> = (0..typed_array_length(ta))
            .map(|i| typed_array_get_index(ta, i))
            .collect();
        let len = elems.len();
        let arr = self.create_object();
        {
            let mut a = arr.borrow_mut();
            a.class_name = "Array".to_string();
            a.prototype = self.realm().array_prototype.clone();
            a.array_elements = Some(elems);
            a.insert_builtin("length".to_string(), JsValue::Number(len as f64));
        }
        arr
    }

    fn setup_ta_higher_order_methods(&mut self, proto: &Rc<RefCell<JsObjectData>>) {
        // find
        let find_fn = self.create_function(JsFunction::native(
            "find".to_string(),
            1,
            |interp, this_val, args| {
                let (ta, callback) = match extract_ta_and_callback(interp, this_val, args) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let len = typed_array_length(&ta);
                for i in 0..len {
                    let val = typed_array_get_index(&ta, i);
                    match interp.call_function(
                        &callback,
                        &this_arg,
                        &[val.clone(), JsValue::Number(i as f64), this_val.clone()],
                    ) {
                        Completion::Normal(result) => {
                            if interp.to_boolean_val(&result) {
                                return Completion::Normal(val);
                            }
                        }
                        other => return other,
                    }
                }
                Completion::Normal(JsValue::Undefined)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("find".to_string(), find_fn);

        // findIndex
        let find_index_fn = self.create_function(JsFunction::native(
            "findIndex".to_string(),
            1,
            |interp, this_val, args| {
                let (ta, callback) = match extract_ta_and_callback(interp, this_val, args) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let len = typed_array_length(&ta);
                for i in 0..len {
                    let val = typed_array_get_index(&ta, i);
                    match interp.call_function(
                        &callback,
                        &this_arg,
                        &[val, JsValue::Number(i as f64), this_val.clone()],
                    ) {
                        Completion::Normal(result) => {
                            if interp.to_boolean_val(&result) {
                                return Completion::Normal(JsValue::Number(i as f64));
                            }
                        }
                        other => return other,
                    }
                }
                Completion::Normal(JsValue::Number(-1.0))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("findIndex".to_string(), find_index_fn);

        // findLast
        let find_last_fn = self.create_function(JsFunction::native(
            "findLast".to_string(),
            1,
            |interp, this_val, args| {
                let (ta, callback) = match extract_ta_and_callback(interp, this_val, args) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let len = typed_array_length(&ta);
                let mut i = len as i64 - 1;
                while i >= 0 {
                    let val = typed_array_get_index(&ta, i as usize);
                    match interp.call_function(
                        &callback,
                        &this_arg,
                        &[val.clone(), JsValue::Number(i as f64), this_val.clone()],
                    ) {
                        Completion::Normal(result) => {
                            if interp.to_boolean_val(&result) {
                                return Completion::Normal(val);
                            }
                        }
                        other => return other,
                    }
                    i -= 1;
                }
                Completion::Normal(JsValue::Undefined)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("findLast".to_string(), find_last_fn);

        // findLastIndex
        let find_last_index_fn = self.create_function(JsFunction::native(
            "findLastIndex".to_string(),
            1,
            |interp, this_val, args| {
                let (ta, callback) = match extract_ta_and_callback(interp, this_val, args) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let len = typed_array_length(&ta);
                let mut i = len as i64 - 1;
                while i >= 0 {
                    let val = typed_array_get_index(&ta, i as usize);
                    match interp.call_function(
                        &callback,
                        &this_arg,
                        &[val, JsValue::Number(i as f64), this_val.clone()],
                    ) {
                        Completion::Normal(result) => {
                            if interp.to_boolean_val(&result) {
                                return Completion::Normal(JsValue::Number(i as f64));
                            }
                        }
                        other => return other,
                    }
                    i -= 1;
                }
                Completion::Normal(JsValue::Number(-1.0))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("findLastIndex".to_string(), find_last_index_fn);

        // forEach
        let for_each_fn = self.create_function(JsFunction::native(
            "forEach".to_string(),
            1,
            |interp, this_val, args| {
                let (ta, callback) = match extract_ta_and_callback(interp, this_val, args) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let len = typed_array_length(&ta);
                for i in 0..len {
                    let val = typed_array_get_index(&ta, i);
                    match interp.call_function(
                        &callback,
                        &this_arg,
                        &[val, JsValue::Number(i as f64), this_val.clone()],
                    ) {
                        Completion::Normal(_) => {}
                        other => return other,
                    }
                }
                Completion::Normal(JsValue::Undefined)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("forEach".to_string(), for_each_fn);

        // map
        let map_fn = self.create_function(JsFunction::native(
            "map".to_string(),
            1,
            |interp, this_val, args| {
                let (ta, callback) = match extract_ta_and_callback(interp, this_val, args) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let len = typed_array_length(&ta);

                // Use TypedArraySpeciesCreate
                let new_ta_val = match interp
                    .typed_array_species_create(this_val, &[JsValue::Number(len as f64)])
                {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                interp.gc_root_value(&new_ta_val);

                let new_ta = if let JsValue::Object(o) = &new_ta_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    obj.borrow().typed_array_info.as_ref().unwrap().clone()
                } else {
                    return Completion::Throw(interp.create_type_error("not a TypedArray"));
                };

                for i in 0..len {
                    let val = typed_array_get_index(&ta, i);
                    match interp.call_function(
                        &callback,
                        &this_arg,
                        &[val, JsValue::Number(i as f64), this_val.clone()],
                    ) {
                        Completion::Normal(result) => {
                            typed_array_set_index(&new_ta, i, &result);
                        }
                        other => return other,
                    }
                }
                Completion::Normal(new_ta_val)
            },
        ));
        proto.borrow_mut().insert_builtin("map".to_string(), map_fn);

        // filter
        let filter_fn = self.create_function(JsFunction::native(
            "filter".to_string(),
            1,
            |interp, this_val, args| {
                let (ta, callback) = match extract_ta_and_callback(interp, this_val, args) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let mut kept: Vec<JsValue> = Vec::new();
                let len = typed_array_length(&ta);
                for i in 0..len {
                    let val = typed_array_get_index(&ta, i);
                    match interp.call_function(
                        &callback,
                        &this_arg,
                        &[val.clone(), JsValue::Number(i as f64), this_val.clone()],
                    ) {
                        Completion::Normal(result) => {
                            if interp.to_boolean_val(&result) {
                                kept.push(val);
                            }
                        }
                        other => return other,
                    }
                }
                let len = kept.len();

                // Use TypedArraySpeciesCreate
                let new_ta_val = match interp
                    .typed_array_species_create(this_val, &[JsValue::Number(len as f64)])
                {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                if let JsValue::Object(o) = &new_ta_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let new_ta = obj.borrow().typed_array_info.as_ref().unwrap().clone();
                    for (i, val) in kept.iter().enumerate() {
                        typed_array_set_index(&new_ta, i, val);
                    }
                }
                Completion::Normal(new_ta_val)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("filter".to_string(), filter_fn);

        // every
        let every_fn = self.create_function(JsFunction::native(
            "every".to_string(),
            1,
            |interp, this_val, args| {
                let (ta, callback) = match extract_ta_and_callback(interp, this_val, args) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let len = typed_array_length(&ta);
                for i in 0..len {
                    let val = typed_array_get_index(&ta, i);
                    match interp.call_function(
                        &callback,
                        &this_arg,
                        &[val, JsValue::Number(i as f64), this_val.clone()],
                    ) {
                        Completion::Normal(result) => {
                            if !interp.to_boolean_val(&result) {
                                return Completion::Normal(JsValue::Boolean(false));
                            }
                        }
                        other => return other,
                    }
                }
                Completion::Normal(JsValue::Boolean(true))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("every".to_string(), every_fn);

        // some
        let some_fn = self.create_function(JsFunction::native(
            "some".to_string(),
            1,
            |interp, this_val, args| {
                let (ta, callback) = match extract_ta_and_callback(interp, this_val, args) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let len = typed_array_length(&ta);
                for i in 0..len {
                    let val = typed_array_get_index(&ta, i);
                    match interp.call_function(
                        &callback,
                        &this_arg,
                        &[val, JsValue::Number(i as f64), this_val.clone()],
                    ) {
                        Completion::Normal(result) => {
                            if interp.to_boolean_val(&result) {
                                return Completion::Normal(JsValue::Boolean(true));
                            }
                        }
                        other => return other,
                    }
                }
                Completion::Normal(JsValue::Boolean(false))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("some".to_string(), some_fn);

        // reduce
        let reduce_fn = self.create_function(JsFunction::native(
            "reduce".to_string(),
            1,
            |interp, this_val, args| {
                let (ta, callback) = match extract_ta_and_callback(interp, this_val, args) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = typed_array_length(&ta);
                let mut acc;
                let start;
                if args.len() > 1 {
                    acc = args[1].clone();
                    start = 0;
                } else {
                    if len == 0 {
                        return Completion::Throw(
                            interp.create_type_error("Reduce of empty array with no initial value"),
                        );
                    }
                    acc = typed_array_get_index(&ta, 0);
                    start = 1;
                }
                for i in start..len {
                    let val = typed_array_get_index(&ta, i);
                    match interp.call_function(
                        &callback,
                        &JsValue::Undefined,
                        &[acc, val, JsValue::Number(i as f64), this_val.clone()],
                    ) {
                        Completion::Normal(result) => acc = result,
                        other => return other,
                    }
                }
                Completion::Normal(acc)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("reduce".to_string(), reduce_fn);

        // reduceRight
        let reduce_right_fn = self.create_function(JsFunction::native(
            "reduceRight".to_string(),
            1,
            |interp, this_val, args| {
                let (ta, callback) = match extract_ta_and_callback(interp, this_val, args) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = typed_array_length(&ta);
                let mut acc;
                let start: i64;
                if args.len() > 1 {
                    acc = args[1].clone();
                    start = len as i64 - 1;
                } else {
                    if len == 0 {
                        return Completion::Throw(
                            interp.create_type_error("Reduce of empty array with no initial value"),
                        );
                    }
                    acc = typed_array_get_index(&ta, len - 1);
                    start = len as i64 - 2;
                }
                let mut i = start;
                while i >= 0 {
                    let val = typed_array_get_index(&ta, i as usize);
                    match interp.call_function(
                        &callback,
                        &JsValue::Undefined,
                        &[acc, val, JsValue::Number(i as f64), this_val.clone()],
                    ) {
                        Completion::Normal(result) => acc = result,
                        other => return other,
                    }
                    i -= 1;
                }
                Completion::Normal(acc)
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("reduceRight".to_string(), reduce_right_fn);
    }

    fn setup_typed_array_constructors(&mut self) {
        let kinds = [
            TypedArrayKind::Int8,
            TypedArrayKind::Uint8,
            TypedArrayKind::Uint8Clamped,
            TypedArrayKind::Int16,
            TypedArrayKind::Uint16,
            TypedArrayKind::Int32,
            TypedArrayKind::Uint32,
            TypedArrayKind::Float32,
            TypedArrayKind::Float64,
            TypedArrayKind::BigInt64,
            TypedArrayKind::BigUint64,
        ];

        // %TypedArray% constructor (not directly constructible, but holds from/of)
        let ta_proto = self.realm().typed_array_prototype.clone().unwrap();
        let ta_ctor = self.create_function(JsFunction::constructor(
            "TypedArray".to_string(),
            0,
            |interp, _this, _args| {
                Completion::Throw(
                    interp
                        .create_type_error("Abstract class TypedArray not directly constructable"),
                )
            },
        ));
        // TypedArray.from
        let ta_from_fn = self.create_function(JsFunction::native(
            "from".to_string(),
            1,
            |interp, this_val, args| {
                // this_val is the constructor (e.g. Uint8Array)
                let source = args.first().cloned().unwrap_or(JsValue::Undefined);
                let map_fn = args.get(1).cloned();
                let this_arg = args.get(2).cloned().unwrap_or(JsValue::Undefined);

                // Step 3: If mapfn is provided and not undefined, check callable
                let mapping = if let Some(ref mf) = map_fn
                    && !matches!(mf, JsValue::Undefined)
                {
                    let is_callable = matches!(mf, JsValue::Object(o) if {
                        interp.get_object(o.id).is_some_and(|obj| obj.borrow().callable.is_some())
                    });
                    if !is_callable {
                        return Completion::Throw(
                            interp.create_type_error("mapfn is not a function"),
                        );
                    }
                    true
                } else {
                    false
                };

                // Collect source values (iterable or array-like)
                let values = interp.collect_iterable_or_arraylike(&source);
                let values = match values {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let len = values.len();

                // Create the target object by calling the constructor with len
                let target_obj = match interp.typed_array_create(this_val, len) {
                    Completion::Normal(v) => v,
                    other => return other,
                };

                // For each element: apply mapper if any, then Set(targetObj, k, value, true)
                let ta_kind = if let JsValue::Object(ref o) = target_obj {
                    interp
                        .get_object(o.id)
                        .and_then(|obj| obj.borrow().typed_array_info.as_ref().map(|ta| ta.kind))
                } else {
                    None
                };
                let ta_kind = match ta_kind {
                    Some(k) => k,
                    None => return Completion::Throw(
                        interp.create_type_error("TypedArray.from: target is not a TypedArray"),
                    ),
                };

                for (k, val) in values.iter().enumerate() {
                    let mapped_val = if mapping {
                        let mf = map_fn.as_ref().unwrap();
                        match interp.call_function(
                            mf,
                            &this_arg,
                            &[val.clone(), JsValue::Number(k as f64)],
                        ) {
                            Completion::Normal(v) => v,
                            other => return other,
                        }
                    } else {
                        val.clone()
                    };
                    // Coerce and Set (silently ignores OOB/detached)
                    let coerced = match interp.typed_array_coerce_value(ta_kind, &mapped_val) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    };
                    let key = k.to_string();
                    if let JsValue::Object(ref o) = target_obj {
                        if let Some(obj) = interp.get_object(o.id) {
                            obj.borrow_mut().set_property_value(&key, coerced);
                        }
                    }
                }
                Completion::Normal(target_obj)
            },
        ));
        if let JsValue::Object(o) = &ta_ctor
            && let Some(obj) = self.get_object(o.id)
        {
            obj.borrow_mut()
                .insert_builtin("from".to_string(), ta_from_fn);
        }
        // TypedArray.of — spec §23.2.2.2
        let ta_of_fn = self.create_function(JsFunction::native(
            "of".to_string(),
            0,
            |interp, this_val, args| {
                let len = args.len();
                // Step 4: Let newObj = TypedArrayCreate(C, len)
                let new_obj = match interp.typed_array_create(this_val, len) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                // Get ta_kind for coercion
                let ta_kind = if let JsValue::Object(ref o) = new_obj {
                    interp
                        .get_object(o.id)
                        .and_then(|obj| obj.borrow().typed_array_info.as_ref().map(|ta| ta.kind))
                } else {
                    None
                };
                let ta_kind = match ta_kind {
                    Some(k) => k,
                    None => return Completion::Throw(
                        interp.create_type_error("TypedArray.of: not a TypedArray"),
                    ),
                };
                // Step 5-6: Set each element
                for (k, val) in args.iter().enumerate() {
                    let coerced = match interp.typed_array_coerce_value(ta_kind, val) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    };
                    let key = k.to_string();
                    if let JsValue::Object(ref o) = new_obj {
                        if let Some(obj) = interp.get_object(o.id) {
                            obj.borrow_mut().set_property_value(&key, coerced);
                        }
                    }
                }
                Completion::Normal(new_obj)
            },
        ));
        if let JsValue::Object(o) = &ta_ctor
            && let Some(obj) = self.get_object(o.id)
        {
            obj.borrow_mut().insert_builtin("of".to_string(), ta_of_fn);
        }

        // Set %TypedArray%.prototype → %TypedArray.prototype%
        if let JsValue::Object(o) = &ta_ctor
            && let Some(obj) = self.get_object(o.id)
        {
            let proto_id = ta_proto.borrow().id.unwrap();
            obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(
                    JsValue::Object(JsObject { id: proto_id }),
                    false,
                    false,
                    false,
                ),
            );
        }

        // Set %TypedArray.prototype%.constructor → %TypedArray%
        ta_proto.borrow_mut().insert_property(
            "constructor".to_string(),
            PropertyDescriptor::data(ta_ctor.clone(), true, false, true),
        );

        // Add @@species getter on %TypedArray%
        let species_getter = self.create_function(JsFunction::native(
            "get [Symbol.species]".to_string(),
            0,
            |_interp, this_val, _args| Completion::Normal(this_val.clone()),
        ));
        if let JsValue::Object(o) = &ta_ctor
            && let Some(obj) = self.get_object(o.id)
        {
            obj.borrow_mut().insert_property(
                "Symbol(Symbol.species)".to_string(),
                PropertyDescriptor {
                    value: None,
                    writable: None,
                    get: Some(species_getter),
                    set: None,
                    enumerable: Some(false),
                    configurable: Some(true),
                },
            );
        }

        self.realm_mut().typed_array_constructor = Some(ta_ctor.clone());

        for kind in kinds {
            let name = kind.name().to_string();
            let bpe = kind.bytes_per_element();
            let ta_proto_clone = ta_proto.clone();
            let ta_ctor_clone = ta_ctor.clone();

            // Create per-type prototype
            let type_proto = self.create_object();
            {
                let mut p = type_proto.borrow_mut();
                p.prototype = Some(ta_proto_clone.clone());
                p.class_name = name.clone();
                p.insert_property(
                    "BYTES_PER_ELEMENT".to_string(),
                    PropertyDescriptor::data(JsValue::Number(bpe as f64), false, false, false),
                );
            }

            let type_proto_clone = type_proto.clone();
            let ctor = self.create_function(JsFunction::constructor(
                name.clone(), 3,
                move |interp, _this, args| {
                    // §23.2.5.1 step 2: If NewTarget is undefined, throw TypeError
                    if interp.new_target.is_none() {
                        return Completion::Throw(
                            interp.create_type_error(&format!("{} is not a constructor", kind.name())),
                        );
                    }
                    // OrdinaryCreateFromConstructor: get prototype from NewTarget
                    let proto = match interp.get_prototype_from_new_target(&Some(type_proto_clone.clone())) {
                        Ok(p) => p.unwrap_or_else(|| type_proto_clone.clone()),
                        Err(e) => return Completion::Throw(e),
                    };
                    if args.is_empty() {
                        // new XArray() -> length 0
                        return interp.create_typed_array_from_length(kind, 0, &proto);
                    }
                    let first = &args[0];
                    match first {
                        JsValue::Object(o) => {
                            if let Some(src_obj) = interp.get_object(o.id) {
                                let src_ref = src_obj.borrow();
                                // Case: new XArray(arraybuffer, byteOffset?, length?)
                                if let Some(ref ab_data) = src_ref.arraybuffer_data {
                                    if let Some(ref det) = src_ref.arraybuffer_detached
                                        && det.get() {
                                            return Completion::Throw(interp.create_type_error(
                                                "Cannot construct TypedArray from detached ArrayBuffer"
                                            ));
                                        }
                                    let buf_rc = ab_data.clone();
                                    let detached = src_ref.arraybuffer_detached.clone()
                                        .unwrap_or_else(|| Rc::new(Cell::new(false)));
                                    let is_resizable = src_ref.arraybuffer_max_byte_length.is_some();
                                    let buf_len = buf_rc.borrow().len();
                                    drop(src_ref);
                                    let byte_offset = if args.len() > 1 && !matches!(args[1], JsValue::Undefined) {
                                        let offset_val = match interp.to_index(&args[1]) {
                                            Completion::Normal(v) => v,
                                            Completion::Throw(e) => return Completion::Throw(e),
                                            _ => return Completion::Normal(JsValue::Undefined),
                                        };
                                        if let JsValue::Number(n) = offset_val { n as usize } else { 0 }
                                    } else { 0 };
                                    if byte_offset % bpe != 0 {
                                        return Completion::Throw(interp.create_error("RangeError",
                                            "start offset of typed array should be a multiple of BYTES_PER_ELEMENT"
                                        ));
                                    }
                                    let has_length_arg = args.len() > 2 && !matches!(args[2], JsValue::Undefined);
                                    let is_length_tracking = is_resizable && !has_length_arg;
                                    let array_length = if has_length_arg {
                                        let len_val = match interp.to_index(&args[2]) {
                                            Completion::Normal(v) => v,
                                            Completion::Throw(e) => return Completion::Throw(e),
                                            _ => return Completion::Normal(JsValue::Undefined),
                                        };
                                        if let JsValue::Number(n) = len_val { n as usize } else { 0 }
                                    } else {
                                        if buf_len < byte_offset {
                                            return Completion::Throw(interp.create_error("RangeError",
                                                "start offset is outside the bounds of the buffer"
                                            ));
                                        }
                                        if !is_resizable && (buf_len - byte_offset) % bpe != 0 {
                                            return Completion::Throw(interp.create_error("RangeError",
                                                "byte length of typed array should be a multiple of BYTES_PER_ELEMENT"
                                            ));
                                        }
                                        (buf_len - byte_offset) / bpe
                                    };
                                    let byte_length = array_length * bpe;
                                    if byte_offset + byte_length > buf_len {
                                        return Completion::Throw(interp.create_error("RangeError", "invalid typed array length"));
                                    }
                                    let ta_info = TypedArrayInfo {
                                        kind,
                                        buffer: buf_rc,
                                        byte_offset,
                                        byte_length,
                                        array_length,
                                        is_detached: detached,
                                        is_length_tracking,
                                    };
                                    let buf_val = first.clone();
                                    let result = interp.create_typed_array_object_with_proto(ta_info, buf_val, &proto);
                                    let id = result.borrow().id.unwrap();
                                    return Completion::Normal(JsValue::Object(JsObject { id }));
                                }
                                // Case: new XArray(typedArray)
                                if let Some(ref src_ta) = src_ref.typed_array_info {
                                    let src_ta = src_ta.clone();
                                    drop(src_ref);
                                    // Check content type compatibility
                                    if kind.is_bigint() != src_ta.kind.is_bigint() {
                                        return Completion::Throw(interp.create_type_error(
                                            "cannot mix BigInt and non-BigInt typed arrays",
                                        ));
                                    }
                                    if src_ta.is_detached.get() {
                                        return Completion::Throw(interp.create_type_error(
                                            "source typed array is detached",
                                        ));
                                    }
                                    let len = typed_array_length(&src_ta);
                                    let new_buf = vec![0u8; len * bpe];
                                    let new_buf_rc = Rc::new(RefCell::new(new_buf));
                                    let new_detached = Rc::new(Cell::new(false));
                                    let new_ta = TypedArrayInfo {
                                        kind,
                                        buffer: new_buf_rc.clone(),
                                        byte_offset: 0,
                                        byte_length: len * bpe,
                                        array_length: len,
                                        is_detached: new_detached.clone(),
                                        is_length_tracking: false,
                                    };
                                    for i in 0..len {
                                        let val = typed_array_get_index(&src_ta, i);
                                        typed_array_set_index(&new_ta, i, &val);
                                    }
                                    let ab_obj = interp.create_object();
                                    {
                                        let mut ab = ab_obj.borrow_mut();
                                        ab.class_name = "ArrayBuffer".to_string();
                                        ab.prototype = interp.realm().arraybuffer_prototype.clone();
                                        ab.arraybuffer_data = Some(new_buf_rc);
                                        ab.arraybuffer_detached = Some(new_detached);
                                    }
                                    let ab_id = ab_obj.borrow().id.unwrap();
                                    let buf_val = JsValue::Object(JsObject { id: ab_id });
                                    let result = interp.create_typed_array_object_with_proto(new_ta, buf_val, &proto);
                                    let id = result.borrow().id.unwrap();
                                    return Completion::Normal(JsValue::Object(JsObject { id }));
                                }
                                // Case: new XArray(arrayLike/iterable)
                                drop(src_ref);
                                let values = interp.collect_iterable_or_arraylike(first);
                                let values = match values {
                                    Ok(v) => v,
                                    Err(c) => return c,
                                };
                                let len = values.len();
                                let new_buf = vec![0u8; len * bpe];
                                let new_buf_rc = Rc::new(RefCell::new(new_buf));
                                let new_detached = Rc::new(Cell::new(false));
                                let new_ta = TypedArrayInfo {
                                    kind,
                                    buffer: new_buf_rc.clone(),
                                    byte_offset: 0,
                                    byte_length: len * bpe,
                                    array_length: len,
                                    is_detached: new_detached.clone(),
                                    is_length_tracking: false,
                                };
                                for (i, val) in values.iter().enumerate() {
                                    let coerced = match interp.typed_array_coerce_value(kind, val) {
                                        Ok(v) => v,
                                        Err(e) => return Completion::Throw(e),
                                    };
                                    typed_array_set_index(&new_ta, i, &coerced);
                                }
                                let ab_obj = interp.create_object();
                                {
                                    let mut ab = ab_obj.borrow_mut();
                                    ab.class_name = "ArrayBuffer".to_string();
                                    ab.prototype = interp.realm().arraybuffer_prototype.clone();
                                    ab.arraybuffer_data = Some(new_buf_rc);
                                    ab.arraybuffer_detached = Some(new_detached);
                                }
                                let ab_id = ab_obj.borrow().id.unwrap();
                                let buf_val = JsValue::Object(JsObject { id: ab_id });
                                let result = interp.create_typed_array_object_with_proto(new_ta, buf_val, &proto);
                                let id = result.borrow().id.unwrap();
                                return Completion::Normal(JsValue::Object(JsObject { id }));
                            }
                            Completion::Throw(interp.create_type_error("invalid argument"))
                        }
                        _ => {
                            // §22.2.5.1: If firstArgument is not an Object, treat as length
                            let len_val = match interp.to_index(first) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => return Completion::Normal(JsValue::Undefined),
                            };
                            let len = if let JsValue::Number(n) = len_val { n as usize } else { 0 };
                            interp.create_typed_array_from_length(kind, len, &proto)
                        }
                    }
                },
            ));

            // Set BYTES_PER_ELEMENT on constructor
            if let JsValue::Object(o) = &ctor
                && let Some(obj) = self.get_object(o.id)
            {
                obj.borrow_mut().insert_property(
                    "BYTES_PER_ELEMENT".to_string(),
                    PropertyDescriptor::data(JsValue::Number(bpe as f64), false, false, false),
                );
                // Set prototype property
                let proto_id = type_proto.borrow().id.unwrap();
                obj.borrow_mut().insert_property(
                    "prototype".to_string(),
                    PropertyDescriptor::data(
                        JsValue::Object(JsObject { id: proto_id }),
                        false,
                        false,
                        false,
                    ),
                );
                // Set __proto__ to %TypedArray% so from/of are inherited
                if let JsValue::Object(ta_o) = &ta_ctor_clone
                    && let Some(ta_obj) = self.get_object(ta_o.id)
                {
                    obj.borrow_mut().prototype = Some(ta_obj.clone());
                }
            }

            // Set constructor on prototype
            type_proto.borrow_mut().insert_property(
                "constructor".to_string(),
                PropertyDescriptor::data(ctor.clone(), true, false, true),
            );

            // Store prototype for this kind
            match kind {
                TypedArrayKind::Int8 => self.realm_mut().int8array_prototype = Some(type_proto.clone()),
                TypedArrayKind::Uint8 => self.realm_mut().uint8array_prototype = Some(type_proto.clone()),
                TypedArrayKind::Uint8Clamped => {
                    self.realm_mut().uint8clampedarray_prototype = Some(type_proto.clone())
                }
                TypedArrayKind::Int16 => self.realm_mut().int16array_prototype = Some(type_proto.clone()),
                TypedArrayKind::Uint16 => self.realm_mut().uint16array_prototype = Some(type_proto.clone()),
                TypedArrayKind::Int32 => self.realm_mut().int32array_prototype = Some(type_proto.clone()),
                TypedArrayKind::Uint32 => self.realm_mut().uint32array_prototype = Some(type_proto.clone()),
                TypedArrayKind::Float32 => self.realm_mut().float32array_prototype = Some(type_proto.clone()),
                TypedArrayKind::Float64 => self.realm_mut().float64array_prototype = Some(type_proto.clone()),
                TypedArrayKind::BigInt64 => self.realm_mut().bigint64array_prototype = Some(type_proto.clone()),
                TypedArrayKind::BigUint64 => {
                    self.realm_mut().biguint64array_prototype = Some(type_proto.clone())
                }
            }

            self.realm().global_env
                .borrow_mut()
                .declare(&name, BindingKind::Var);
            let _ = self.realm().global_env.borrow_mut().set(&name, ctor);
        }
    }

    fn setup_uint8array_base64_hex(&mut self) {
        // Get Uint8Array constructor from global env
        let uint8_ctor = self.realm().global_env.borrow().get("Uint8Array").unwrap();
        let uint8_proto = self.realm().uint8array_prototype.clone().unwrap();

        // --- Static methods on Uint8Array constructor ---

        // Uint8Array.fromBase64(string [, options])
        let from_base64_fn = self.create_function(JsFunction::native(
            "fromBase64".to_string(),
            1,
            |interp, _this, args| {
                let input = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(input, JsValue::String(_)) {
                    return Completion::Throw(
                        interp.create_type_error("fromBase64 requires a string argument"),
                    );
                }
                let input_str = to_js_string(&input);

                let opts = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let (alphabet, last_chunk) = match parse_base64_options(interp, &opts) {
                    Ok(v) => v,
                    Err(c) => return c,
                };

                let result = decode_base64(&input_str, &alphabet, &last_chunk, None);
                if let Some(msg) = result.error {
                    return Completion::Throw(interp.create_error("SyntaxError", &msg));
                }
                create_uint8array_from_bytes(interp, &result.bytes)
            },
        ));

        // Uint8Array.fromHex(string)
        let from_hex_fn = self.create_function(JsFunction::native(
            "fromHex".to_string(),
            1,
            |interp, _this, args| {
                let input = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(input, JsValue::String(_)) {
                    return Completion::Throw(
                        interp.create_type_error("fromHex requires a string argument"),
                    );
                }
                let input_str = to_js_string(&input);

                let result = decode_hex(&input_str, None);
                if let Some(msg) = result.error {
                    return Completion::Throw(interp.create_error("SyntaxError", &msg));
                }
                create_uint8array_from_bytes(interp, &result.bytes)
            },
        ));

        if let JsValue::Object(o) = &uint8_ctor
            && let Some(obj) = self.get_object(o.id)
        {
            obj.borrow_mut()
                .insert_builtin("fromBase64".to_string(), from_base64_fn);
            obj.borrow_mut()
                .insert_builtin("fromHex".to_string(), from_hex_fn);
        }

        // --- Instance methods on Uint8Array.prototype ---

        // toHex()
        let to_hex_fn = self.create_function(JsFunction::native(
            "toHex".to_string(),
            0,
            |interp, this_val, _args| {
                let ta = match validate_uint8array(interp, this_val) {
                    Ok(ta) => ta,
                    Err(c) => return c,
                };
                let buf = ta.buffer.borrow();
                let start = ta.byte_offset;
                let end = start + ta.byte_length;
                let mut result = String::with_capacity(ta.byte_length * 2);
                for &b in &buf[start..end] {
                    result.push_str(&format!("{:02x}", b));
                }
                Completion::Normal(JsValue::String(JsString::from_str(&result)))
            },
        ));
        uint8_proto
            .borrow_mut()
            .insert_builtin("toHex".to_string(), to_hex_fn);

        // toBase64([options])
        let to_base64_fn = self.create_function(JsFunction::native(
            "toBase64".to_string(),
            0,
            |interp, this_val, args| {
                let ta = match validate_uint8array_no_detach_check(interp, this_val) {
                    Ok(ta) => ta,
                    Err(c) => return c,
                };

                let opts = args.first().cloned().unwrap_or(JsValue::Undefined);
                let (alphabet, omit_padding) = match parse_to_base64_options(interp, &opts) {
                    Ok(v) => v,
                    Err(c) => return c,
                };

                if let Err(c) = check_detached(interp, &ta) {
                    return c;
                }

                let buf = ta.buffer.borrow();
                let start = ta.byte_offset;
                let end = start + ta.byte_length;
                let data = &buf[start..end];

                let result = encode_base64(data, &alphabet, omit_padding);
                Completion::Normal(JsValue::String(JsString::from_str(&result)))
            },
        ));
        uint8_proto
            .borrow_mut()
            .insert_builtin("toBase64".to_string(), to_base64_fn);

        // setFromHex(string)
        let set_from_hex_fn = self.create_function(JsFunction::native(
            "setFromHex".to_string(),
            1,
            |interp, this_val, args| {
                let ta = match validate_uint8array(interp, this_val) {
                    Ok(ta) => ta,
                    Err(c) => return c,
                };

                let input = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(input, JsValue::String(_)) {
                    return Completion::Throw(
                        interp.create_type_error("setFromHex requires a string argument"),
                    );
                }
                let input_str = to_js_string(&input);

                let max_bytes = typed_array_length(&ta);
                let result = decode_hex(&input_str, Some(max_bytes));
                let written = result.bytes.len();
                {
                    let mut buf = ta.buffer.borrow_mut();
                    let start = ta.byte_offset;
                    for (idx, &b) in result.bytes.iter().enumerate() {
                        buf[start + idx] = b;
                    }
                }
                if let Some(msg) = result.error {
                    return Completion::Throw(interp.create_error("SyntaxError", &msg));
                }
                make_read_written_result(interp, result.read, written)
            },
        ));
        uint8_proto
            .borrow_mut()
            .insert_builtin("setFromHex".to_string(), set_from_hex_fn);

        // setFromBase64(string [, options])
        let set_from_base64_fn = self.create_function(JsFunction::native(
            "setFromBase64".to_string(),
            1,
            |interp, this_val, args| {
                let ta = match validate_uint8array_no_detach_check(interp, this_val) {
                    Ok(ta) => ta,
                    Err(c) => return c,
                };

                let input = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(input, JsValue::String(_)) {
                    return Completion::Throw(
                        interp.create_type_error("setFromBase64 requires a string argument"),
                    );
                }
                let input_str = to_js_string(&input);

                let opts = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let (alphabet, last_chunk) = match parse_base64_options(interp, &opts) {
                    Ok(v) => v,
                    Err(c) => return c,
                };

                if let Err(c) = check_detached(interp, &ta) {
                    return c;
                }

                let max_bytes = typed_array_length(&ta);
                let result = decode_base64(&input_str, &alphabet, &last_chunk, Some(max_bytes));
                let written = result.bytes.len();
                {
                    let mut buf = ta.buffer.borrow_mut();
                    let start = ta.byte_offset;
                    for (idx, &b) in result.bytes.iter().enumerate() {
                        buf[start + idx] = b;
                    }
                }
                if let Some(msg) = result.error {
                    return Completion::Throw(interp.create_error("SyntaxError", &msg));
                }
                make_read_written_result(interp, result.read, written)
            },
        ));
        uint8_proto
            .borrow_mut()
            .insert_builtin("setFromBase64".to_string(), set_from_base64_fn);
    }

    /// TypedArraySpeciesCreate(exemplar, argumentList) — §23.2.4.1
    /// Creates a new TypedArray using @@species from the exemplar's constructor.
    pub(crate) fn typed_array_species_create(
        &mut self,
        exemplar: &JsValue,
        args: &[JsValue],
    ) -> Result<JsValue, JsValue> {
        let (kind, _ta) = if let JsValue::Object(o) = exemplar
            && let Some(obj) = self.get_object(o.id)
        {
            let obj_ref = obj.borrow();
            if let Some(ref ta) = obj_ref.typed_array_info {
                (ta.kind, ta.clone())
            } else {
                return Err(self.create_type_error("not a TypedArray"));
            }
        } else {
            return Err(self.create_type_error("not a TypedArray"));
        };

        let default_ctor_name = kind.name();
        let default_ctor = self.realm()
            .global_env
            .borrow()
            .get(default_ctor_name)
            .unwrap_or(JsValue::Undefined);

        let ctor = self.species_constructor(exemplar, &default_ctor)?;

        let result = self.construct_with_new_target(&ctor, args, ctor.clone());
        let result_val = match result {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            _ => return Err(self.create_type_error("constructor returned abnormally")),
        };

        // Validate result is a TypedArray
        let result_kind = if let JsValue::Object(o) = &result_val
            && let Some(obj) = self.get_object(o.id)
        {
            let obj_ref = obj.borrow();
            if let Some(ref ta) = obj_ref.typed_array_info {
                if ta.is_detached.get() {
                    return Err(self.create_type_error("new TypedArray is detached"));
                }
                ta.kind
            } else {
                return Err(
                    self.create_type_error("species constructor did not return a TypedArray")
                );
            }
        } else {
            return Err(self.create_type_error("species constructor did not return a TypedArray"));
        };

        // ContentType compatibility check (only for single-length-arg case)
        if args.len() == 1 && kind.is_bigint() != result_kind.is_bigint() {
            return Err(self.create_type_error(
                "species constructor returned a TypedArray with incompatible content type",
            ));
        }

        // Validate length >= requested
        if let Some(JsValue::Number(requested_len)) = args.first() {
            let requested = *requested_len as usize;
            if let JsValue::Object(o) = &result_val
                && let Some(obj) = self.get_object(o.id)
            {
                let obj_ref = obj.borrow();
                if let Some(ref ta) = obj_ref.typed_array_info
                    && typed_array_length(ta) < requested
                {
                    return Err(self.create_type_error(
                        "species constructor returned a TypedArray that is too small",
                    ));
                }
            }
        }

        Ok(result_val)
    }

    /// TypedArrayCreateSameType(exemplar, argumentList) — §23.2.4.3
    /// Creates a new TypedArray of the same kind, without using @@species.
    #[allow(dead_code)]
    fn typed_array_create_same_type(
        &mut self,
        exemplar_kind: TypedArrayKind,
        len: usize,
    ) -> Completion {
        let proto = self.get_typed_array_prototype(exemplar_kind);
        let bpe = exemplar_kind.bytes_per_element();
        let buf = vec![0u8; len * bpe];
        let buf_rc = Rc::new(RefCell::new(buf));
        let detached = Rc::new(Cell::new(false));
        let ta_info = TypedArrayInfo {
            kind: exemplar_kind,
            buffer: buf_rc.clone(),
            byte_offset: 0,
            byte_length: len * bpe,
            array_length: len,
            is_detached: detached.clone(),
            is_length_tracking: false,
        };
        let ab_obj = self.create_object();
        {
            let mut ab = ab_obj.borrow_mut();
            ab.class_name = "ArrayBuffer".to_string();
            ab.prototype = self.realm().arraybuffer_prototype.clone();
            ab.arraybuffer_data = Some(buf_rc);
            ab.arraybuffer_detached = Some(detached);
        }
        let ab_id = ab_obj.borrow().id.unwrap();
        let result = self.create_object();
        {
            let mut r = result.borrow_mut();
            r.class_name = exemplar_kind.name().to_string();
            r.prototype = proto;
            r.view_buffer_object_id = Some(ab_id);
            r.typed_array_info = Some(ta_info);
        }
        let id = result.borrow().id.unwrap();
        Completion::Normal(JsValue::Object(JsObject { id }))
    }

    /// Coerce a value for writing to a TypedArray element.
    /// For Number kinds: ToNumber(value). For BigInt kinds: ToBigInt(value).
    /// Returns the coerced JsValue or throws.
    pub(crate) fn typed_array_coerce_value(
        &mut self,
        kind: TypedArrayKind,
        value: &JsValue,
    ) -> Result<JsValue, JsValue> {
        if kind.is_bigint() {
            self.to_bigint_value(value)
        } else {
            self.to_number_value(value).map(JsValue::Number)
        }
    }

    fn create_typed_array_from_length(
        &mut self,
        kind: TypedArrayKind,
        len: usize,
        type_proto: &Rc<RefCell<JsObjectData>>,
    ) -> Completion {
        let bpe = kind.bytes_per_element();
        let buf = vec![0u8; len * bpe];
        let buf_rc = Rc::new(RefCell::new(buf));
        let detached = Rc::new(Cell::new(false));
        let ta_info = TypedArrayInfo {
            kind,
            buffer: buf_rc.clone(),
            byte_offset: 0,
            byte_length: len * bpe,
            array_length: len,
            is_detached: detached.clone(),
            is_length_tracking: false,
        };
        let ab_obj = self.create_object();
        {
            let mut ab = ab_obj.borrow_mut();
            ab.class_name = "ArrayBuffer".to_string();
            ab.prototype = self.realm().arraybuffer_prototype.clone();
            ab.arraybuffer_data = Some(buf_rc);
            ab.arraybuffer_detached = Some(detached);
        }
        let ab_id = ab_obj.borrow().id.unwrap();
        let buf_val = JsValue::Object(JsObject { id: ab_id });
        let result = self.create_typed_array_object_with_proto(ta_info, buf_val, type_proto);
        let id = result.borrow().id.unwrap();
        Completion::Normal(JsValue::Object(JsObject { id }))
    }

    pub(crate) fn create_typed_array_object(
        &mut self,
        info: TypedArrayInfo,
        buf_val: JsValue,
    ) -> Rc<RefCell<JsObjectData>> {
        let proto = self.get_typed_array_prototype(info.kind);
        let obj = self.create_object();
        {
            let mut o = obj.borrow_mut();
            o.class_name = info.kind.name().to_string();
            o.prototype = proto;
            if let JsValue::Object(ref bobj) = buf_val {
                o.view_buffer_object_id = Some(bobj.id);
            }
            o.typed_array_info = Some(info);
        }
        obj
    }

    fn create_typed_array_object_with_proto(
        &mut self,
        info: TypedArrayInfo,
        buf_val: JsValue,
        proto: &Rc<RefCell<JsObjectData>>,
    ) -> Rc<RefCell<JsObjectData>> {
        let obj = self.create_object();
        {
            let mut o = obj.borrow_mut();
            o.class_name = info.kind.name().to_string();
            o.prototype = Some(proto.clone());
            if let JsValue::Object(ref bobj) = buf_val {
                o.view_buffer_object_id = Some(bobj.id);
            }
            o.typed_array_info = Some(info);
        }
        obj
    }

    fn get_typed_array_prototype(&self, kind: TypedArrayKind) -> Option<Rc<RefCell<JsObjectData>>> {
        match kind {
            TypedArrayKind::Int8 => self.realm().int8array_prototype.clone(),
            TypedArrayKind::Uint8 => self.realm().uint8array_prototype.clone(),
            TypedArrayKind::Uint8Clamped => self.realm().uint8clampedarray_prototype.clone(),
            TypedArrayKind::Int16 => self.realm().int16array_prototype.clone(),
            TypedArrayKind::Uint16 => self.realm().uint16array_prototype.clone(),
            TypedArrayKind::Int32 => self.realm().int32array_prototype.clone(),
            TypedArrayKind::Uint32 => self.realm().uint32array_prototype.clone(),
            TypedArrayKind::Float32 => self.realm().float32array_prototype.clone(),
            TypedArrayKind::Float64 => self.realm().float64array_prototype.clone(),
            TypedArrayKind::BigInt64 => self.realm().bigint64array_prototype.clone(),
            TypedArrayKind::BigUint64 => self.realm().biguint64array_prototype.clone(),
        }
    }

    /// TypedArrayCreate(C, argumentList) — §23.2.4.2
    /// Creates a TypedArray by calling C with argumentList. Validates result is a TypedArray.
    fn typed_array_create(
        &mut self,
        ctor: &JsValue,
        len: usize,
    ) -> Completion {
        // Fast path for known built-in TypedArray constructors
        if let JsValue::Object(o) = ctor
            && let Some(obj) = self.get_object(o.id)
        {
            let name = {
                let obj_ref = obj.borrow();
                if let Some(ref func) = obj_ref.callable {
                    match func {
                        JsFunction::Native(n, _, _, _) => Some(n.clone()),
                        _ => None,
                    }
                } else {
                    None
                }
            };
            if let Some(ref n) = name {
                let kind = match n.as_str() {
                    "Int8Array" => Some(TypedArrayKind::Int8),
                    "Uint8Array" => Some(TypedArrayKind::Uint8),
                    "Uint8ClampedArray" => Some(TypedArrayKind::Uint8Clamped),
                    "Int16Array" => Some(TypedArrayKind::Int16),
                    "Uint16Array" => Some(TypedArrayKind::Uint16),
                    "Int32Array" => Some(TypedArrayKind::Int32),
                    "Uint32Array" => Some(TypedArrayKind::Uint32),
                    "Float32Array" => Some(TypedArrayKind::Float32),
                    "Float64Array" => Some(TypedArrayKind::Float64),
                    "BigInt64Array" => Some(TypedArrayKind::BigInt64),
                    "BigUint64Array" => Some(TypedArrayKind::BigUint64),
                    _ => None,
                };
                if let Some(kind) = kind {
                    let proto = self.get_typed_array_prototype(kind);
                    let bpe = kind.bytes_per_element();
                    let new_buf = vec![0u8; len * bpe];
                    let new_buf_rc = Rc::new(RefCell::new(new_buf));
                    let new_detached = Rc::new(Cell::new(false));
                    let ta = TypedArrayInfo {
                        kind,
                        buffer: new_buf_rc.clone(),
                        byte_offset: 0,
                        byte_length: len * bpe,
                        array_length: len,
                        is_detached: new_detached.clone(),
                        is_length_tracking: false,
                    };
                    let ab_obj = self.create_object();
                    {
                        let mut ab = ab_obj.borrow_mut();
                        ab.class_name = "ArrayBuffer".to_string();
                        ab.prototype = self.realm().arraybuffer_prototype.clone();
                        ab.arraybuffer_data = Some(new_buf_rc);
                        ab.arraybuffer_detached = Some(new_detached);
                    }
                    let ab_id = ab_obj.borrow().id.unwrap();
                    let result = self.create_object();
                    {
                        let mut r = result.borrow_mut();
                        r.class_name = kind.name().to_string();
                        r.prototype = proto;
                        r.view_buffer_object_id = Some(ab_id);
                        r.typed_array_info = Some(ta);
                    }
                    let id = result.borrow().id.unwrap();
                    return Completion::Normal(JsValue::Object(JsObject { id }));
                }
            }
        }
        // Generic path: call ctor(len), validate result is TypedArray
        let new_obj = match self.construct_with_new_target(
            ctor,
            &[JsValue::Number(len as f64)],
            ctor.clone(),
        ) {
            Completion::Normal(v) => v,
            other => return other,
        };
        if let JsValue::Object(ref o) = new_obj {
            if self
                .get_object(o.id)
                .is_some_and(|obj| obj.borrow().typed_array_info.is_some())
            {
                return Completion::Normal(new_obj);
            }
        }
        Completion::Throw(
            self.create_type_error("TypedArray.from/of: constructor did not return a TypedArray"),
        )
    }

    fn construct_typed_array_from_this(
        &mut self,
        this_val: &JsValue,
        values: &[JsValue],
    ) -> Completion {
        let len = values.len();
        // Check `this` is callable (constructor)
        if let JsValue::Object(o) = this_val {
            let is_callable = self
                .get_object(o.id)
                .is_some_and(|obj| obj.borrow().callable.is_some());
            if !is_callable {
                return Completion::Throw(
                    self.create_type_error("not a TypedArray constructor"),
                );
            }
        } else {
            return Completion::Throw(self.create_type_error("not a TypedArray constructor"));
        }

        // Use fast path for known built-in TypedArray constructors
        if let JsValue::Object(o) = this_val
            && let Some(obj) = self.get_object(o.id)
        {
            let name = {
                let obj_ref = obj.borrow();
                if let Some(ref func) = obj_ref.callable {
                    match func {
                        JsFunction::Native(n, _, _, _) => Some(n.clone()),
                        JsFunction::User { .. } => None,
                    }
                } else {
                    None
                }
            };
            if let Some(name) = name {
                let kind = match name.as_str() {
                    "Int8Array" => Some(TypedArrayKind::Int8),
                    "Uint8Array" => Some(TypedArrayKind::Uint8),
                    "Uint8ClampedArray" => Some(TypedArrayKind::Uint8Clamped),
                    "Int16Array" => Some(TypedArrayKind::Int16),
                    "Uint16Array" => Some(TypedArrayKind::Uint16),
                    "Int32Array" => Some(TypedArrayKind::Int32),
                    "Uint32Array" => Some(TypedArrayKind::Uint32),
                    "Float32Array" => Some(TypedArrayKind::Float32),
                    "Float64Array" => Some(TypedArrayKind::Float64),
                    "BigInt64Array" => Some(TypedArrayKind::BigInt64),
                    "BigUint64Array" => Some(TypedArrayKind::BigUint64),
                    _ => None,
                };
                if let Some(kind) = kind {
                    let proto = self.get_typed_array_prototype(kind);
                    let bpe = kind.bytes_per_element();
                    let new_buf = vec![0u8; len * bpe];
                    let new_buf_rc = Rc::new(RefCell::new(new_buf));
                    let new_detached = Rc::new(Cell::new(false));
                    let ta = TypedArrayInfo {
                        kind,
                        buffer: new_buf_rc.clone(),
                        byte_offset: 0,
                        byte_length: len * bpe,
                        array_length: len,
                        is_detached: new_detached.clone(),
                        is_length_tracking: false,
                    };
                    for (i, val) in values.iter().enumerate() {
                        let coerced = match self.typed_array_coerce_value(kind, val) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        };
                        typed_array_set_index(&ta, i, &coerced);
                    }
                    let ab_obj = self.create_object();
                    {
                        let mut ab = ab_obj.borrow_mut();
                        ab.class_name = "ArrayBuffer".to_string();
                        ab.prototype = self.realm().arraybuffer_prototype.clone();
                        ab.arraybuffer_data = Some(new_buf_rc);
                        ab.arraybuffer_detached = Some(new_detached);
                    }
                    let ab_id = ab_obj.borrow().id.unwrap();
                    let result = self.create_object();
                    {
                        let mut r = result.borrow_mut();
                        r.class_name = kind.name().to_string();
                        r.prototype = proto;
                        r.view_buffer_object_id = Some(ab_id);
                        r.typed_array_info = Some(ta);
                    }
                    let id = result.borrow().id.unwrap();
                    return Completion::Normal(JsValue::Object(JsObject { id }));
                }
            }
        }

        // Generic path: call this_val as constructor with len, validate result is TypedArray
        let new_obj = match self.construct_with_new_target(
            this_val,
            &[JsValue::Number(len as f64)],
            this_val.clone(),
        ) {
            Completion::Normal(v) => v,
            other => return other,
        };
        // Validate result is a TypedArray
        let ta_kind = if let JsValue::Object(ref o) = new_obj {
            self.get_object(o.id)
                .and_then(|obj| obj.borrow().typed_array_info.as_ref().map(|ta| ta.kind))
        } else {
            None
        };
        let ta_kind = match ta_kind {
            Some(k) => k,
            None => {
                return Completion::Throw(
                    self.create_type_error("TypedArray.of/from: constructor did not return a TypedArray"),
                );
            }
        };
        // Set each element using Set semantics (which respects OOB/detach)
        for (i, val) in values.iter().enumerate() {
            let key = i.to_string();
            let coerced = match self.typed_array_coerce_value(ta_kind, val) {
                Ok(v) => v,
                Err(e) => return Completion::Throw(e),
            };
            if let JsValue::Object(ref o) = new_obj {
                if let Some(obj) = self.get_object(o.id) {
                    obj.borrow_mut().set_property_value(&key, coerced);
                }
            }
        }
        Completion::Normal(new_obj)
    }

    pub(crate) fn collect_iterable_or_arraylike(
        &mut self,
        val: &JsValue,
    ) -> Result<Vec<JsValue>, Completion> {
        if let JsValue::Object(o) = val
            && let Some(obj) = self.get_object(o.id)
        {
            let obj_ref = obj.borrow();
            // Check for Symbol.iterator
            let has_iterator = obj_ref.has_property("Symbol(Symbol.iterator)");
            // Check for array_elements
            if let Some(ref elems) = obj_ref.array_elements {
                return Ok(elems.clone());
            }
            drop(obj_ref);

            if has_iterator {
                // Use iterator protocol
                let iter_fn = match self.get_object_property(o.id, "Symbol(Symbol.iterator)", val) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(Completion::Throw(e)),
                    _ => {
                        return Err(Completion::Throw(self.create_type_error("bad iterator")));
                    }
                };
                let iter = match self.call_function(&iter_fn, val, &[]) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(Completion::Throw(e)),
                    _ => return Err(Completion::Throw(self.create_type_error("bad iterator"))),
                };
                if !matches!(iter, JsValue::Object(_)) {
                    return Err(Completion::Throw(
                        self.create_type_error("Result of the Symbol.iterator method is not an object"),
                    ));
                }
                let mut values = Vec::new();
                while let JsValue::Object(io) = &iter {
                    let next_fn = match self.get_object_property(io.id, "next", &iter) {
                        Completion::Normal(v) => v,
                        _ => break,
                    };
                    let result = match self.call_function(&next_fn, &iter, &[]) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Err(Completion::Throw(e)),
                        _ => break,
                    };
                    if let JsValue::Object(ro) = &result {
                        let done = match self.get_object_property(ro.id, "done", &result) {
                            Completion::Normal(v) => self.to_boolean_val(&v),
                            _ => true,
                        };
                        if done {
                            break;
                        }
                        let value = match self.get_object_property(ro.id, "value", &result) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Err(Completion::Throw(e)),
                            _ => JsValue::Undefined,
                        };
                        values.push(value);
                    } else {
                        break;
                    }
                }
                return Ok(values);
            }

            // Array-like
            let len_val = match self.get_object_property(o.id, "length", val) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(Completion::Throw(e)),
                _ => return Ok(Vec::new()),
            };
            let len = match self.to_number_value(&len_val) {
                Ok(n) => to_integer(n) as usize,
                Err(e) => return Err(Completion::Throw(e)),
            };
            let mut values = Vec::new();
            for i in 0..len {
                let v = match self.get_object_property(o.id, &i.to_string(), val) {
                    Completion::Normal(v) => v,
                    _ => JsValue::Undefined,
                };
                values.push(v);
            }
            return Ok(values);
        }
        Ok(Vec::new())
    }

    fn setup_dataview(&mut self) {
        let dv_proto = self.create_object();
        dv_proto.borrow_mut().class_name = "DataView".to_string();
        self.realm_mut().dataview_prototype = Some(dv_proto.clone());

        // Getters: buffer, byteOffset, byteLength
        let buffer_getter = self.create_function(JsFunction::native(
            "get buffer".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if obj_ref.data_view_info.is_some() {
                        if let Some(buf_id) = obj_ref.view_buffer_object_id {
                            return Completion::Normal(JsValue::Object(JsObject { id: buf_id }));
                        }
                        return Completion::Normal(JsValue::Undefined);
                    }
                }
                Completion::Throw(interp.create_type_error("not a DataView"))
            },
        ));
        dv_proto.borrow_mut().insert_property(
            "buffer".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(buffer_getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );

        // byteOffset getter
        let dv_byte_offset_getter = self.create_function(JsFunction::native(
            "get byteOffset".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if let Some(ref dv) = obj_ref.data_view_info {
                        if dv.is_detached.get() {
                            return Completion::Throw(
                                interp.create_type_error("DataView buffer is detached"),
                            );
                        }
                        let buf_len = dv.buffer.borrow().len();
                        if dv.is_length_tracking {
                            if dv.byte_offset > buf_len {
                                return Completion::Throw(
                                    interp.create_type_error("DataView is out of bounds"),
                                );
                            }
                        } else if dv.byte_offset + dv.byte_length > buf_len {
                            return Completion::Throw(
                                interp.create_type_error("DataView is out of bounds"),
                            );
                        }
                        return Completion::Normal(JsValue::Number(dv.byte_offset as f64));
                    }
                }
                Completion::Throw(interp.create_type_error("not a DataView"))
            },
        ));
        dv_proto.borrow_mut().insert_property(
            "byteOffset".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(dv_byte_offset_getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );
        // byteLength getter
        let dv_byte_length_getter = self.create_function(JsFunction::native(
            "get byteLength".to_string(),
            0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if let Some(ref dv) = obj_ref.data_view_info {
                        if dv.is_detached.get() {
                            return Completion::Throw(
                                interp.create_type_error("DataView buffer is detached"),
                            );
                        }
                        let buf_len = dv.buffer.borrow().len();
                        if dv.is_length_tracking {
                            if dv.byte_offset > buf_len {
                                return Completion::Throw(
                                    interp.create_type_error("DataView is out of bounds"),
                                );
                            }
                            return Completion::Normal(JsValue::Number(
                                (buf_len - dv.byte_offset) as f64,
                            ));
                        } else {
                            if dv.byte_offset + dv.byte_length > buf_len {
                                return Completion::Throw(
                                    interp.create_type_error("DataView is out of bounds"),
                                );
                            }
                            return Completion::Normal(JsValue::Number(dv.byte_length as f64));
                        }
                    }
                }
                Completion::Throw(interp.create_type_error("not a DataView"))
            },
        ));
        dv_proto.borrow_mut().insert_property(
            "byteLength".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(dv_byte_length_getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );

        // DataView get/set methods
        // Spec ordering for GetViewValue:
        // 1. Require this to be a DataView (no detach check yet)
        // 2. ToIndex(byteOffset)
        // 3. Check buffer detached
        // 4. Check bounds
        // 5. Read value
        macro_rules! dv_get_method {
            ($method_name:expr, $size:expr, $read_fn:expr) => {{
                let getter = self.create_function(JsFunction::native(
                    $method_name.to_string(),
                    1,
                    |interp, this_val, args| {
                        if let JsValue::Object(o) = this_val
                            && let Some(obj) = interp.get_object(o.id)
                        {
                            {
                                let obj_ref = obj.borrow();
                                if obj_ref.data_view_info.is_none() {
                                    return Completion::Throw(
                                        interp.create_type_error("not a DataView"),
                                    );
                                }
                            }
                            let byte_offset = match interp
                                .to_index(args.first().unwrap_or(&JsValue::Undefined))
                            {
                                Completion::Normal(JsValue::Number(n)) => n as usize,
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => 0,
                            };
                            let little_endian = if args.len() > 1 {
                                interp.to_boolean_val(&args[1])
                            } else {
                                false
                            };
                            let dv = {
                                let obj_ref = obj.borrow();
                                obj_ref.data_view_info.as_ref().unwrap().clone()
                            };
                            if dv.is_detached.get() {
                                return Completion::Throw(
                                    interp.create_type_error("DataView buffer is detached"),
                                );
                            }
                            let buf_len = dv.buffer.borrow().len();
                            let effective_byte_length = if dv.is_length_tracking {
                                if dv.byte_offset > buf_len {
                                    return Completion::Throw(
                                        interp.create_type_error("DataView is out of bounds"),
                                    );
                                }
                                buf_len - dv.byte_offset
                            } else {
                                if dv.byte_offset + dv.byte_length > buf_len {
                                    return Completion::Throw(
                                        interp.create_type_error("DataView is out of bounds"),
                                    );
                                }
                                dv.byte_length
                            };
                            if byte_offset + $size > effective_byte_length {
                                return Completion::Throw(interp.create_error(
                                    "RangeError",
                                    "offset is outside the bounds of the DataView",
                                ));
                            }
                            let idx = dv.byte_offset + byte_offset;
                            let buf = dv.buffer.borrow();
                            let result = $read_fn(&buf[idx..idx + $size], little_endian);
                            return Completion::Normal(result);
                        }
                        Completion::Throw(interp.create_type_error("not a DataView"))
                    },
                ));
                dv_proto
                    .borrow_mut()
                    .insert_builtin($method_name.to_string(), getter);
            }};
        }

        dv_get_method!("getInt8", 1, |buf: &[u8], _le: bool| -> JsValue {
            JsValue::Number(buf[0] as i8 as f64)
        });
        dv_get_method!("getUint8", 1, |buf: &[u8], _le: bool| -> JsValue {
            JsValue::Number(buf[0] as f64)
        });
        dv_get_method!("getInt16", 2, |buf: &[u8], le: bool| -> JsValue {
            let v = if le {
                i16::from_le_bytes([buf[0], buf[1]])
            } else {
                i16::from_be_bytes([buf[0], buf[1]])
            };
            JsValue::Number(v as f64)
        });
        dv_get_method!("getUint16", 2, |buf: &[u8], le: bool| -> JsValue {
            let v = if le {
                u16::from_le_bytes([buf[0], buf[1]])
            } else {
                u16::from_be_bytes([buf[0], buf[1]])
            };
            JsValue::Number(v as f64)
        });
        dv_get_method!("getInt32", 4, |buf: &[u8], le: bool| -> JsValue {
            let v = if le {
                i32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]])
            } else {
                i32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]])
            };
            JsValue::Number(v as f64)
        });
        dv_get_method!("getUint32", 4, |buf: &[u8], le: bool| -> JsValue {
            let v = if le {
                u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]])
            } else {
                u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]])
            };
            JsValue::Number(v as f64)
        });
        dv_get_method!("getFloat32", 4, |buf: &[u8], le: bool| -> JsValue {
            let v = if le {
                f32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]])
            } else {
                f32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]])
            };
            JsValue::Number(v as f64)
        });
        dv_get_method!("getFloat64", 8, |buf: &[u8], le: bool| -> JsValue {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(buf);
            let v = if le {
                f64::from_le_bytes(bytes)
            } else {
                f64::from_be_bytes(bytes)
            };
            JsValue::Number(v)
        });
        dv_get_method!("getBigInt64", 8, |buf: &[u8], le: bool| -> JsValue {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(buf);
            let v = if le {
                i64::from_le_bytes(bytes)
            } else {
                i64::from_be_bytes(bytes)
            };
            JsValue::BigInt(JsBigInt {
                value: num_bigint::BigInt::from(v),
            })
        });
        dv_get_method!("getBigUint64", 8, |buf: &[u8], le: bool| -> JsValue {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(buf);
            let v = if le {
                u64::from_le_bytes(bytes)
            } else {
                u64::from_be_bytes(bytes)
            };
            JsValue::BigInt(JsBigInt {
                value: num_bigint::BigInt::from(v),
            })
        });
        dv_get_method!("getFloat16", 2, |buf: &[u8], le: bool| -> JsValue {
            let bits = if le {
                u16::from_le_bytes([buf[0], buf[1]])
            } else {
                u16::from_be_bytes([buf[0], buf[1]])
            };
            JsValue::Number(dv_f16_to_f64(bits))
        });

        // DataView set methods
        // Spec ordering for SetViewValue:
        // 1. Require this to be a DataView (no detach check yet)
        // 2. ToIndex(byteOffset)
        // 3. ToNumber(value) or ToBigInt(value)
        // 4. Check buffer detached
        // 5. Check bounds
        // 6. Write value
        macro_rules! dv_set_method {
            ($method_name:expr, $size:expr, number, $write_fn:expr) => {{
                let setter = self.create_function(JsFunction::native(
                    $method_name.to_string(),
                    2,
                    |interp, this_val, args| {
                        // Step 1: Require this to be a DataView (no detach check)
                        if let JsValue::Object(o) = this_val
                            && let Some(obj) = interp.get_object(o.id)
                        {
                            {
                                let obj_ref = obj.borrow();
                                if obj_ref.data_view_info.is_none() {
                                    return Completion::Throw(
                                        interp.create_type_error("not a DataView"),
                                    );
                                }
                            }
                            // Step 2: ToIndex(byteOffset) — before detach check
                            let byte_offset = match interp
                                .to_index(args.first().unwrap_or(&JsValue::Undefined))
                            {
                                Completion::Normal(JsValue::Number(n)) => n as usize,
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => 0,
                            };
                            // Step 3: ToNumber(value) — before detach check
                            let num_value = match interp
                                .to_number_value(args.get(1).unwrap_or(&JsValue::Undefined))
                            {
                                Ok(n) => n,
                                Err(e) => return Completion::Throw(e),
                            };
                            let little_endian = if args.len() > 2 {
                                interp.to_boolean_val(&args[2])
                            } else {
                                false
                            };
                            // Step 4-6: Re-borrow, check detach, check bounds, write
                            let dv = {
                                let obj_ref = obj.borrow();
                                obj_ref.data_view_info.as_ref().unwrap().clone()
                            };
                            if dv.is_detached.get() {
                                return Completion::Throw(
                                    interp.create_type_error("DataView buffer is detached"),
                                );
                            }
                            let buf_len = dv.buffer.borrow().len();
                            let effective_byte_length = if dv.is_length_tracking {
                                if dv.byte_offset > buf_len {
                                    return Completion::Throw(
                                        interp.create_type_error("DataView is out of bounds"),
                                    );
                                }
                                buf_len - dv.byte_offset
                            } else {
                                if dv.byte_offset + dv.byte_length > buf_len {
                                    return Completion::Throw(
                                        interp.create_type_error("DataView is out of bounds"),
                                    );
                                }
                                dv.byte_length
                            };
                            if byte_offset + $size > effective_byte_length {
                                return Completion::Throw(interp.create_error(
                                    "RangeError",
                                    "offset is outside the bounds of the DataView",
                                ));
                            }
                            let idx = dv.byte_offset + byte_offset;
                            let mut buf = dv.buffer.borrow_mut();
                            $write_fn(&mut buf[idx..idx + $size], num_value, little_endian);
                            return Completion::Normal(JsValue::Undefined);
                        }
                        Completion::Throw(interp.create_type_error("not a DataView"))
                    },
                ));
                dv_proto
                    .borrow_mut()
                    .insert_builtin($method_name.to_string(), setter);
            }};
            ($method_name:expr, $size:expr, bigint, $write_fn:expr) => {{
                let setter = self.create_function(JsFunction::native(
                    $method_name.to_string(),
                    2,
                    |interp, this_val, args| {
                        // Step 1: Require this to be a DataView (no detach check)
                        if let JsValue::Object(o) = this_val
                            && let Some(obj) = interp.get_object(o.id)
                        {
                            {
                                let obj_ref = obj.borrow();
                                if obj_ref.data_view_info.is_none() {
                                    return Completion::Throw(
                                        interp.create_type_error("not a DataView"),
                                    );
                                }
                            }
                            // Step 2: ToIndex(byteOffset) — before detach check
                            let byte_offset = match interp
                                .to_index(args.first().unwrap_or(&JsValue::Undefined))
                            {
                                Completion::Normal(JsValue::Number(n)) => n as usize,
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => 0,
                            };
                            // Step 3: ToBigInt(value) — before detach check
                            let bigint_value = match interp
                                .to_bigint_value(args.get(1).unwrap_or(&JsValue::Undefined))
                            {
                                Ok(v) => v,
                                Err(e) => return Completion::Throw(e),
                            };
                            let little_endian = if args.len() > 2 {
                                interp.to_boolean_val(&args[2])
                            } else {
                                false
                            };
                            // Step 4-6: Re-borrow, check detach, check bounds, write
                            let dv = {
                                let obj_ref = obj.borrow();
                                obj_ref.data_view_info.as_ref().unwrap().clone()
                            };
                            if dv.is_detached.get() {
                                return Completion::Throw(
                                    interp.create_type_error("DataView buffer is detached"),
                                );
                            }
                            let buf_len = dv.buffer.borrow().len();
                            let effective_byte_length = if dv.is_length_tracking {
                                if dv.byte_offset > buf_len {
                                    return Completion::Throw(
                                        interp.create_type_error("DataView is out of bounds"),
                                    );
                                }
                                buf_len - dv.byte_offset
                            } else {
                                if dv.byte_offset + dv.byte_length > buf_len {
                                    return Completion::Throw(
                                        interp.create_type_error("DataView is out of bounds"),
                                    );
                                }
                                dv.byte_length
                            };
                            if byte_offset + $size > effective_byte_length {
                                return Completion::Throw(interp.create_error(
                                    "RangeError",
                                    "offset is outside the bounds of the DataView",
                                ));
                            }
                            let idx = dv.byte_offset + byte_offset;
                            let mut buf = dv.buffer.borrow_mut();
                            $write_fn(&mut buf[idx..idx + $size], &bigint_value, little_endian);
                            return Completion::Normal(JsValue::Undefined);
                        }
                        Completion::Throw(interp.create_type_error("not a DataView"))
                    },
                ));
                dv_proto
                    .borrow_mut()
                    .insert_builtin($method_name.to_string(), setter);
            }};
        }

        dv_set_method!("setInt8", 1, number, |buf: &mut [u8], n: f64, _le: bool| {
            buf[0] = to_int32_modular(n) as i8 as u8;
        });
        dv_set_method!(
            "setUint8",
            1,
            number,
            |buf: &mut [u8], n: f64, _le: bool| {
                buf[0] = to_int32_modular(n) as u8;
            }
        );
        dv_set_method!("setInt16", 2, number, |buf: &mut [u8], n: f64, le: bool| {
            let v = to_int32_modular(n) as i16;
            let bytes = if le { v.to_le_bytes() } else { v.to_be_bytes() };
            buf.copy_from_slice(&bytes);
        });
        dv_set_method!(
            "setUint16",
            2,
            number,
            |buf: &mut [u8], n: f64, le: bool| {
                let v = to_int32_modular(n) as u16;
                let bytes = if le { v.to_le_bytes() } else { v.to_be_bytes() };
                buf.copy_from_slice(&bytes);
            }
        );
        dv_set_method!("setInt32", 4, number, |buf: &mut [u8], n: f64, le: bool| {
            let v = to_int32_modular(n);
            let bytes = if le { v.to_le_bytes() } else { v.to_be_bytes() };
            buf.copy_from_slice(&bytes);
        });
        dv_set_method!(
            "setUint32",
            4,
            number,
            |buf: &mut [u8], n: f64, le: bool| {
                let v = to_int32_modular(n) as u32;
                let bytes = if le { v.to_le_bytes() } else { v.to_be_bytes() };
                buf.copy_from_slice(&bytes);
            }
        );
        dv_set_method!(
            "setFloat32",
            4,
            number,
            |buf: &mut [u8], n: f64, le: bool| {
                let v = n as f32;
                let bytes = if le { v.to_le_bytes() } else { v.to_be_bytes() };
                buf.copy_from_slice(&bytes);
            }
        );
        dv_set_method!(
            "setFloat64",
            8,
            number,
            |buf: &mut [u8], n: f64, le: bool| {
                let bytes = if le { n.to_le_bytes() } else { n.to_be_bytes() };
                buf.copy_from_slice(&bytes);
            }
        );
        dv_set_method!(
            "setFloat16",
            2,
            number,
            |buf: &mut [u8], n: f64, le: bool| {
                let bits = dv_f64_to_f16_bits(n);
                let bytes = if le {
                    bits.to_le_bytes()
                } else {
                    bits.to_be_bytes()
                };
                buf.copy_from_slice(&bytes);
            }
        );
        dv_set_method!(
            "setBigInt64",
            8,
            bigint,
            |buf: &mut [u8], v: &JsValue, le: bool| {
                let n = match v {
                    JsValue::BigInt(b) => i64::try_from(&b.value).unwrap_or(0),
                    _ => 0,
                };
                let bytes = if le { n.to_le_bytes() } else { n.to_be_bytes() };
                buf.copy_from_slice(&bytes);
            }
        );
        dv_set_method!(
            "setBigUint64",
            8,
            bigint,
            |buf: &mut [u8], v: &JsValue, le: bool| {
                let n = match v {
                    JsValue::BigInt(b) => u64::try_from(&b.value).unwrap_or(0),
                    _ => 0,
                };
                let bytes = if le { n.to_le_bytes() } else { n.to_be_bytes() };
                buf.copy_from_slice(&bytes);
            }
        );

        // @@toStringTag
        let tag = JsValue::String(JsString::from_str("DataView"));
        dv_proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor::data(tag, false, false, true),
        );

        // DataView constructor
        let dv_proto_clone = dv_proto.clone();
        let ctor = self.create_function(JsFunction::constructor(
            "DataView".to_string(),
            1,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("Constructor DataView requires 'new'"),
                    );
                }
                let buf_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = &buf_arg
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let is_arraybuffer = {
                        let obj_ref = obj.borrow();
                        obj_ref.arraybuffer_data.is_some()
                    };
                    if !is_arraybuffer {
                        return Completion::Throw(interp.create_type_error(
                            "First argument to DataView constructor must be an ArrayBuffer",
                        ));
                    }
                    // ToIndex(byteOffset) BEFORE detach check
                    let byte_offset =
                        match interp.to_index(args.get(1).unwrap_or(&JsValue::Undefined)) {
                            Completion::Normal(JsValue::Number(n)) => n as usize,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => 0,
                        };
                    let (buf_rc, detached_flag, buf_len, is_resizable) = {
                        let obj_ref = obj.borrow();
                        if let Some(ref det) = obj_ref.arraybuffer_detached
                            && det.get()
                        {
                            return Completion::Throw(interp.create_type_error(
                                "Cannot construct DataView from detached ArrayBuffer",
                            ));
                        }
                        let buf = obj_ref.arraybuffer_data.as_ref().unwrap().clone();
                        let det = obj_ref
                            .arraybuffer_detached
                            .clone()
                            .unwrap_or_else(|| Rc::new(Cell::new(false)));
                        let len = buf.borrow().len();
                        let resizable = obj_ref.arraybuffer_max_byte_length.is_some();
                        (buf, det, len, resizable)
                    };
                    if byte_offset > buf_len {
                        return Completion::Throw(interp.create_error(
                            "RangeError",
                            "offset is outside the bounds of the buffer",
                        ));
                    }
                    let byte_length_arg = args.get(2).unwrap_or(&JsValue::Undefined);
                    let has_byte_length = !matches!(byte_length_arg, JsValue::Undefined);
                    let is_length_tracking = is_resizable && !has_byte_length;
                    let byte_length = if !has_byte_length {
                        buf_len - byte_offset
                    } else {
                        match interp.to_index(byte_length_arg) {
                            Completion::Normal(JsValue::Number(n)) => {
                                let bl = n as usize;
                                if byte_offset + bl > buf_len {
                                    return Completion::Throw(
                                        interp
                                            .create_error("RangeError", "invalid DataView length"),
                                    );
                                }
                                bl
                            }
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => 0,
                        }
                    };
                    let dv_info = DataViewInfo {
                        buffer: buf_rc,
                        byte_offset,
                        byte_length,
                        is_detached: detached_flag,
                        is_length_tracking,
                    };
                    let result = interp.create_object();
                    {
                        let mut r = result.borrow_mut();
                        r.class_name = "DataView".to_string();
                        r.prototype = Some(dv_proto_clone.clone());
                        if let JsValue::Object(ref bobj) = buf_arg {
                            r.view_buffer_object_id = Some(bobj.id);
                        }
                        r.data_view_info = Some(dv_info);
                    }
                    let id = result.borrow().id.unwrap();
                    return Completion::Normal(JsValue::Object(JsObject { id }));
                }
                Completion::Throw(interp.create_type_error(
                    "First argument to DataView constructor must be an ArrayBuffer",
                ))
            },
        ));

        // Wire DataView.prototype to the proto object with all the methods
        let dv_proto_val = {
            let id = dv_proto.borrow().id.unwrap();
            JsValue::Object(crate::types::JsObject { id })
        };
        if let JsValue::Object(o) = &ctor
            && let Some(obj) = self.get_object(o.id)
        {
            obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(dv_proto_val, false, false, false),
            );
        }
        dv_proto.borrow_mut().insert_property(
            "constructor".to_string(),
            PropertyDescriptor::data(ctor.clone(), true, false, true),
        );

        self.realm().global_env
            .borrow_mut()
            .declare("DataView", BindingKind::Var);
        let _ = self.realm().global_env.borrow_mut().set("DataView", ctor);
    }
}

fn extract_ta_and_callback(
    interp: &mut Interpreter,
    this_val: &JsValue,
    args: &[JsValue],
) -> Result<(TypedArrayInfo, JsValue), Completion> {
    if let JsValue::Object(o) = this_val
        && let Some(obj) = interp.get_object(o.id)
    {
        let ta = {
            let obj_ref = obj.borrow();
            if let Some(ref ta) = obj_ref.typed_array_info {
                if ta.is_detached.get() || is_typed_array_out_of_bounds(ta) {
                    return Err(Completion::Throw(
                        interp.create_type_error("typed array is detached"),
                    ));
                }
                ta.clone()
            } else {
                return Err(Completion::Throw(
                    interp.create_type_error("not a TypedArray"),
                ));
            }
        };
        let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
        let is_callable = if let JsValue::Object(co) = &callback {
            interp
                .get_object(co.id)
                .is_some_and(|obj| obj.borrow().callable.is_some())
        } else {
            false
        };
        if !is_callable {
            return Err(Completion::Throw(
                interp.create_type_error("callback is not a function"),
            ));
        }
        return Ok((ta, callback));
    }
    Err(Completion::Throw(
        interp.create_type_error("not a TypedArray"),
    ))
}

/// Convert IEEE 754 binary16 (half-precision) bits to f64.
fn dv_f16_to_f64(bits: u16) -> f64 {
    let sign = ((bits >> 15) & 1) as u64;
    let exp = ((bits >> 10) & 0x1F) as u64;
    let frac = (bits & 0x3FF) as u64;

    if exp == 0 {
        if frac == 0 {
            return f64::from_bits(sign << 63);
        }
        let mut shifts = 0_i32;
        let mut f = frac;
        while f & 0x400 == 0 {
            f <<= 1;
            shifts += 1;
        }
        let f64_exp = (1023 - 14 - shifts) as u64;
        let f64_frac = (f & 0x3FF) << 42;
        return f64::from_bits((sign << 63) | (f64_exp << 52) | f64_frac);
    }

    if exp == 31 {
        if frac == 0 {
            return f64::from_bits((sign << 63) | 0x7FF0_0000_0000_0000);
        }
        return f64::from_bits((sign << 63) | 0x7FF8_0000_0000_0000 | (frac << 42));
    }

    let f64_exp = (exp as i32 - 15 + 1023) as u64;
    let f64_frac = frac << 42;
    f64::from_bits((sign << 63) | (f64_exp << 52) | f64_frac)
}

/// Convert f64 to IEEE 754 binary16 (half-precision) bits.
/// Uses round-to-nearest-even (banker's rounding).
fn dv_f64_to_f16_bits(val: f64) -> u16 {
    if val.is_nan() {
        return 0x7E00; // NaN
    }
    if !val.is_finite() {
        return if val > 0.0 { 0x7C00 } else { 0xFC00 };
    }
    if val == 0.0 {
        return if val.is_sign_negative() {
            0x8000
        } else {
            0x0000
        };
    }

    let bits = val.to_bits();
    let sign = ((bits >> 63) as u16) << 15;
    let exp = ((bits >> 52) & 0x7FF) as i32;
    let frac = bits & 0x000F_FFFF_FFFF_FFFF;
    let unbiased = exp - 1023;

    if unbiased > 15 {
        return sign | 0x7C00; // Infinity
    }

    if unbiased >= -14 {
        // Normal f16
        let f16_exp = ((unbiased + 15) as u16) << 10;
        let mantissa_10 = (frac >> 42) as u16;
        let round_bits = frac & 0x3FF_FFFF_FFFF;
        let halfway = 0x200_0000_0000_u64;

        let rounded = if round_bits > halfway {
            mantissa_10 + 1
        } else if round_bits == halfway {
            if mantissa_10 & 1 != 0 {
                mantissa_10 + 1
            } else {
                mantissa_10
            }
        } else {
            mantissa_10
        };

        let result = sign | f16_exp | (rounded & 0x3FF);
        return if rounded > 0x3FF {
            result + (1 << 10)
        } else {
            result
        };
    }

    // Subnormal f16
    let shift = (-14 - unbiased) as u64;
    let full = (1_u64 << 52) | frac;
    let total_shift = 42 + shift;

    if total_shift >= 53 {
        if total_shift == 53 {
            if frac > 0 {
                return sign | 1;
            }
            return sign;
        }
        return sign;
    }

    let mantissa = ((full >> total_shift) & 0x3FF) as u16;
    let round_bit_pos = total_shift - 1;
    let round_bit = (full >> round_bit_pos) & 1;
    let sticky = if round_bit_pos > 0 {
        full & ((1_u64 << round_bit_pos) - 1)
    } else {
        0
    };
    let rounded = if round_bit == 1 {
        if sticky > 0 || (mantissa & 1 != 0) {
            mantissa + 1
        } else {
            mantissa
        }
    } else {
        mantissa
    };

    if rounded >= 0x400 {
        return sign | (1 << 10);
    }
    sign | rounded
}

fn to_integer(n: f64) -> f64 {
    if n.is_nan() {
        return 0.0;
    }
    if n == 0.0 || n.is_infinite() {
        return n;
    }
    n.signum() * n.abs().floor()
}

fn is_bigint_kind(kind: TypedArrayKind) -> bool {
    matches!(kind, TypedArrayKind::BigInt64 | TypedArrayKind::BigUint64)
}

fn same_value_zero(x: &JsValue, y: &JsValue) -> bool {
    match (x, y) {
        (JsValue::Number(a), JsValue::Number(b)) => {
            if a.is_nan() && b.is_nan() {
                return true;
            }
            if *a == 0.0 && *b == 0.0 {
                return true;
            }
            a == b
        }
        _ => strict_eq(x, y),
    }
}

fn strict_eq(x: &JsValue, y: &JsValue) -> bool {
    match (x, y) {
        (JsValue::Undefined, JsValue::Undefined) | (JsValue::Null, JsValue::Null) => true,
        (JsValue::Boolean(a), JsValue::Boolean(b)) => a == b,
        (JsValue::Number(a), JsValue::Number(b)) => a == b,
        (JsValue::String(a), JsValue::String(b)) => a.to_rust_string() == b.to_rust_string(),
        (JsValue::BigInt(a), JsValue::BigInt(b)) => a.value == b.value,
        (JsValue::Object(a), JsValue::Object(b)) => a.id == b.id,
        _ => false,
    }
}

fn validate_uint8array(
    interp: &mut Interpreter,
    this_val: &JsValue,
) -> Result<TypedArrayInfo, Completion> {
    if let JsValue::Object(o) = this_val
        && let Some(obj) = interp.get_object(o.id)
    {
        let obj_ref = obj.borrow();
        if let Some(ref ta) = obj_ref.typed_array_info {
            if !matches!(ta.kind, TypedArrayKind::Uint8) {
                return Err(Completion::Throw(
                    interp.create_type_error("not a Uint8Array"),
                ));
            }
            if ta.is_detached.get() {
                return Err(Completion::Throw(
                    interp.create_type_error("typed array is detached"),
                ));
            }
            return Ok(ta.clone());
        }
    }
    Err(Completion::Throw(
        interp.create_type_error("not a Uint8Array"),
    ))
}

fn validate_uint8array_no_detach_check(
    interp: &mut Interpreter,
    this_val: &JsValue,
) -> Result<TypedArrayInfo, Completion> {
    if let JsValue::Object(o) = this_val
        && let Some(obj) = interp.get_object(o.id)
    {
        let obj_ref = obj.borrow();
        if let Some(ref ta) = obj_ref.typed_array_info {
            if !matches!(ta.kind, TypedArrayKind::Uint8) {
                return Err(Completion::Throw(
                    interp.create_type_error("not a Uint8Array"),
                ));
            }
            return Ok(ta.clone());
        }
    }
    Err(Completion::Throw(
        interp.create_type_error("not a Uint8Array"),
    ))
}

fn check_detached(interp: &mut Interpreter, ta: &TypedArrayInfo) -> Result<(), Completion> {
    if ta.is_detached.get() {
        Err(Completion::Throw(
            interp.create_type_error("typed array is detached"),
        ))
    } else {
        Ok(())
    }
}

fn parse_base64_options(
    interp: &mut Interpreter,
    opts: &JsValue,
) -> Result<(String, String), Completion> {
    let mut alphabet = "base64".to_string();
    let mut last_chunk = "loose".to_string();

    if !matches!(opts, JsValue::Undefined | JsValue::Null)
        && let JsValue::Object(o) = opts
    {
        let alpha_val = match interp.get_object_property(o.id, "alphabet", opts) {
            Completion::Normal(v) => v,
            other => return Err(other),
        };
        if !matches!(alpha_val, JsValue::Undefined) {
            if !matches!(alpha_val, JsValue::String(_)) {
                return Err(Completion::Throw(
                    interp.create_type_error("alphabet must be a string"),
                ));
            }
            let s = to_js_string(&alpha_val);
            if s != "base64" && s != "base64url" {
                return Err(Completion::Throw(interp.create_type_error(
                    "expected alphabet to be either \"base64\" or \"base64url\"",
                )));
            }
            alphabet = s;
        }

        let lch_val = match interp.get_object_property(o.id, "lastChunkHandling", opts) {
            Completion::Normal(v) => v,
            other => return Err(other),
        };
        if !matches!(lch_val, JsValue::Undefined) {
            if !matches!(lch_val, JsValue::String(_)) {
                return Err(Completion::Throw(
                    interp.create_type_error("lastChunkHandling must be a string"),
                ));
            }
            let s = to_js_string(&lch_val);
            if s != "loose" && s != "strict" && s != "stop-before-partial" {
                return Err(Completion::Throw(
                        interp.create_type_error("expected lastChunkHandling to be either \"loose\", \"strict\", or \"stop-before-partial\""),
                    ));
            }
            last_chunk = s;
        }
    }
    Ok((alphabet, last_chunk))
}

fn parse_to_base64_options(
    interp: &mut Interpreter,
    opts: &JsValue,
) -> Result<(String, bool), Completion> {
    let mut alphabet = "base64".to_string();
    let mut omit_padding = false;

    if !matches!(opts, JsValue::Undefined | JsValue::Null)
        && let JsValue::Object(o) = opts
    {
        let alpha_val = match interp.get_object_property(o.id, "alphabet", opts) {
            Completion::Normal(v) => v,
            other => return Err(other),
        };
        if !matches!(alpha_val, JsValue::Undefined) {
            if !matches!(alpha_val, JsValue::String(_)) {
                return Err(Completion::Throw(
                    interp.create_type_error("alphabet must be a string"),
                ));
            }
            let s = to_js_string(&alpha_val);
            if s != "base64" && s != "base64url" {
                return Err(Completion::Throw(interp.create_type_error(
                    "expected alphabet to be either \"base64\" or \"base64url\"",
                )));
            }
            alphabet = s;
        }

        let omit_val = match interp.get_object_property(o.id, "omitPadding", opts) {
            Completion::Normal(v) => v,
            other => return Err(other),
        };
        omit_padding = interp.to_boolean_val(&omit_val);
    }
    Ok((alphabet, omit_padding))
}

fn base64_char_value(c: char, alphabet: &str) -> Option<u8> {
    match c {
        'A'..='Z' => Some(c as u8 - b'A'),
        'a'..='z' => Some(c as u8 - b'a' + 26),
        '0'..='9' => Some(c as u8 - b'0' + 52),
        '+' if alphabet == "base64" => Some(62),
        '/' if alphabet == "base64" => Some(63),
        '-' if alphabet == "base64url" => Some(62),
        '_' if alphabet == "base64url" => Some(63),
        _ => None,
    }
}

fn is_base64_whitespace(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\x0C' | '\r' | ' ')
}

// Decode base64 with full spec compliance.
// Returns (decoded_bytes, chars_read_from_original_input)
// max_bytes: if Some, stop decoding when output would exceed this many bytes
// Result of FromBase64/FromHex spec algorithm
struct DecodeResult {
    bytes: Vec<u8>,
    read: usize, // chars read from original input
    error: Option<String>,
}

fn decode_base64(
    input: &str,
    alphabet: &str,
    last_chunk_handling: &str,
    max_bytes: Option<usize>,
) -> DecodeResult {
    let max = max_bytes.unwrap_or(usize::MAX);
    if max == 0 {
        return DecodeResult {
            bytes: Vec::new(),
            read: 0,
            error: None,
        };
    }

    let chars: Vec<char> = input.chars().collect();

    // Strip whitespace, tracking original positions (byte position in original string)
    let mut cleaned: Vec<(char, usize)> = Vec::new();
    for (i, &c) in chars.iter().enumerate() {
        if is_base64_whitespace(c) {
            continue;
        }
        cleaned.push((c, i));
    }

    let mut output = Vec::new();
    let mut i = 0; // index into cleaned

    while i < cleaned.len() {
        let chunk_start = i;
        let _chunk_start_output_len = output.len();

        // Collect data chars for this chunk
        let mut chunk: Vec<u8> = Vec::new();
        let mut padding_count = 0;
        let mut saw_padding = false;

        while i < cleaned.len() && chunk.len() + padding_count < 4 {
            let (c, _) = cleaned[i];
            if c == '=' {
                saw_padding = true;
                padding_count += 1;
                i += 1;
            } else if saw_padding {
                // data char after padding within a 4-char group: error
                break;
            } else if let Some(val) = base64_char_value(c, alphabet) {
                chunk.push(val);
                i += 1;
            } else {
                // invalid character
                let read = if chunk_start > 0 {
                    cleaned[chunk_start - 1].1 + 1
                } else {
                    0
                };
                return DecodeResult {
                    bytes: output,
                    read,
                    error: Some(format!("invalid character {}", c)),
                };
            }
        }

        let group_len = chunk.len() + padding_count;
        if group_len == 0 {
            break;
        }

        // Complete 4-char group (no padding)
        if chunk.len() == 4 && padding_count == 0 {
            if output.len() + 3 > max {
                i = chunk_start;
                break;
            }
            let b0 = (chunk[0] << 2) | (chunk[1] >> 4);
            let b1 = ((chunk[1] & 0x0F) << 4) | (chunk[2] >> 2);
            let b2 = ((chunk[2] & 0x03) << 6) | chunk[3];
            output.push(b0);
            output.push(b1);
            output.push(b2);
            // If we've reached maxLength, stop immediately
            if output.len() >= max {
                break;
            }
            continue;
        }

        // Padded 4-char group: 2 data + 2 pad
        if chunk.len() == 2 && padding_count == 2 && group_len == 4 {
            // Check for excess data/padding after this padded chunk BEFORE decoding
            if i < cleaned.len() {
                let read = if chunk_start > 0 {
                    cleaned[chunk_start - 1].1 + 1
                } else {
                    0
                };
                return DecodeResult {
                    bytes: output,
                    read,
                    error: Some("unexpected data after padding".to_string()),
                };
            }
            if output.len() + 1 > max {
                i = chunk_start;
                break;
            }
            if last_chunk_handling == "strict" && (chunk[1] & 0x0F) != 0 {
                let read = if chunk_start > 0 {
                    cleaned[chunk_start - 1].1 + 1
                } else {
                    0
                };
                return DecodeResult {
                    bytes: output,
                    read,
                    error: Some("non-zero padding bits".to_string()),
                };
            }
            let b0 = (chunk[0] << 2) | (chunk[1] >> 4);
            output.push(b0);
            if output.len() >= max {
                break;
            }
            continue;
        }

        // Padded 4-char group: 3 data + 1 pad
        if chunk.len() == 3 && padding_count == 1 && group_len == 4 {
            // Check for excess data after padded chunk BEFORE decoding
            if i < cleaned.len() {
                let read = if chunk_start > 0 {
                    cleaned[chunk_start - 1].1 + 1
                } else {
                    0
                };
                return DecodeResult {
                    bytes: output,
                    read,
                    error: Some("unexpected data after padding".to_string()),
                };
            }
            if output.len() + 2 > max {
                i = chunk_start;
                break;
            }
            if last_chunk_handling == "strict" && (chunk[2] & 0x03) != 0 {
                let read = if chunk_start > 0 {
                    cleaned[chunk_start - 1].1 + 1
                } else {
                    0
                };
                return DecodeResult {
                    bytes: output,
                    read,
                    error: Some("non-zero padding bits".to_string()),
                };
            }
            let b0 = (chunk[0] << 2) | (chunk[1] >> 4);
            let b1 = ((chunk[1] & 0x0F) << 4) | (chunk[2] >> 2);
            output.push(b0);
            output.push(b1);
            if output.len() >= max {
                break;
            }
            continue;
        }

        // Incomplete group (less than 4 chars total, end of input)
        // This includes: partial padding cases like "Zg=" (2 data + 1 pad = 3)
        // and unpadded partials like "Zg" (2 data, 0 pad = 2)

        if saw_padding {
            // Incomplete group with padding didn't complete to 4 chars.
            // Only allow stop-before-partial backup for 2+ data chars
            // (incomplete but potentially valid padding like "AA=").
            // With 0 or 1 data chars, padding is always invalid.
            if chunk.len() >= 2 && last_chunk_handling == "stop-before-partial" {
                i = chunk_start;
                break;
            }

            let read = if chunk_start > 0 {
                cleaned[chunk_start - 1].1 + 1
            } else {
                0
            };
            return DecodeResult {
                bytes: output,
                read,
                error: Some("invalid padding".to_string()),
            };
        }

        // Unpadded partial chunk at end: 1, 2, or 3 data chars
        if chunk.len() == 1 {
            if last_chunk_handling == "stop-before-partial" {
                i = chunk_start;
                break;
            }
            let read = if chunk_start > 0 {
                cleaned[chunk_start - 1].1 + 1
            } else {
                0
            };
            return DecodeResult {
                bytes: output,
                read,
                error: Some("incomplete chunk".to_string()),
            };
        }

        if chunk.len() == 2 {
            if last_chunk_handling == "stop-before-partial" {
                i = chunk_start;
                break;
            }
            if last_chunk_handling == "strict" {
                let read = if chunk_start > 0 {
                    cleaned[chunk_start - 1].1 + 1
                } else {
                    0
                };
                return DecodeResult {
                    bytes: output,
                    read,
                    error: Some("missing padding".to_string()),
                };
            }
            // loose: decode
            if output.len() + 1 > max {
                i = chunk_start;
                break;
            }
            let b0 = (chunk[0] << 2) | (chunk[1] >> 4);
            output.push(b0);
            continue;
        }

        if chunk.len() == 3 {
            if last_chunk_handling == "stop-before-partial" {
                i = chunk_start;
                break;
            }
            if last_chunk_handling == "strict" {
                let read = if chunk_start > 0 {
                    cleaned[chunk_start - 1].1 + 1
                } else {
                    0
                };
                return DecodeResult {
                    bytes: output,
                    read,
                    error: Some("missing padding".to_string()),
                };
            }
            // loose: decode
            if output.len() + 2 > max {
                i = chunk_start;
                break;
            }
            let b0 = (chunk[0] << 2) | (chunk[1] >> 4);
            let b1 = ((chunk[1] & 0x0F) << 4) | (chunk[2] >> 2);
            output.push(b0);
            output.push(b1);
            continue;
        }
    }

    // Calculate chars read from original input
    let chars_read = if i > 0 && i <= cleaned.len() {
        cleaned[i - 1].1 + 1
    } else if i == 0 {
        0
    } else {
        chars.len()
    };

    DecodeResult {
        bytes: output,
        read: chars_read,
        error: None,
    }
}

fn decode_hex(input: &str, max_bytes: Option<usize>) -> DecodeResult {
    let chars: Vec<char> = input.chars().collect();
    let max = max_bytes.unwrap_or(usize::MAX);

    // Check odd length first (before maxLength check per spec)
    if !chars.len().is_multiple_of(2) {
        return DecodeResult {
            bytes: Vec::new(),
            read: 0,
            error: Some("hex string must have an even number of characters".to_string()),
        };
    }

    let mut output = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        if output.len() >= max {
            break;
        }
        let hi = match hex_digit(chars[i]) {
            Some(v) => v,
            None => {
                let read = output.len() * 2;
                return DecodeResult {
                    bytes: output,
                    read,
                    error: Some(format!("invalid hex character: {}", chars[i])),
                };
            }
        };
        let lo = match hex_digit(chars[i + 1]) {
            Some(v) => v,
            None => {
                let read = output.len() * 2;
                return DecodeResult {
                    bytes: output,
                    read,
                    error: Some(format!("invalid hex character: {}", chars[i + 1])),
                };
            }
        };
        output.push((hi << 4) | lo);
        i += 2;
    }

    let read = output.len() * 2;
    DecodeResult {
        bytes: output,
        read,
        error: None,
    }
}

fn hex_digit(c: char) -> Option<u8> {
    match c {
        '0'..='9' => Some(c as u8 - b'0'),
        'a'..='f' => Some(c as u8 - b'a' + 10),
        'A'..='F' => Some(c as u8 - b'A' + 10),
        _ => None,
    }
}

const BASE64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const BASE64URL_CHARS: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn encode_base64(data: &[u8], alphabet: &str, omit_padding: bool) -> String {
    let table = if alphabet == "base64url" {
        BASE64URL_CHARS
    } else {
        BASE64_CHARS
    };

    let mut result = String::new();
    let chunks = data.chunks(3);

    for chunk in chunks {
        match chunk.len() {
            3 => {
                let b0 = chunk[0];
                let b1 = chunk[1];
                let b2 = chunk[2];
                result.push(table[(b0 >> 2) as usize] as char);
                result.push(table[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
                result.push(table[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
                result.push(table[(b2 & 0x3F) as usize] as char);
            }
            2 => {
                let b0 = chunk[0];
                let b1 = chunk[1];
                result.push(table[(b0 >> 2) as usize] as char);
                result.push(table[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
                result.push(table[((b1 & 0x0F) << 2) as usize] as char);
                if !omit_padding {
                    result.push('=');
                }
            }
            1 => {
                let b0 = chunk[0];
                result.push(table[(b0 >> 2) as usize] as char);
                result.push(table[((b0 & 0x03) << 4) as usize] as char);
                if !omit_padding {
                    result.push('=');
                    result.push('=');
                }
            }
            _ => {}
        }
    }
    result
}

fn create_uint8array_from_bytes(interp: &mut Interpreter, bytes: &[u8]) -> Completion {
    let len = bytes.len();
    let buf = bytes.to_vec();
    let buf_rc = Rc::new(RefCell::new(buf));
    let detached = Rc::new(Cell::new(false));
    let ta_info = TypedArrayInfo {
        kind: TypedArrayKind::Uint8,
        buffer: buf_rc.clone(),
        byte_offset: 0,
        byte_length: len,
        array_length: len,
        is_detached: detached.clone(),
        is_length_tracking: false,
    };
    let ab_obj = interp.create_object();
    {
        let mut ab = ab_obj.borrow_mut();
        ab.class_name = "ArrayBuffer".to_string();
        ab.prototype = interp.realm().arraybuffer_prototype.clone();
        ab.arraybuffer_data = Some(buf_rc);
        ab.arraybuffer_detached = Some(detached);
    }
    let ab_id = ab_obj.borrow().id.unwrap();
    let buf_val = JsValue::Object(JsObject { id: ab_id });

    let proto = interp.realm().uint8array_prototype.clone().unwrap();
    let result = interp.create_typed_array_object_with_proto(ta_info, buf_val, &proto);
    let id = result.borrow().id.unwrap();
    Completion::Normal(JsValue::Object(JsObject { id }))
}

fn make_read_written_result(interp: &mut Interpreter, read: usize, written: usize) -> Completion {
    let obj = interp.create_object();
    obj.borrow_mut()
        .set_property_value("read", JsValue::Number(read as f64));
    obj.borrow_mut()
        .set_property_value("written", JsValue::Number(written as f64));
    let id = obj.borrow().id.unwrap();
    Completion::Normal(JsValue::Object(JsObject { id }))
}
