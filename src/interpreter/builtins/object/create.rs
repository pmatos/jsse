//! `Object.create` (§20.1.2.2) and the ObjectDefineProperties step it runs.

use super::super::super::*;

/// The new object, the Properties object and each descriptor object stay rooted
/// across the getters and proxy traps ObjectDefineProperties can run.
pub(super) fn object_create(interp: &mut Interpreter, args: &[JsValue]) -> Completion {
    interp.with_gc_root_scope(|interp| {
        let proto_arg = args.first().cloned().unwrap_or(JsValue::NULL);
        if !proto_arg.is_object() && !proto_arg.is_null() {
            return Completion::Throw(interp.proto_value_error(&proto_arg));
        }
        let new_obj_id = interp.create_object_id();
        interp.gc_root_id(new_obj_id);
        if let Some(proto_id) = proto_arg.as_object_id() {
            interp
                .get_object_cell_expect(new_obj_id)
                .borrow_mut()
                .prototype_id = Some(proto_id);
        } else if proto_arg.is_null() {
            interp
                .get_object_cell_expect(new_obj_id)
                .borrow_mut()
                .prototype_id = None;
        } else {
            unreachable!()
        }
        let id = new_obj_id;
        let target = JsValue::object(id);

        let props_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
        if !(props_arg).is_undefined() {
            // ObjectDefineProperties(target, props_arg)
            let props_obj_val = match interp.to_object(&props_arg) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Completion::Throw(e),
                _ => return Completion::Normal(target),
            };
            interp.gc_root_value(&props_obj_val);
            if let Some(d_id) = props_obj_val.as_object_id() {
                let all_keys = match interp.proxy_own_keys(d_id) {
                    Ok(k) => k,
                    Err(e) => return Completion::Throw(e),
                };
                for key_val in all_keys {
                    let frame = interp.gc_root_frame();
                    let key = to_property_key_string(&key_val);
                    // Check enumerability via [[GetOwnProperty]]
                    let desc_check = match interp.proxy_get_own_property_descriptor(d_id, &key) {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    };
                    if (desc_check).is_undefined() {
                        continue;
                    }
                    interp.gc_root_value(&desc_check);
                    if let Ok(chk) = interp.to_property_descriptor(&desc_check)
                        && chk.enumerable == Some(false)
                    {
                        interp.gc_unroot_frame(frame);
                        continue;
                    }
                    let prop_desc_val = match interp.get_object_property(d_id, &key, &props_obj_val)
                    {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => {
                            interp.gc_unroot_frame(frame);
                            continue;
                        }
                    };
                    interp.gc_root_value(&prop_desc_val);
                    match interp.to_property_descriptor(&prop_desc_val) {
                        Ok(desc) => {
                            if let Some(target_obj) = interp.get_object(id)
                                && !target_obj.borrow_mut().define_own_property(key, desc)
                            {
                                return Completion::Throw(interp.create_type_error(
                                    "Cannot define property on non-extensible object",
                                ));
                            }
                        }
                        Err(Some(e)) => return Completion::Throw(e),
                        Err(None) => {
                            return Completion::Throw(interp.descriptor_error(&prop_desc_val));
                        }
                    }
                    interp.gc_unroot_frame(frame);
                }
            }
        }

        Completion::Normal(target)
    })
}
