//! The `Promise` constructor (§27.2.3.1).

use super::*;

pub(super) fn promise_construct(interp: &mut Interpreter, args: &[JsValue]) -> Completion {
    interp.with_gc_root_scope(|interp| {
        // Step 1: If NewTarget is undefined, throw a TypeError
        if interp.new_target.is_none() {
            return Completion::Throw(
                interp.create_type_error("Promise constructor requires 'new'"),
            );
        }
        // Step 2: If IsCallable(executor) is false, throw a TypeError
        let executor = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
        if !interp.is_callable(&executor) {
            return Completion::Throw(interp.promise_resolver_error(&executor));
        }
        // OrdinaryCreateFromConstructor — realm-aware prototype
        let proto =
            match interp.get_prototype_from_new_target_realm(|realm| realm.promise_prototype) {
                Ok(p) => p,
                Err(e) => return Completion::Throw(e),
            };
        let promise = interp.create_promise_object();
        if let Some(p) = proto
            && let Some(po) = promise.as_object_id()
            && let Some(pobj) = interp.get_object_cell(po)
        {
            pobj.borrow_mut().prototype_id = Some(p);
        }
        let promise_id = promise.as_object_id().unwrap_or_default();
        let (resolve_fn, reject_fn) = interp.create_resolving_functions(promise_id);
        // An executor that ignores its second parameter leaves `reject_fn` held only
        // here, yet it is called if the executor throws after a collection; it also
        // pins the promise, which the resolving functions reference by id only.
        interp.gc_root_value(&reject_fn);
        let result = interp.call_function(
            &executor,
            &JsValue::UNDEFINED,
            &[resolve_fn.clone(), reject_fn.clone()],
        );
        if let Completion::Throw(e) = result
            && let Completion::Throw(e2) =
                interp.call_function(&reject_fn, &JsValue::UNDEFINED, &[e])
        {
            return Completion::Throw(e2);
        }
        Completion::Normal(promise)
    })
}
