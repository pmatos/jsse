use super::super::*;
use crate::types::{JsBigInt, JsObject, JsString, JsValue};
use std::cell::RefCell;
use std::rc::Rc;

impl Interpreter {
    pub(crate) fn setup_typedarray_builtins(&mut self) {
        self.setup_arraybuffer();
        self.setup_typed_array_base_prototype();
        self.setup_typed_array_constructors();
        self.setup_dataview();
    }

    fn setup_arraybuffer(&mut self) {
        let ab_proto = self.create_object();
        ab_proto.borrow_mut().class_name = "ArrayBuffer".to_string();
        self.arraybuffer_prototype = Some(ab_proto.clone());

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
                        if let Some(ref buf) = obj_ref.arraybuffer_data {
                            let b = buf.borrow();
                            (b.clone(), b.len())
                        } else {
                            return Completion::Throw(interp.create_type_error("not an ArrayBuffer"));
                        }
                    };
                    let len = buf_len as f64;
                    let start_arg = args.first().map(|v| to_number(v)).unwrap_or(0.0);
                    let start = if start_arg < 0.0 {
                        ((len + start_arg) as isize).max(0) as usize
                    } else {
                        (start_arg as usize).min(buf_len)
                    };
                    let end_arg = if args.len() > 1 && !matches!(args[1], JsValue::Undefined) {
                        to_number(&args[1])
                    } else {
                        len
                    };
                    let end = if end_arg < 0.0 {
                        ((len + end_arg) as isize).max(0) as usize
                    } else {
                        (end_arg as usize).min(buf_len)
                    };
                    let new_len = if end > start { end - start } else { 0 };
                    let new_buf: Vec<u8> = buf_data[start..start + new_len].to_vec();
                    let new_ab = interp.create_arraybuffer(new_buf);
                    let id = new_ab.borrow().id.unwrap();
                    return Completion::Normal(JsValue::Object(JsObject { id }));
                }
                Completion::Throw(interp.create_type_error("not an ArrayBuffer"))
            },
        ));
        ab_proto.borrow_mut().insert_builtin("slice".to_string(), slice_fn);

        // @@toStringTag
        let tag = JsValue::String(JsString::from_str("ArrayBuffer"));
        let sym_key = format!("Symbol(Symbol.toStringTag)");
        ab_proto.borrow_mut().insert_property(
            sym_key,
            PropertyDescriptor::data(tag, false, false, true),
        );

        // ArrayBuffer constructor
        let ab_proto_clone = ab_proto.clone();
        let ctor = self.create_function(JsFunction::native(
            "ArrayBuffer".to_string(),
            1,
            move |interp, _this, args| {
                let len_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let len = to_number(&len_val);
                if len.is_nan() || len < 0.0 || len.fract() != 0.0 || len > 2147483647.0 {
                    return Completion::Throw(interp.create_type_error("Invalid array buffer length"));
                }
                let len = len as usize;
                let buf = vec![0u8; len];
                let buf_rc = Rc::new(RefCell::new(buf));
                let obj = interp.create_object();
                {
                    let mut o = obj.borrow_mut();
                    o.class_name = "ArrayBuffer".to_string();
                    o.prototype = Some(ab_proto_clone.clone());
                    o.arraybuffer_data = Some(buf_rc);
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
                if let JsValue::Object(o) = &arg {
                    if let Some(obj) = interp.get_object(o.id) {
                        let obj_ref = obj.borrow();
                        if obj_ref.typed_array_info.is_some() || obj_ref.data_view_info.is_some() {
                            return Completion::Normal(JsValue::Boolean(true));
                        }
                    }
                }
                Completion::Normal(JsValue::Boolean(false))
            },
        ));
        if let JsValue::Object(o) = &ctor {
            if let Some(obj) = self.get_object(o.id) {
                obj.borrow_mut().insert_builtin("isView".to_string(), is_view_fn);
            }
        }

        self.global_env.borrow_mut().declare("ArrayBuffer", BindingKind::Var);
        let _ = self.global_env.borrow_mut().set("ArrayBuffer", ctor);
    }

    pub(crate) fn create_arraybuffer(&mut self, data: Vec<u8>) -> Rc<RefCell<JsObjectData>> {
        let buf_rc = Rc::new(RefCell::new(data));
        let obj = self.create_object();
        {
            let mut o = obj.borrow_mut();
            o.class_name = "ArrayBuffer".to_string();
            o.prototype = self.arraybuffer_prototype.clone();
            o.arraybuffer_data = Some(buf_rc);
        }
        obj
    }

    fn setup_typed_array_base_prototype(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "TypedArray".to_string();
        self.typed_array_prototype = Some(proto.clone());

        // Getters: buffer, byteOffset, byteLength, length
        macro_rules! ta_getter {
            ($name:expr, $field:ident) => {{
                let getter = self.create_function(JsFunction::native(
                    format!("get {}", $name),
                    0,
                    |interp, this_val, _args| {
                        if let JsValue::Object(o) = this_val
                            && let Some(obj) = interp.get_object(o.id)
                        {
                            let obj_ref = obj.borrow();
                            if let Some(ref ta) = obj_ref.typed_array_info {
                                return Completion::Normal(JsValue::Number(ta.$field as f64));
                            }
                        }
                        Completion::Throw(interp.create_type_error("not a TypedArray"))
                    },
                ));
                proto.borrow_mut().insert_property(
                    $name.to_string(),
                    PropertyDescriptor {
                        value: None, writable: None,
                        get: Some(getter), set: None,
                        enumerable: Some(false), configurable: Some(true),
                    },
                );
            }};
        }
        ta_getter!("byteOffset", byte_offset);
        ta_getter!("byteLength", byte_length);
        ta_getter!("length", array_length);

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
                        // Look for the stored buffer object reference
                        let buf_val = obj_ref.get_property("__buffer__");
                        if !matches!(buf_val, JsValue::Undefined) {
                            return Completion::Normal(buf_val);
                        }
                    }
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto.borrow_mut().insert_property(
            "buffer".to_string(),
            PropertyDescriptor {
                value: None, writable: None,
                get: Some(buffer_getter), set: None,
                enumerable: Some(false), configurable: Some(true),
            },
        );

        // [Symbol.iterator] = values
        self.setup_ta_values_method(&proto);

        // entries, keys, values
        self.setup_ta_iterator_methods(&proto);

        // at
        let at_fn = self.create_function(JsFunction::native(
            "at".to_string(), 1,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if let Some(ref ta) = obj_ref.typed_array_info {
                        let idx_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let idx = to_number(&idx_val) as i64;
                        let len = ta.array_length as i64;
                        let actual = if idx < 0 { len + idx } else { idx };
                        if actual < 0 || actual >= len {
                            return Completion::Normal(JsValue::Undefined);
                        }
                        return Completion::Normal(typed_array_get_index(ta, actual as usize));
                    }
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto.borrow_mut().insert_builtin("at".to_string(), at_fn);

        // set
        let set_fn = self.create_function(JsFunction::native(
            "set".to_string(), 1,
            |interp, this_val, args| {
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
                    let offset = if args.len() > 1 { to_number(&args[1]) as usize } else { 0 };

                    if let JsValue::Object(src_o) = &source {
                        if let Some(src_obj) = interp.get_object(src_o.id) {
                            let src_ref = src_obj.borrow();
                            if let Some(ref src_ta) = src_ref.typed_array_info {
                                let src_len = src_ta.array_length;
                                if offset + src_len > ta.array_length {
                                    return Completion::Throw(interp.create_type_error("offset is out of bounds"));
                                }
                                for i in 0..src_len {
                                    let val = typed_array_get_index(src_ta, i);
                                    typed_array_set_index(&ta, offset + i, &val);
                                }
                                return Completion::Normal(JsValue::Undefined);
                            }
                            // Array-like source
                            let len_val = src_ref.get_property("length");
                            drop(src_ref);
                            let src_len = to_number(&len_val) as usize;
                            if offset + src_len > ta.array_length {
                                return Completion::Throw(interp.create_type_error("offset is out of bounds"));
                            }
                            for i in 0..src_len {
                                let val = match interp.get_object_property(src_o.id, &i.to_string(), &source) {
                                    Completion::Normal(v) => v,
                                    other => return other,
                                };
                                typed_array_set_index(&ta, offset + i, &val);
                            }
                            return Completion::Normal(JsValue::Undefined);
                        }
                    }
                    Completion::Throw(interp.create_type_error("argument is not an object"))
                } else {
                    Completion::Throw(interp.create_type_error("not a TypedArray"))
                }
            },
        ));
        proto.borrow_mut().insert_builtin("set".to_string(), set_fn);

        // subarray
        let subarray_fn = self.create_function(JsFunction::native(
            "subarray".to_string(), 2,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let (ta, buf_val) = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info {
                            (ta.clone(), obj_ref.get_property("__buffer__"))
                        } else {
                            return Completion::Throw(interp.create_type_error("not a TypedArray"));
                        }
                    };
                    let len = ta.array_length as f64;
                    let begin_arg = args.first().map(|v| to_number(v)).unwrap_or(0.0);
                    let begin = if begin_arg < 0.0 {
                        ((len + begin_arg) as isize).max(0) as usize
                    } else {
                        (begin_arg as usize).min(ta.array_length)
                    };
                    let end_arg = if args.len() > 1 && !matches!(args[1], JsValue::Undefined) {
                        to_number(&args[1])
                    } else {
                        len
                    };
                    let end = if end_arg < 0.0 {
                        ((len + end_arg) as isize).max(0) as usize
                    } else {
                        (end_arg as usize).min(ta.array_length)
                    };
                    let new_len = if end > begin { end - begin } else { 0 };
                    let bpe = ta.kind.bytes_per_element();
                    let new_offset = ta.byte_offset + begin * bpe;
                    let new_byte_len = new_len * bpe;
                    let new_ta = TypedArrayInfo {
                        kind: ta.kind,
                        buffer: ta.buffer.clone(),
                        byte_offset: new_offset,
                        byte_length: new_byte_len,
                        array_length: new_len,
                    };
                    let result = interp.create_typed_array_object(new_ta, buf_val);
                    let id = result.borrow().id.unwrap();
                    return Completion::Normal(JsValue::Object(JsObject { id }));
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto.borrow_mut().insert_builtin("subarray".to_string(), subarray_fn);

        // slice
        let slice_fn = self.create_function(JsFunction::native(
            "slice".to_string(), 2,
            |interp, this_val, args| {
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
                    let len = ta.array_length as f64;
                    let begin_arg = args.first().map(|v| to_number(v)).unwrap_or(0.0);
                    let begin = if begin_arg < 0.0 {
                        ((len + begin_arg) as isize).max(0) as usize
                    } else {
                        (begin_arg as usize).min(ta.array_length)
                    };
                    let end_arg = if args.len() > 1 && !matches!(args[1], JsValue::Undefined) {
                        to_number(&args[1])
                    } else {
                        len
                    };
                    let end = if end_arg < 0.0 {
                        ((len + end_arg) as isize).max(0) as usize
                    } else {
                        (end_arg as usize).min(ta.array_length)
                    };
                    let new_len = if end > begin { end - begin } else { 0 };
                    let bpe = ta.kind.bytes_per_element();
                    let new_buf = vec![0u8; new_len * bpe];
                    let new_buf_rc = Rc::new(RefCell::new(new_buf));
                    // Copy elements
                    for i in 0..new_len {
                        let val = typed_array_get_index(&ta, begin + i);
                        let new_ta_tmp = TypedArrayInfo {
                            kind: ta.kind,
                            buffer: new_buf_rc.clone(),
                            byte_offset: 0,
                            byte_length: new_len * bpe,
                            array_length: new_len,
                        };
                        typed_array_set_index(&new_ta_tmp, i, &val);
                    }
                    let new_ta_info = TypedArrayInfo {
                        kind: ta.kind,
                        buffer: new_buf_rc.clone(),
                        byte_offset: 0,
                        byte_length: new_len * bpe,
                        array_length: new_len,
                    };
                    // Create the buffer object
                    let ab_obj = interp.create_object();
                    {
                        let mut ab = ab_obj.borrow_mut();
                        ab.class_name = "ArrayBuffer".to_string();
                        ab.prototype = interp.arraybuffer_prototype.clone();
                        ab.arraybuffer_data = Some(new_buf_rc);
                    }
                    let ab_id = ab_obj.borrow().id.unwrap();
                    let buf_val = JsValue::Object(JsObject { id: ab_id });
                    let result = interp.create_typed_array_object(new_ta_info, buf_val);
                    let id = result.borrow().id.unwrap();
                    return Completion::Normal(JsValue::Object(JsObject { id }));
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto.borrow_mut().insert_builtin("slice".to_string(), slice_fn);

        // copyWithin
        let copy_within_fn = self.create_function(JsFunction::native(
            "copyWithin".to_string(), 2,
            |interp, this_val, args| {
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
                    let len = ta.array_length as i64;
                    let target = {
                        let v = to_number(args.first().unwrap_or(&JsValue::Undefined)) as i64;
                        (if v < 0 { (len + v).max(0) } else { v.min(len) }) as usize
                    };
                    let start = {
                        let v = to_number(args.get(1).unwrap_or(&JsValue::Undefined)) as i64;
                        (if v < 0 { (len + v).max(0) } else { v.min(len) }) as usize
                    };
                    let end = {
                        let v = if args.len() > 2 && !matches!(args[2], JsValue::Undefined) {
                            to_number(&args[2]) as i64
                        } else { len };
                        (if v < 0 { (len + v).max(0) } else { v.min(len) }) as usize
                    };
                    let count = (end - start).min(len as usize - target).min(end - start);
                    let bpe = ta.kind.bytes_per_element();
                    let mut buf = ta.buffer.borrow_mut();
                    let src_start = ta.byte_offset + start * bpe;
                    let dst_start = ta.byte_offset + target * bpe;
                    let byte_count = count * bpe;
                    // Use memmove semantics
                    let src: Vec<u8> = buf[src_start..src_start + byte_count].to_vec();
                    buf[dst_start..dst_start + byte_count].copy_from_slice(&src);
                    return Completion::Normal(this_val.clone());
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto.borrow_mut().insert_builtin("copyWithin".to_string(), copy_within_fn);

        // fill
        let fill_fn = self.create_function(JsFunction::native(
            "fill".to_string(), 1,
            |interp, this_val, args| {
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
                    let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let len = ta.array_length as f64;
                    let start = {
                        let v = if args.len() > 1 { to_number(&args[1]) } else { 0.0 };
                        if v < 0.0 { ((len + v) as isize).max(0) as usize } else { (v as usize).min(ta.array_length) }
                    };
                    let end = {
                        let v = if args.len() > 2 && !matches!(args[2], JsValue::Undefined) { to_number(&args[2]) } else { len };
                        if v < 0.0 { ((len + v) as isize).max(0) as usize } else { (v as usize).min(ta.array_length) }
                    };
                    for i in start..end {
                        typed_array_set_index(&ta, i, &value);
                    }
                    return Completion::Normal(this_val.clone());
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto.borrow_mut().insert_builtin("fill".to_string(), fill_fn);

        // indexOf
        let index_of_fn = self.create_function(JsFunction::native(
            "indexOf".to_string(), 1,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info { ta.clone() }
                        else { return Completion::Throw(interp.create_type_error("not a TypedArray")); }
                    };
                    let search = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let from = if args.len() > 1 { to_number(&args[1]) as i64 } else { 0 };
                    let start = if from < 0 { (ta.array_length as i64 + from).max(0) as usize } else { from as usize };
                    for i in start..ta.array_length {
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
        proto.borrow_mut().insert_builtin("indexOf".to_string(), index_of_fn);

        // lastIndexOf
        let last_index_of_fn = self.create_function(JsFunction::native(
            "lastIndexOf".to_string(), 1,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info { ta.clone() }
                        else { return Completion::Throw(interp.create_type_error("not a TypedArray")); }
                    };
                    let search = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let from = if args.len() > 1 {
                        to_number(&args[1]) as i64
                    } else {
                        ta.array_length as i64 - 1
                    };
                    let start = if from < 0 {
                        (ta.array_length as i64 + from).max(-1)
                    } else {
                        from.min(ta.array_length as i64 - 1)
                    };
                    let mut i = start;
                    while i >= 0 {
                        let elem = typed_array_get_index(&ta, i as usize);
                        if strict_eq(&elem, &search) {
                            return Completion::Normal(JsValue::Number(i as f64));
                        }
                        i -= 1;
                    }
                    return Completion::Normal(JsValue::Number(-1.0));
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto.borrow_mut().insert_builtin("lastIndexOf".to_string(), last_index_of_fn);

        // includes
        let includes_fn = self.create_function(JsFunction::native(
            "includes".to_string(), 1,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info { ta.clone() }
                        else { return Completion::Throw(interp.create_type_error("not a TypedArray")); }
                    };
                    let search = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let from = if args.len() > 1 { to_number(&args[1]) as i64 } else { 0 };
                    let start = if from < 0 { (ta.array_length as i64 + from).max(0) as usize } else { from as usize };
                    for i in start..ta.array_length {
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
        proto.borrow_mut().insert_builtin("includes".to_string(), includes_fn);

        // Higher-order methods: find, findIndex, findLast, findLastIndex, forEach, map, filter,
        // every, some, reduce, reduceRight
        self.setup_ta_higher_order_methods(&proto);

        // reverse
        let reverse_fn = self.create_function(JsFunction::native(
            "reverse".to_string(), 0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info { ta.clone() }
                        else { return Completion::Throw(interp.create_type_error("not a TypedArray")); }
                    };
                    let mut lo = 0usize;
                    let mut hi = ta.array_length;
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
        proto.borrow_mut().insert_builtin("reverse".to_string(), reverse_fn);

        // sort
        let sort_fn = self.create_function(JsFunction::native(
            "sort".to_string(), 1,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info { ta.clone() }
                        else { return Completion::Throw(interp.create_type_error("not a TypedArray")); }
                    };
                    let comparefn = args.first().cloned();
                    let mut elems: Vec<JsValue> = (0..ta.array_length)
                        .map(|i| typed_array_get_index(&ta, i))
                        .collect();
                    // Sort with comparison
                    let mut error: Option<JsValue> = None;
                    elems.sort_by(|a, b| {
                        if error.is_some() {
                            return std::cmp::Ordering::Equal;
                        }
                        if let Some(ref cmp) = comparefn {
                            if !matches!(cmp, JsValue::Undefined) {
                                match interp.call_function(cmp, &JsValue::Undefined, &[a.clone(), b.clone()]) {
                                    Completion::Normal(v) => {
                                        let n = to_number(&v);
                                        if n < 0.0 { return std::cmp::Ordering::Less; }
                                        if n > 0.0 { return std::cmp::Ordering::Greater; }
                                        return std::cmp::Ordering::Equal;
                                    }
                                    Completion::Throw(e) => {
                                        error = Some(e);
                                        return std::cmp::Ordering::Equal;
                                    }
                                    _ => return std::cmp::Ordering::Equal,
                                }
                            }
                        }
                        // Default numeric sort
                        let na = to_number(a);
                        let nb = to_number(b);
                        na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal)
                    });
                    if let Some(e) = error {
                        return Completion::Throw(e);
                    }
                    for (i, val) in elems.iter().enumerate() {
                        typed_array_set_index(&ta, i, val);
                    }
                    return Completion::Normal(this_val.clone());
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto.borrow_mut().insert_builtin("sort".to_string(), sort_fn);

        // join
        let join_fn = self.create_function(JsFunction::native(
            "join".to_string(), 1,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info { ta.clone() }
                        else { return Completion::Throw(interp.create_type_error("not a TypedArray")); }
                    };
                    let sep = if args.is_empty() || matches!(args[0], JsValue::Undefined) {
                        ",".to_string()
                    } else {
                        to_js_string(&args[0])
                    };
                    let parts: Vec<String> = (0..ta.array_length)
                        .map(|i| to_js_string(&typed_array_get_index(&ta, i)))
                        .collect();
                    Completion::Normal(JsValue::String(JsString::from_str(&parts.join(&sep))))
                } else {
                    Completion::Throw(interp.create_type_error("not a TypedArray"))
                }
            },
        ));
        proto.borrow_mut().insert_builtin("join".to_string(), join_fn);

        // toString (same as join with comma)
        let tostring_fn = self.create_function(JsFunction::native(
            "toString".to_string(), 0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info { ta.clone() }
                        else { return Completion::Throw(interp.create_type_error("not a TypedArray")); }
                    };
                    let parts: Vec<String> = (0..ta.array_length)
                        .map(|i| to_js_string(&typed_array_get_index(&ta, i)))
                        .collect();
                    Completion::Normal(JsValue::String(JsString::from_str(&parts.join(","))))
                } else {
                    Completion::Throw(interp.create_type_error("not a TypedArray"))
                }
            },
        ));
        proto.borrow_mut().insert_builtin("toString".to_string(), tostring_fn);

        // toReversed
        let to_reversed_fn = self.create_function(JsFunction::native(
            "toReversed".to_string(), 0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info { ta.clone() }
                        else { return Completion::Throw(interp.create_type_error("not a TypedArray")); }
                    };
                    let len = ta.array_length;
                    let bpe = ta.kind.bytes_per_element();
                    let new_buf = vec![0u8; len * bpe];
                    let new_buf_rc = Rc::new(RefCell::new(new_buf));
                    let new_ta = TypedArrayInfo {
                        kind: ta.kind, buffer: new_buf_rc.clone(),
                        byte_offset: 0, byte_length: len * bpe, array_length: len,
                    };
                    for i in 0..len {
                        let val = typed_array_get_index(&ta, len - 1 - i);
                        typed_array_set_index(&new_ta, i, &val);
                    }
                    let ab_obj = interp.create_object();
                    {
                        let mut ab = ab_obj.borrow_mut();
                        ab.class_name = "ArrayBuffer".to_string();
                        ab.prototype = interp.arraybuffer_prototype.clone();
                        ab.arraybuffer_data = Some(new_buf_rc);
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
        proto.borrow_mut().insert_builtin("toReversed".to_string(), to_reversed_fn);

        // toSorted
        let to_sorted_fn = self.create_function(JsFunction::native(
            "toSorted".to_string(), 1,
            |interp, this_val, args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info { ta.clone() }
                        else { return Completion::Throw(interp.create_type_error("not a TypedArray")); }
                    };
                    let comparefn = args.first().cloned();
                    let mut elems: Vec<JsValue> = (0..ta.array_length)
                        .map(|i| typed_array_get_index(&ta, i))
                        .collect();
                    let mut error: Option<JsValue> = None;
                    elems.sort_by(|a, b| {
                        if error.is_some() { return std::cmp::Ordering::Equal; }
                        if let Some(ref cmp) = comparefn {
                            if !matches!(cmp, JsValue::Undefined) {
                                match interp.call_function(cmp, &JsValue::Undefined, &[a.clone(), b.clone()]) {
                                    Completion::Normal(v) => {
                                        let n = to_number(&v);
                                        if n < 0.0 { return std::cmp::Ordering::Less; }
                                        if n > 0.0 { return std::cmp::Ordering::Greater; }
                                        return std::cmp::Ordering::Equal;
                                    }
                                    Completion::Throw(e) => { error = Some(e); return std::cmp::Ordering::Equal; }
                                    _ => return std::cmp::Ordering::Equal,
                                }
                            }
                        }
                        let na = to_number(a);
                        let nb = to_number(b);
                        na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal)
                    });
                    if let Some(e) = error { return Completion::Throw(e); }
                    let len = ta.array_length;
                    let bpe = ta.kind.bytes_per_element();
                    let new_buf = vec![0u8; len * bpe];
                    let new_buf_rc = Rc::new(RefCell::new(new_buf));
                    let new_ta = TypedArrayInfo {
                        kind: ta.kind, buffer: new_buf_rc.clone(),
                        byte_offset: 0, byte_length: len * bpe, array_length: len,
                    };
                    for (i, val) in elems.iter().enumerate() {
                        typed_array_set_index(&new_ta, i, val);
                    }
                    let ab_obj = interp.create_object();
                    {
                        let mut ab = ab_obj.borrow_mut();
                        ab.class_name = "ArrayBuffer".to_string();
                        ab.prototype = interp.arraybuffer_prototype.clone();
                        ab.arraybuffer_data = Some(new_buf_rc);
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
        proto.borrow_mut().insert_builtin("toSorted".to_string(), to_sorted_fn);

        // @@toStringTag getter
        let to_string_tag_getter = self.create_function(JsFunction::native(
            "get [Symbol.toStringTag]".to_string(), 0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if let Some(ref ta) = obj_ref.typed_array_info {
                        return Completion::Normal(JsValue::String(JsString::from_str(ta.kind.name())));
                    }
                }
                Completion::Normal(JsValue::Undefined)
            },
        ));
        proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor {
                value: None, writable: None,
                get: Some(to_string_tag_getter), set: None,
                enumerable: Some(false), configurable: Some(true),
            },
        );
    }

    fn setup_ta_values_method(&mut self, proto: &Rc<RefCell<JsObjectData>>) {
        let values_fn = self.create_function(JsFunction::native(
            "values".to_string(), 0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info { ta.clone() }
                        else { return Completion::Throw(interp.create_type_error("not a TypedArray")); }
                    };
                    // Create an array from the typed array and return array iterator
                    let arr = interp.create_array_from_ta(&ta);
                    let arr_id = arr.borrow().id.unwrap();
                    let iter = interp.create_array_iterator(arr_id, IteratorKind::Value);
                    return Completion::Normal(iter);
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto.borrow_mut().insert_builtin("values".to_string(), values_fn.clone());
        proto.borrow_mut().insert_builtin("Symbol(Symbol.iterator)".to_string(), values_fn);
    }

    fn setup_ta_iterator_methods(&mut self, proto: &Rc<RefCell<JsObjectData>>) {
        let entries_fn = self.create_function(JsFunction::native(
            "entries".to_string(), 0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info { ta.clone() }
                        else { return Completion::Throw(interp.create_type_error("not a TypedArray")); }
                    };
                    let arr = interp.create_array_from_ta(&ta);
                    let arr_id = arr.borrow().id.unwrap();
                    let iter = interp.create_array_iterator(arr_id, IteratorKind::KeyValue);
                    return Completion::Normal(iter);
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto.borrow_mut().insert_builtin("entries".to_string(), entries_fn);

        let keys_fn = self.create_function(JsFunction::native(
            "keys".to_string(), 0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let ta = {
                        let obj_ref = obj.borrow();
                        if let Some(ref ta) = obj_ref.typed_array_info { ta.clone() }
                        else { return Completion::Throw(interp.create_type_error("not a TypedArray")); }
                    };
                    let arr = interp.create_array_from_ta(&ta);
                    let arr_id = arr.borrow().id.unwrap();
                    let iter = interp.create_array_iterator(arr_id, IteratorKind::Key);
                    return Completion::Normal(iter);
                }
                Completion::Throw(interp.create_type_error("not a TypedArray"))
            },
        ));
        proto.borrow_mut().insert_builtin("keys".to_string(), keys_fn);
    }

    fn create_array_from_ta(&mut self, ta: &TypedArrayInfo) -> Rc<RefCell<JsObjectData>> {
        let elems: Vec<JsValue> = (0..ta.array_length)
            .map(|i| typed_array_get_index(ta, i))
            .collect();
        let len = elems.len();
        let arr = self.create_object();
        {
            let mut a = arr.borrow_mut();
            a.class_name = "Array".to_string();
            a.prototype = self.array_prototype.clone();
            a.array_elements = Some(elems);
            a.insert_value("length".to_string(), JsValue::Number(len as f64));
        }
        arr
    }

    fn setup_ta_higher_order_methods(&mut self, proto: &Rc<RefCell<JsObjectData>>) {
        // find
        let find_fn = self.create_function(JsFunction::native(
            "find".to_string(), 1,
            |interp, this_val, args| {
                let (ta, callback) = match extract_ta_and_callback(interp, this_val, args) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                for i in 0..ta.array_length {
                    let val = typed_array_get_index(&ta, i);
                    match interp.call_function(&callback, &this_arg, &[val.clone(), JsValue::Number(i as f64), this_val.clone()]) {
                        Completion::Normal(result) => {
                            if to_boolean(&result) {
                                return Completion::Normal(val);
                            }
                        }
                        other => return other,
                    }
                }
                Completion::Normal(JsValue::Undefined)
            },
        ));
        proto.borrow_mut().insert_builtin("find".to_string(), find_fn);

        // findIndex
        let find_index_fn = self.create_function(JsFunction::native(
            "findIndex".to_string(), 1,
            |interp, this_val, args| {
                let (ta, callback) = match extract_ta_and_callback(interp, this_val, args) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                for i in 0..ta.array_length {
                    let val = typed_array_get_index(&ta, i);
                    match interp.call_function(&callback, &this_arg, &[val, JsValue::Number(i as f64), this_val.clone()]) {
                        Completion::Normal(result) => {
                            if to_boolean(&result) {
                                return Completion::Normal(JsValue::Number(i as f64));
                            }
                        }
                        other => return other,
                    }
                }
                Completion::Normal(JsValue::Number(-1.0))
            },
        ));
        proto.borrow_mut().insert_builtin("findIndex".to_string(), find_index_fn);

        // findLast
        let find_last_fn = self.create_function(JsFunction::native(
            "findLast".to_string(), 1,
            |interp, this_val, args| {
                let (ta, callback) = match extract_ta_and_callback(interp, this_val, args) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let mut i = ta.array_length as i64 - 1;
                while i >= 0 {
                    let val = typed_array_get_index(&ta, i as usize);
                    match interp.call_function(&callback, &this_arg, &[val.clone(), JsValue::Number(i as f64), this_val.clone()]) {
                        Completion::Normal(result) => {
                            if to_boolean(&result) {
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
        proto.borrow_mut().insert_builtin("findLast".to_string(), find_last_fn);

        // findLastIndex
        let find_last_index_fn = self.create_function(JsFunction::native(
            "findLastIndex".to_string(), 1,
            |interp, this_val, args| {
                let (ta, callback) = match extract_ta_and_callback(interp, this_val, args) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let mut i = ta.array_length as i64 - 1;
                while i >= 0 {
                    let val = typed_array_get_index(&ta, i as usize);
                    match interp.call_function(&callback, &this_arg, &[val, JsValue::Number(i as f64), this_val.clone()]) {
                        Completion::Normal(result) => {
                            if to_boolean(&result) {
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
        proto.borrow_mut().insert_builtin("findLastIndex".to_string(), find_last_index_fn);

        // forEach
        let for_each_fn = self.create_function(JsFunction::native(
            "forEach".to_string(), 1,
            |interp, this_val, args| {
                let (ta, callback) = match extract_ta_and_callback(interp, this_val, args) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                for i in 0..ta.array_length {
                    let val = typed_array_get_index(&ta, i);
                    match interp.call_function(&callback, &this_arg, &[val, JsValue::Number(i as f64), this_val.clone()]) {
                        Completion::Normal(_) => {}
                        other => return other,
                    }
                }
                Completion::Normal(JsValue::Undefined)
            },
        ));
        proto.borrow_mut().insert_builtin("forEach".to_string(), for_each_fn);

        // map
        let map_fn = self.create_function(JsFunction::native(
            "map".to_string(), 1,
            |interp, this_val, args| {
                let (ta, callback) = match extract_ta_and_callback(interp, this_val, args) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let len = ta.array_length;
                let bpe = ta.kind.bytes_per_element();
                let new_buf = vec![0u8; len * bpe];
                let new_buf_rc = Rc::new(RefCell::new(new_buf));
                let new_ta = TypedArrayInfo {
                    kind: ta.kind, buffer: new_buf_rc.clone(),
                    byte_offset: 0, byte_length: len * bpe, array_length: len,
                };
                for i in 0..len {
                    let val = typed_array_get_index(&ta, i);
                    match interp.call_function(&callback, &this_arg, &[val, JsValue::Number(i as f64), this_val.clone()]) {
                        Completion::Normal(result) => {
                            typed_array_set_index(&new_ta, i, &result);
                        }
                        other => return other,
                    }
                }
                let ab_obj = interp.create_object();
                {
                    let mut ab = ab_obj.borrow_mut();
                    ab.class_name = "ArrayBuffer".to_string();
                    ab.prototype = interp.arraybuffer_prototype.clone();
                    ab.arraybuffer_data = Some(new_buf_rc);
                }
                let ab_id = ab_obj.borrow().id.unwrap();
                let buf_val = JsValue::Object(JsObject { id: ab_id });
                let result = interp.create_typed_array_object(new_ta, buf_val);
                let id = result.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(JsObject { id }))
            },
        ));
        proto.borrow_mut().insert_builtin("map".to_string(), map_fn);

        // filter
        let filter_fn = self.create_function(JsFunction::native(
            "filter".to_string(), 1,
            |interp, this_val, args| {
                let (ta, callback) = match extract_ta_and_callback(interp, this_val, args) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let mut kept: Vec<JsValue> = Vec::new();
                for i in 0..ta.array_length {
                    let val = typed_array_get_index(&ta, i);
                    match interp.call_function(&callback, &this_arg, &[val.clone(), JsValue::Number(i as f64), this_val.clone()]) {
                        Completion::Normal(result) => {
                            if to_boolean(&result) {
                                kept.push(val);
                            }
                        }
                        other => return other,
                    }
                }
                let len = kept.len();
                let bpe = ta.kind.bytes_per_element();
                let new_buf = vec![0u8; len * bpe];
                let new_buf_rc = Rc::new(RefCell::new(new_buf));
                let new_ta = TypedArrayInfo {
                    kind: ta.kind, buffer: new_buf_rc.clone(),
                    byte_offset: 0, byte_length: len * bpe, array_length: len,
                };
                for (i, val) in kept.iter().enumerate() {
                    typed_array_set_index(&new_ta, i, val);
                }
                let ab_obj = interp.create_object();
                {
                    let mut ab = ab_obj.borrow_mut();
                    ab.class_name = "ArrayBuffer".to_string();
                    ab.prototype = interp.arraybuffer_prototype.clone();
                    ab.arraybuffer_data = Some(new_buf_rc);
                }
                let ab_id = ab_obj.borrow().id.unwrap();
                let buf_val = JsValue::Object(JsObject { id: ab_id });
                let result = interp.create_typed_array_object(new_ta, buf_val);
                let id = result.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(JsObject { id }))
            },
        ));
        proto.borrow_mut().insert_builtin("filter".to_string(), filter_fn);

        // every
        let every_fn = self.create_function(JsFunction::native(
            "every".to_string(), 1,
            |interp, this_val, args| {
                let (ta, callback) = match extract_ta_and_callback(interp, this_val, args) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                for i in 0..ta.array_length {
                    let val = typed_array_get_index(&ta, i);
                    match interp.call_function(&callback, &this_arg, &[val, JsValue::Number(i as f64), this_val.clone()]) {
                        Completion::Normal(result) => {
                            if !to_boolean(&result) {
                                return Completion::Normal(JsValue::Boolean(false));
                            }
                        }
                        other => return other,
                    }
                }
                Completion::Normal(JsValue::Boolean(true))
            },
        ));
        proto.borrow_mut().insert_builtin("every".to_string(), every_fn);

        // some
        let some_fn = self.create_function(JsFunction::native(
            "some".to_string(), 1,
            |interp, this_val, args| {
                let (ta, callback) = match extract_ta_and_callback(interp, this_val, args) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                for i in 0..ta.array_length {
                    let val = typed_array_get_index(&ta, i);
                    match interp.call_function(&callback, &this_arg, &[val, JsValue::Number(i as f64), this_val.clone()]) {
                        Completion::Normal(result) => {
                            if to_boolean(&result) {
                                return Completion::Normal(JsValue::Boolean(true));
                            }
                        }
                        other => return other,
                    }
                }
                Completion::Normal(JsValue::Boolean(false))
            },
        ));
        proto.borrow_mut().insert_builtin("some".to_string(), some_fn);

        // reduce
        let reduce_fn = self.create_function(JsFunction::native(
            "reduce".to_string(), 1,
            |interp, this_val, args| {
                let (ta, callback) = match extract_ta_and_callback(interp, this_val, args) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let mut acc;
                let start;
                if args.len() > 1 {
                    acc = args[1].clone();
                    start = 0;
                } else {
                    if ta.array_length == 0 {
                        return Completion::Throw(interp.create_type_error("Reduce of empty array with no initial value"));
                    }
                    acc = typed_array_get_index(&ta, 0);
                    start = 1;
                }
                for i in start..ta.array_length {
                    let val = typed_array_get_index(&ta, i);
                    match interp.call_function(&callback, &JsValue::Undefined, &[acc, val, JsValue::Number(i as f64), this_val.clone()]) {
                        Completion::Normal(result) => acc = result,
                        other => return other,
                    }
                }
                Completion::Normal(acc)
            },
        ));
        proto.borrow_mut().insert_builtin("reduce".to_string(), reduce_fn);

        // reduceRight
        let reduce_right_fn = self.create_function(JsFunction::native(
            "reduceRight".to_string(), 1,
            |interp, this_val, args| {
                let (ta, callback) = match extract_ta_and_callback(interp, this_val, args) {
                    Ok(v) => v,
                    Err(c) => return c,
                };
                let mut acc;
                let start: i64;
                if args.len() > 1 {
                    acc = args[1].clone();
                    start = ta.array_length as i64 - 1;
                } else {
                    if ta.array_length == 0 {
                        return Completion::Throw(interp.create_type_error("Reduce of empty array with no initial value"));
                    }
                    acc = typed_array_get_index(&ta, ta.array_length - 1);
                    start = ta.array_length as i64 - 2;
                }
                let mut i = start;
                while i >= 0 {
                    let val = typed_array_get_index(&ta, i as usize);
                    match interp.call_function(&callback, &JsValue::Undefined, &[acc, val, JsValue::Number(i as f64), this_val.clone()]) {
                        Completion::Normal(result) => acc = result,
                        other => return other,
                    }
                    i -= 1;
                }
                Completion::Normal(acc)
            },
        ));
        proto.borrow_mut().insert_builtin("reduceRight".to_string(), reduce_right_fn);
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
        let ta_proto = self.typed_array_prototype.clone().unwrap();
        let ta_ctor = self.create_function(JsFunction::native(
            "TypedArray".to_string(), 0,
            |interp, _this, _args| {
                Completion::Throw(interp.create_type_error("Abstract class TypedArray not directly constructable"))
            },
        ));
        // TypedArray.from
        let ta_from_fn = self.create_function(JsFunction::native(
            "from".to_string(), 1,
            |interp, this_val, args| {
                // this_val is the constructor (e.g. Uint8Array)
                let source = args.first().cloned().unwrap_or(JsValue::Undefined);
                let map_fn = args.get(1).cloned();
                let this_arg = args.get(2).cloned().unwrap_or(JsValue::Undefined);

                // Get array-like or iterable
                let values = interp.collect_iterable_or_arraylike(&source);
                let values = match values {
                    Ok(v) => v,
                    Err(c) => return c,
                };

                let mapped: Vec<JsValue> = if let Some(ref mf) = map_fn {
                    let mut result = Vec::new();
                    for (i, val) in values.iter().enumerate() {
                        match interp.call_function(mf, &this_arg, &[val.clone(), JsValue::Number(i as f64)]) {
                            Completion::Normal(v) => result.push(v),
                            other => return other,
                        }
                    }
                    result
                } else {
                    values
                };

                // Call this_val as constructor with the values
                interp.construct_typed_array_from_this(this_val, &mapped)
            },
        ));
        if let JsValue::Object(o) = &ta_ctor {
            if let Some(obj) = self.get_object(o.id) {
                obj.borrow_mut().insert_builtin("from".to_string(), ta_from_fn);
            }
        }
        // TypedArray.of
        let ta_of_fn = self.create_function(JsFunction::native(
            "of".to_string(), 0,
            |interp, this_val, args| {
                interp.construct_typed_array_from_this(this_val, args)
            },
        ));
        if let JsValue::Object(o) = &ta_ctor {
            if let Some(obj) = self.get_object(o.id) {
                obj.borrow_mut().insert_builtin("of".to_string(), ta_of_fn);
            }
        }

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
            let ctor = self.create_function(JsFunction::native(
                name.clone(), 3,
                move |interp, _this, args| {
                    if args.is_empty() {
                        // new XArray() -> length 0
                        return interp.create_typed_array_from_length(kind, 0, &type_proto_clone);
                    }
                    let first = &args[0];
                    match first {
                        JsValue::Number(n) => {
                            let len = *n as usize;
                            interp.create_typed_array_from_length(kind, len, &type_proto_clone)
                        }
                        JsValue::Object(o) => {
                            if let Some(src_obj) = interp.get_object(o.id) {
                                let src_ref = src_obj.borrow();
                                // Case: new XArray(arraybuffer, byteOffset?, length?)
                                if src_ref.arraybuffer_data.is_some() {
                                    let buf_rc = src_ref.arraybuffer_data.as_ref().unwrap().clone();
                                    let buf_len = buf_rc.borrow().len();
                                    drop(src_ref);
                                    let byte_offset = if args.len() > 1 {
                                        to_number(&args[1]) as usize
                                    } else { 0 };
                                    let array_length = if args.len() > 2 && !matches!(args[2], JsValue::Undefined) {
                                        to_number(&args[2]) as usize
                                    } else {
                                        if (buf_len - byte_offset) % bpe != 0 {
                                            return Completion::Throw(interp.create_type_error(
                                                "byte length of typed array should be a multiple of BYTES_PER_ELEMENT"
                                            ));
                                        }
                                        (buf_len - byte_offset) / bpe
                                    };
                                    let byte_length = array_length * bpe;
                                    if byte_offset + byte_length > buf_len {
                                        return Completion::Throw(interp.create_type_error("invalid typed array length"));
                                    }
                                    let ta_info = TypedArrayInfo {
                                        kind,
                                        buffer: buf_rc,
                                        byte_offset,
                                        byte_length,
                                        array_length,
                                    };
                                    let buf_val = first.clone();
                                    let result = interp.create_typed_array_object_with_proto(ta_info, buf_val, &type_proto_clone);
                                    let id = result.borrow().id.unwrap();
                                    return Completion::Normal(JsValue::Object(JsObject { id }));
                                }
                                // Case: new XArray(typedArray)
                                if let Some(ref src_ta) = src_ref.typed_array_info {
                                    let src_ta = src_ta.clone();
                                    drop(src_ref);
                                    let len = src_ta.array_length;
                                    let new_buf = vec![0u8; len * bpe];
                                    let new_buf_rc = Rc::new(RefCell::new(new_buf));
                                    let new_ta = TypedArrayInfo {
                                        kind,
                                        buffer: new_buf_rc.clone(),
                                        byte_offset: 0,
                                        byte_length: len * bpe,
                                        array_length: len,
                                    };
                                    for i in 0..len {
                                        let val = typed_array_get_index(&src_ta, i);
                                        typed_array_set_index(&new_ta, i, &val);
                                    }
                                    let ab_obj = interp.create_object();
                                    {
                                        let mut ab = ab_obj.borrow_mut();
                                        ab.class_name = "ArrayBuffer".to_string();
                                        ab.prototype = interp.arraybuffer_prototype.clone();
                                        ab.arraybuffer_data = Some(new_buf_rc);
                                    }
                                    let ab_id = ab_obj.borrow().id.unwrap();
                                    let buf_val = JsValue::Object(JsObject { id: ab_id });
                                    let result = interp.create_typed_array_object_with_proto(new_ta, buf_val, &type_proto_clone);
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
                                let new_ta = TypedArrayInfo {
                                    kind,
                                    buffer: new_buf_rc.clone(),
                                    byte_offset: 0,
                                    byte_length: len * bpe,
                                    array_length: len,
                                };
                                for (i, val) in values.iter().enumerate() {
                                    typed_array_set_index(&new_ta, i, val);
                                }
                                let ab_obj = interp.create_object();
                                {
                                    let mut ab = ab_obj.borrow_mut();
                                    ab.class_name = "ArrayBuffer".to_string();
                                    ab.prototype = interp.arraybuffer_prototype.clone();
                                    ab.arraybuffer_data = Some(new_buf_rc);
                                }
                                let ab_id = ab_obj.borrow().id.unwrap();
                                let buf_val = JsValue::Object(JsObject { id: ab_id });
                                let result = interp.create_typed_array_object_with_proto(new_ta, buf_val, &type_proto_clone);
                                let id = result.borrow().id.unwrap();
                                return Completion::Normal(JsValue::Object(JsObject { id }));
                            }
                            Completion::Throw(interp.create_type_error("invalid argument"))
                        }
                        _ => {
                            // Treat as length
                            let len = to_number(first) as usize;
                            interp.create_typed_array_from_length(kind, len, &type_proto_clone)
                        }
                    }
                },
            ));

            // Set BYTES_PER_ELEMENT on constructor
            if let JsValue::Object(o) = &ctor {
                if let Some(obj) = self.get_object(o.id) {
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
                            false, false, false,
                        ),
                    );
                    // Inherit from/of from %TypedArray%
                    if let JsValue::Object(ta_o) = &ta_ctor_clone {
                        if let Some(ta_obj) = self.get_object(ta_o.id) {
                            let ta_ref = ta_obj.borrow();
                            if let Some(from_desc) = ta_ref.get_own_property("from") {
                                if let Some(ref v) = from_desc.value {
                                    obj.borrow_mut().insert_builtin("from".to_string(), v.clone());
                                }
                            }
                            if let Some(of_desc) = ta_ref.get_own_property("of") {
                                if let Some(ref v) = of_desc.value {
                                    obj.borrow_mut().insert_builtin("of".to_string(), v.clone());
                                }
                            }
                        }
                    }
                }
            }

            // Set constructor on prototype
            type_proto.borrow_mut().insert_property(
                "constructor".to_string(),
                PropertyDescriptor::data(ctor.clone(), true, false, true),
            );

            // Store prototype for this kind
            match kind {
                TypedArrayKind::Int8 => self.int8array_prototype = Some(type_proto.clone()),
                TypedArrayKind::Uint8 => self.uint8array_prototype = Some(type_proto.clone()),
                TypedArrayKind::Uint8Clamped => self.uint8clampedarray_prototype = Some(type_proto.clone()),
                TypedArrayKind::Int16 => self.int16array_prototype = Some(type_proto.clone()),
                TypedArrayKind::Uint16 => self.uint16array_prototype = Some(type_proto.clone()),
                TypedArrayKind::Int32 => self.int32array_prototype = Some(type_proto.clone()),
                TypedArrayKind::Uint32 => self.uint32array_prototype = Some(type_proto.clone()),
                TypedArrayKind::Float32 => self.float32array_prototype = Some(type_proto.clone()),
                TypedArrayKind::Float64 => self.float64array_prototype = Some(type_proto.clone()),
                TypedArrayKind::BigInt64 => self.bigint64array_prototype = Some(type_proto.clone()),
                TypedArrayKind::BigUint64 => self.biguint64array_prototype = Some(type_proto.clone()),
            }

            self.global_env.borrow_mut().declare(&name, BindingKind::Var);
            let _ = self.global_env.borrow_mut().set(&name, ctor);
        }
    }

    fn create_typed_array_from_length(
        &mut self, kind: TypedArrayKind, len: usize,
        type_proto: &Rc<RefCell<JsObjectData>>,
    ) -> Completion {
        let bpe = kind.bytes_per_element();
        let buf = vec![0u8; len * bpe];
        let buf_rc = Rc::new(RefCell::new(buf));
        let ta_info = TypedArrayInfo {
            kind,
            buffer: buf_rc.clone(),
            byte_offset: 0,
            byte_length: len * bpe,
            array_length: len,
        };
        let ab_obj = self.create_object();
        {
            let mut ab = ab_obj.borrow_mut();
            ab.class_name = "ArrayBuffer".to_string();
            ab.prototype = self.arraybuffer_prototype.clone();
            ab.arraybuffer_data = Some(buf_rc);
        }
        let ab_id = ab_obj.borrow().id.unwrap();
        let buf_val = JsValue::Object(JsObject { id: ab_id });
        let result = self.create_typed_array_object_with_proto(ta_info, buf_val, type_proto);
        let id = result.borrow().id.unwrap();
        Completion::Normal(JsValue::Object(JsObject { id }))
    }

    pub(crate) fn create_typed_array_object(
        &mut self, info: TypedArrayInfo, buf_val: JsValue,
    ) -> Rc<RefCell<JsObjectData>> {
        let proto = self.get_typed_array_prototype(info.kind);
        let obj = self.create_object();
        {
            let mut o = obj.borrow_mut();
            o.class_name = info.kind.name().to_string();
            o.prototype = proto;
            o.insert_property(
                "__buffer__".to_string(),
                PropertyDescriptor::data(buf_val, false, false, false),
            );
            o.typed_array_info = Some(info);
        }
        obj
    }

    fn create_typed_array_object_with_proto(
        &mut self, info: TypedArrayInfo, buf_val: JsValue,
        proto: &Rc<RefCell<JsObjectData>>,
    ) -> Rc<RefCell<JsObjectData>> {
        let obj = self.create_object();
        {
            let mut o = obj.borrow_mut();
            o.class_name = info.kind.name().to_string();
            o.prototype = Some(proto.clone());
            o.insert_property(
                "__buffer__".to_string(),
                PropertyDescriptor::data(buf_val, false, false, false),
            );
            o.typed_array_info = Some(info);
        }
        obj
    }

    fn get_typed_array_prototype(&self, kind: TypedArrayKind) -> Option<Rc<RefCell<JsObjectData>>> {
        match kind {
            TypedArrayKind::Int8 => self.int8array_prototype.clone(),
            TypedArrayKind::Uint8 => self.uint8array_prototype.clone(),
            TypedArrayKind::Uint8Clamped => self.uint8clampedarray_prototype.clone(),
            TypedArrayKind::Int16 => self.int16array_prototype.clone(),
            TypedArrayKind::Uint16 => self.uint16array_prototype.clone(),
            TypedArrayKind::Int32 => self.int32array_prototype.clone(),
            TypedArrayKind::Uint32 => self.uint32array_prototype.clone(),
            TypedArrayKind::Float32 => self.float32array_prototype.clone(),
            TypedArrayKind::Float64 => self.float64array_prototype.clone(),
            TypedArrayKind::BigInt64 => self.bigint64array_prototype.clone(),
            TypedArrayKind::BigUint64 => self.biguint64array_prototype.clone(),
        }
    }

    fn construct_typed_array_from_this(&mut self, this_val: &JsValue, values: &[JsValue]) -> Completion {
        // Determine which TypedArray constructor `this` is
        if let JsValue::Object(o) = this_val {
            if let Some(obj) = self.get_object(o.id) {
                let name = {
                    let obj_ref = obj.borrow();
                    if let Some(ref func) = obj_ref.callable {
                        match func {
                            JsFunction::Native(n, _, _) => Some(n.clone()),
                            JsFunction::User { name, .. } => name.clone(),
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
                        let len = values.len();
                        let new_buf = vec![0u8; len * bpe];
                        let new_buf_rc = Rc::new(RefCell::new(new_buf));
                        let ta = TypedArrayInfo {
                            kind,
                            buffer: new_buf_rc.clone(),
                            byte_offset: 0,
                            byte_length: len * bpe,
                            array_length: len,
                        };
                        for (i, val) in values.iter().enumerate() {
                            typed_array_set_index(&ta, i, val);
                        }
                        let ab_obj = self.create_object();
                        {
                            let mut ab = ab_obj.borrow_mut();
                            ab.class_name = "ArrayBuffer".to_string();
                            ab.prototype = self.arraybuffer_prototype.clone();
                            ab.arraybuffer_data = Some(new_buf_rc);
                        }
                        let ab_id = ab_obj.borrow().id.unwrap();
                        let buf_val = JsValue::Object(JsObject { id: ab_id });
                        let result = self.create_object();
                        {
                            let mut r = result.borrow_mut();
                            r.class_name = kind.name().to_string();
                            r.prototype = proto;
                            r.insert_property(
                                "__buffer__".to_string(),
                                PropertyDescriptor::data(buf_val, false, false, false),
                            );
                            r.typed_array_info = Some(ta);
                        }
                        let id = result.borrow().id.unwrap();
                        return Completion::Normal(JsValue::Object(JsObject { id }));
                    }
                }
            }
        }
        Completion::Throw(self.create_type_error("not a TypedArray constructor"))
    }

    pub(crate) fn collect_iterable_or_arraylike(&mut self, val: &JsValue) -> Result<Vec<JsValue>, Completion> {
        if let JsValue::Object(o) = val {
            if let Some(obj) = self.get_object(o.id) {
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
                        _ => return Err(Completion::Throw(self.create_type_error("bad iterator"))),
                    };
                    let iter = match self.call_function(&iter_fn, val, &[]) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Err(Completion::Throw(e)),
                        _ => return Err(Completion::Throw(self.create_type_error("bad iterator"))),
                    };
                    let mut values = Vec::new();
                    loop {
                        let next_fn = if let JsValue::Object(io) = &iter {
                            match self.get_object_property(io.id, "next", &iter) {
                                Completion::Normal(v) => v,
                                _ => break,
                            }
                        } else { break; };
                        let result = match self.call_function(&next_fn, &iter, &[]) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Err(Completion::Throw(e)),
                            _ => break,
                        };
                        if let JsValue::Object(ro) = &result {
                            let done = match self.get_object_property(ro.id, "done", &result) {
                                Completion::Normal(v) => to_boolean(&v),
                                _ => true,
                            };
                            if done { break; }
                            let value = match self.get_object_property(ro.id, "value", &result) {
                                Completion::Normal(v) => v,
                                _ => JsValue::Undefined,
                            };
                            values.push(value);
                        } else { break; }
                    }
                    return Ok(values);
                }

                // Array-like
                let len_val = match self.get_object_property(o.id, "length", val) {
                    Completion::Normal(v) => v,
                    _ => return Ok(Vec::new()),
                };
                let len = to_number(&len_val) as usize;
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
        }
        Ok(Vec::new())
    }

    fn setup_dataview(&mut self) {
        let dv_proto = self.create_object();
        dv_proto.borrow_mut().class_name = "DataView".to_string();
        self.dataview_prototype = Some(dv_proto.clone());

        // Getters: buffer, byteOffset, byteLength
        let buffer_getter = self.create_function(JsFunction::native(
            "get buffer".to_string(), 0,
            |interp, this_val, _args| {
                if let JsValue::Object(o) = this_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let obj_ref = obj.borrow();
                    if obj_ref.data_view_info.is_some() {
                        let buf_val = obj_ref.get_property("__buffer__");
                        return Completion::Normal(buf_val);
                    }
                }
                Completion::Throw(interp.create_type_error("not a DataView"))
            },
        ));
        dv_proto.borrow_mut().insert_property(
            "buffer".to_string(),
            PropertyDescriptor { value: None, writable: None, get: Some(buffer_getter), set: None, enumerable: Some(false), configurable: Some(true) },
        );

        macro_rules! dv_getter {
            ($name:expr, $field:ident) => {{
                let getter = self.create_function(JsFunction::native(
                    format!("get {}", $name), 0,
                    |interp, this_val, _args| {
                        if let JsValue::Object(o) = this_val
                            && let Some(obj) = interp.get_object(o.id)
                        {
                            let obj_ref = obj.borrow();
                            if let Some(ref dv) = obj_ref.data_view_info {
                                return Completion::Normal(JsValue::Number(dv.$field as f64));
                            }
                        }
                        Completion::Throw(interp.create_type_error("not a DataView"))
                    },
                ));
                dv_proto.borrow_mut().insert_property(
                    $name.to_string(),
                    PropertyDescriptor { value: None, writable: None, get: Some(getter), set: None, enumerable: Some(false), configurable: Some(true) },
                );
            }};
        }
        dv_getter!("byteOffset", byte_offset);
        dv_getter!("byteLength", byte_length);

        // DataView get/set methods
        macro_rules! dv_get_method {
            ($method_name:expr, $size:expr, $read_fn:expr) => {{
                let getter = self.create_function(JsFunction::native(
                    $method_name.to_string(), 1,
                    |interp, this_val, args| {
                        if let JsValue::Object(o) = this_val
                            && let Some(obj) = interp.get_object(o.id)
                        {
                            let dv = {
                                let obj_ref = obj.borrow();
                                if let Some(ref dv) = obj_ref.data_view_info {
                                    dv.clone()
                                } else {
                                    return Completion::Throw(interp.create_type_error("not a DataView"));
                                }
                            };
                            let byte_offset = to_number(args.first().unwrap_or(&JsValue::Undefined)) as usize;
                            let little_endian = if args.len() > 1 { to_boolean(&args[1]) } else { false };
                            let idx = dv.byte_offset + byte_offset;
                            if idx + $size > dv.byte_offset + dv.byte_length {
                                return Completion::Throw(interp.create_type_error("offset is outside the bounds of the DataView"));
                            }
                            let buf = dv.buffer.borrow();
                            let result = $read_fn(&buf[idx..idx + $size], little_endian);
                            return Completion::Normal(result);
                        }
                        Completion::Throw(interp.create_type_error("not a DataView"))
                    },
                ));
                dv_proto.borrow_mut().insert_builtin($method_name.to_string(), getter);
            }};
        }

        dv_get_method!("getInt8", 1, |buf: &[u8], _le: bool| -> JsValue {
            JsValue::Number(buf[0] as i8 as f64)
        });
        dv_get_method!("getUint8", 1, |buf: &[u8], _le: bool| -> JsValue {
            JsValue::Number(buf[0] as f64)
        });
        dv_get_method!("getInt16", 2, |buf: &[u8], le: bool| -> JsValue {
            let v = if le { i16::from_le_bytes([buf[0], buf[1]]) } else { i16::from_be_bytes([buf[0], buf[1]]) };
            JsValue::Number(v as f64)
        });
        dv_get_method!("getUint16", 2, |buf: &[u8], le: bool| -> JsValue {
            let v = if le { u16::from_le_bytes([buf[0], buf[1]]) } else { u16::from_be_bytes([buf[0], buf[1]]) };
            JsValue::Number(v as f64)
        });
        dv_get_method!("getInt32", 4, |buf: &[u8], le: bool| -> JsValue {
            let v = if le { i32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) } else { i32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) };
            JsValue::Number(v as f64)
        });
        dv_get_method!("getUint32", 4, |buf: &[u8], le: bool| -> JsValue {
            let v = if le { u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) } else { u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) };
            JsValue::Number(v as f64)
        });
        dv_get_method!("getFloat32", 4, |buf: &[u8], le: bool| -> JsValue {
            let v = if le { f32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) } else { f32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) };
            JsValue::Number(v as f64)
        });
        dv_get_method!("getFloat64", 8, |buf: &[u8], le: bool| -> JsValue {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(buf);
            let v = if le { f64::from_le_bytes(bytes) } else { f64::from_be_bytes(bytes) };
            JsValue::Number(v)
        });
        dv_get_method!("getBigInt64", 8, |buf: &[u8], le: bool| -> JsValue {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(buf);
            let v = if le { i64::from_le_bytes(bytes) } else { i64::from_be_bytes(bytes) };
            JsValue::BigInt(JsBigInt { value: num_bigint::BigInt::from(v) })
        });
        dv_get_method!("getBigUint64", 8, |buf: &[u8], le: bool| -> JsValue {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(buf);
            let v = if le { u64::from_le_bytes(bytes) } else { u64::from_be_bytes(bytes) };
            JsValue::BigInt(JsBigInt { value: num_bigint::BigInt::from(v) })
        });

        // DataView set methods
        macro_rules! dv_set_method {
            ($method_name:expr, $size:expr, $write_fn:expr) => {{
                let setter = self.create_function(JsFunction::native(
                    $method_name.to_string(), 2,
                    |interp, this_val, args| {
                        if let JsValue::Object(o) = this_val
                            && let Some(obj) = interp.get_object(o.id)
                        {
                            let dv = {
                                let obj_ref = obj.borrow();
                                if let Some(ref dv) = obj_ref.data_view_info {
                                    dv.clone()
                                } else {
                                    return Completion::Throw(interp.create_type_error("not a DataView"));
                                }
                            };
                            let byte_offset = to_number(args.first().unwrap_or(&JsValue::Undefined)) as usize;
                            let value = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                            let little_endian = if args.len() > 2 { to_boolean(&args[2]) } else { false };
                            let idx = dv.byte_offset + byte_offset;
                            if idx + $size > dv.byte_offset + dv.byte_length {
                                return Completion::Throw(interp.create_type_error("offset is outside the bounds of the DataView"));
                            }
                            let mut buf = dv.buffer.borrow_mut();
                            $write_fn(&mut buf[idx..idx + $size], &value, little_endian);
                            return Completion::Normal(JsValue::Undefined);
                        }
                        Completion::Throw(interp.create_type_error("not a DataView"))
                    },
                ));
                dv_proto.borrow_mut().insert_builtin($method_name.to_string(), setter);
            }};
        }

        dv_set_method!("setInt8", 1, |buf: &mut [u8], v: &JsValue, _le: bool| {
            buf[0] = to_number(v) as i32 as i8 as u8;
        });
        dv_set_method!("setUint8", 1, |buf: &mut [u8], v: &JsValue, _le: bool| {
            buf[0] = to_number(v) as i32 as u8;
        });
        dv_set_method!("setInt16", 2, |buf: &mut [u8], v: &JsValue, le: bool| {
            let n = to_number(v) as i16;
            let bytes = if le { n.to_le_bytes() } else { n.to_be_bytes() };
            buf.copy_from_slice(&bytes);
        });
        dv_set_method!("setUint16", 2, |buf: &mut [u8], v: &JsValue, le: bool| {
            let n = to_number(v) as u16;
            let bytes = if le { n.to_le_bytes() } else { n.to_be_bytes() };
            buf.copy_from_slice(&bytes);
        });
        dv_set_method!("setInt32", 4, |buf: &mut [u8], v: &JsValue, le: bool| {
            let n = to_number(v) as i32;
            let bytes = if le { n.to_le_bytes() } else { n.to_be_bytes() };
            buf.copy_from_slice(&bytes);
        });
        dv_set_method!("setUint32", 4, |buf: &mut [u8], v: &JsValue, le: bool| {
            let n = to_number(v) as u32;
            let bytes = if le { n.to_le_bytes() } else { n.to_be_bytes() };
            buf.copy_from_slice(&bytes);
        });
        dv_set_method!("setFloat32", 4, |buf: &mut [u8], v: &JsValue, le: bool| {
            let n = to_number(v) as f32;
            let bytes = if le { n.to_le_bytes() } else { n.to_be_bytes() };
            buf.copy_from_slice(&bytes);
        });
        dv_set_method!("setFloat64", 8, |buf: &mut [u8], v: &JsValue, le: bool| {
            let n = to_number(v);
            let bytes = if le { n.to_le_bytes() } else { n.to_be_bytes() };
            buf.copy_from_slice(&bytes);
        });
        dv_set_method!("setBigInt64", 8, |buf: &mut [u8], v: &JsValue, le: bool| {
            let n = match v {
                JsValue::BigInt(b) => i64::try_from(&b.value).unwrap_or(0),
                _ => 0,
            };
            let bytes = if le { n.to_le_bytes() } else { n.to_be_bytes() };
            buf.copy_from_slice(&bytes);
        });
        dv_set_method!("setBigUint64", 8, |buf: &mut [u8], v: &JsValue, le: bool| {
            let n = match v {
                JsValue::BigInt(b) => u64::try_from(&b.value).unwrap_or(0),
                _ => 0,
            };
            let bytes = if le { n.to_le_bytes() } else { n.to_be_bytes() };
            buf.copy_from_slice(&bytes);
        });

        // @@toStringTag
        let tag = JsValue::String(JsString::from_str("DataView"));
        dv_proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor::data(tag, false, false, true),
        );

        // DataView constructor
        let dv_proto_clone = dv_proto.clone();
        let ctor = self.create_function(JsFunction::native(
            "DataView".to_string(), 1,
            move |interp, _this, args| {
                let buf_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = &buf_arg {
                    if let Some(obj) = interp.get_object(o.id) {
                        let buf_rc = {
                            let obj_ref = obj.borrow();
                            if let Some(ref buf) = obj_ref.arraybuffer_data {
                                buf.clone()
                            } else {
                                return Completion::Throw(interp.create_type_error("First argument to DataView constructor must be an ArrayBuffer"));
                            }
                        };
                        let buf_len = buf_rc.borrow().len();
                        let byte_offset = if args.len() > 1 { to_number(&args[1]) as usize } else { 0 };
                        let byte_length = if args.len() > 2 && !matches!(args[2], JsValue::Undefined) {
                            to_number(&args[2]) as usize
                        } else {
                            buf_len - byte_offset
                        };
                        if byte_offset + byte_length > buf_len {
                            return Completion::Throw(interp.create_type_error("invalid DataView length"));
                        }
                        let dv_info = DataViewInfo {
                            buffer: buf_rc,
                            byte_offset,
                            byte_length,
                        };
                        let result = interp.create_object();
                        {
                            let mut r = result.borrow_mut();
                            r.class_name = "DataView".to_string();
                            r.prototype = Some(dv_proto_clone.clone());
                            r.insert_property(
                                "__buffer__".to_string(),
                                PropertyDescriptor::data(buf_arg.clone(), false, false, false),
                            );
                            r.data_view_info = Some(dv_info);
                        }
                        let id = result.borrow().id.unwrap();
                        return Completion::Normal(JsValue::Object(JsObject { id }));
                    }
                }
                Completion::Throw(interp.create_type_error("First argument to DataView constructor must be an ArrayBuffer"))
            },
        ));

        self.global_env.borrow_mut().declare("DataView", BindingKind::Var);
        let _ = self.global_env.borrow_mut().set("DataView", ctor);
    }
}

fn extract_ta_and_callback(
    interp: &mut Interpreter, this_val: &JsValue, args: &[JsValue],
) -> Result<(TypedArrayInfo, JsValue), Completion> {
    if let JsValue::Object(o) = this_val {
        if let Some(obj) = interp.get_object(o.id) {
            let ta = {
                let obj_ref = obj.borrow();
                if let Some(ref ta) = obj_ref.typed_array_info {
                    ta.clone()
                } else {
                    return Err(Completion::Throw(interp.create_type_error("not a TypedArray")));
                }
            };
            let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
            if matches!(callback, JsValue::Undefined) {
                return Err(Completion::Throw(interp.create_type_error("callback is not a function")));
            }
            return Ok((ta, callback));
        }
    }
    Err(Completion::Throw(interp.create_type_error("not a TypedArray")))
}

fn same_value_zero(x: &JsValue, y: &JsValue) -> bool {
    match (x, y) {
        (JsValue::Number(a), JsValue::Number(b)) => {
            if a.is_nan() && b.is_nan() { return true; }
            if *a == 0.0 && *b == 0.0 { return true; }
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
