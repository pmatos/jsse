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
