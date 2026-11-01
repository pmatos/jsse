//! `next` and `return` of %WrapForValidIteratorPrototype% (§27.1.3.2.1).

use super::*;

pub(super) fn define_wrap_valid_methods(interp: &mut Interpreter, proto_id: u64) {
    interp.define_method(proto_id, "next", 0, move |interp, this, _args| {
        let Some(this_id) = this.as_object_id() else {
            let err = interp.create_type_error("next requires an Iterator wrapper");
            return Completion::Throw(err);
        };
        let record = interp.get_object_cell(this_id).and_then(|o| {
            if let Some(crate::interpreter::types::IterHelperData::Delegation { iter, next }) =
                o.borrow().iter_helper()
            {
                Some((iter.clone(), next.clone()))
            } else {
                None
            }
        });
        let (iter, next_method) = match record {
            Some(r) => r,
            None => {
                let err = interp.create_type_error("next requires an Iterator wrapper");
                return Completion::Throw(err);
            }
        };
        interp.call_function(&next_method, &iter, &[])
    });

    interp.define_method(proto_id, "return", 0, move |interp, this, _args| {
        let Some(this_id) = this.as_object_id() else {
            let err = interp.create_type_error("return requires an Iterator wrapper");
            return Completion::Throw(err);
        };
        let record = interp.get_object_cell(this_id).and_then(|o| {
            if let Some(crate::interpreter::types::IterHelperData::Delegation { iter, next }) =
                o.borrow().iter_helper()
            {
                Some((iter.clone(), next.clone()))
            } else {
                None
            }
        });
        let (iter, _next_method) = match record {
            Some(r) => r,
            None => {
                let err = interp.create_type_error("return requires an Iterator wrapper");
                return Completion::Throw(err);
            }
        };
        if let Some(iter_id) = iter.as_object_id() {
            let return_method = match interp.get_object_property(iter_id, "return", &iter) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Completion::Throw(e),
                _ => JsValue::UNDEFINED,
            };
            if return_method.is_nullish() {
                return Completion::Normal(
                    interp.create_iter_result_object(JsValue::UNDEFINED, true),
                );
            }
            interp.call_function(&return_method, &iter, &[])
        } else {
            Completion::Normal(interp.create_iter_result_object(JsValue::UNDEFINED, true))
        }
    });
}

/// `Iterator.from` (§27.1.3.2.1). The iterator record stays rooted across
/// OrdinaryHasInstance, which can run a Proxy `getPrototypeOf` trap.
pub(super) fn iterator_from(
    interp: &mut Interpreter,
    args: &[JsValue],
    iterator_ctor: &JsValue,
    wrapper_proto: u64,
) -> Completion {
    let obj = args.first().cloned().unwrap_or(JsValue::UNDEFINED);

    // Use GetIteratorFlattenable(obj, iterate-strings) per spec
    let (iter_val, next_method) = match get_iterator_flattenable(interp, &obj, false) {
        Ok(pair) => pair,
        Err(e) => return Completion::Throw(e),
    };

    // OrdinaryHasInstance(%Iterator%, iteratorRecord.[[Iterator]]) can run a
    // Proxy getPrototypeOf trap; the record is only held here until then.
    let frame = interp.gc_root_frame();
    interp.gc_root_value(&iter_val);
    interp.gc_root_value(&next_method);
    let has_iter_proto = match interp.ordinary_has_instance(iterator_ctor, &iter_val) {
        Completion::Normal(v) => v.as_boolean() == Some(true),
        _ => false,
    };
    interp.gc_unroot_frame(frame);

    if has_iter_proto {
        return Completion::Normal(iter_val);
    }

    // Create wrapper with shared WrapForValidIteratorPrototype
    let wrapper_id = interp.create_object_id();
    interp
        .get_object_cell_expect(wrapper_id)
        .borrow_mut()
        .set_proto(Some(wrapper_proto));
    interp
        .get_object_cell_expect(wrapper_id)
        .borrow_mut()
        .class_name = "Iterator";
    {
        let mut wrapper = interp.get_object_cell_expect(wrapper_id).borrow_mut();
        wrapper.kind = crate::interpreter::types::ObjectKind::IterHelper(Box::new(
            crate::interpreter::types::IterHelperData::Delegation {
                iter: iter_val.clone(),
                next: next_method.clone(),
            },
        ));
        wrapper
            .gc_native_roots_mut()
            .extend([iter_val, next_method]);
    }

    Completion::Normal(JsValue::object(wrapper_id))
}
