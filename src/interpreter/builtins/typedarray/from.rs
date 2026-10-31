//! `%TypedArray%.from` (§23.2.2.1).

use super::*;
use crate::interpreter::helpers::to_integer_or_infinity;

/// TypedArrayCreateFromConstructor plus the checks the element writes depend on.
/// The target stays rooted until the enclosing root scope closes.
fn create_target(
    interp: &mut Interpreter,
    ctor: &JsValue,
    len: usize,
) -> Result<(JsValue, TypedArrayKind), Completion> {
    let target_obj = match interp.typed_array_create(ctor, len) {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    interp.gc_root_value(&target_obj);
    check_target_ta_writable(interp, &target_obj)?;
    let kind = target_obj.as_object_id().and_then(|o| {
        interp
            .get_object_cell(o)
            .and_then(|obj| obj.borrow().typed_array_info().map(|ta| ta.kind))
    });
    match kind {
        Some(kind) => Ok((target_obj, kind)),
        None => Err(Completion::Throw(
            interp.create_type_error("TypedArray.from: target is not a TypedArray"),
        )),
    }
}

/// Steps 5.e / 9: `mapfn` (when present), then the element coercion and write.
fn map_and_store(
    interp: &mut Interpreter,
    map: Option<(&JsValue, &JsValue)>,
    target: &(JsValue, TypedArrayKind),
    k: usize,
    value: JsValue,
) -> Result<(), Completion> {
    let (target_obj, kind) = target;
    let mapped = match map {
        Some((map_fn, this_arg)) => {
            match interp.call_function(map_fn, this_arg, &[value, JsValue::number(k as f64)]) {
                Completion::Normal(v) => v,
                other => return Err(other),
            }
        }
        None => value,
    };
    let coerced = interp
        .typed_array_coerce_value(*kind, &mapped)
        .map_err(Completion::Throw)?;
    if let Some(obj) = target_obj
        .as_object_id()
        .and_then(|o| interp.get_object_cell(o))
    {
        obj.borrow_mut().set_property_value(&k.to_string(), coerced);
    }
    Ok(())
}

/// Step 7: LengthOfArrayLike (ToLength runs `valueOf` through ToNumber).
fn array_like_length(interp: &mut Interpreter, array_like: &JsValue) -> Result<usize, Completion> {
    let Some(o) = array_like.as_object_id() else {
        return Ok(0);
    };
    let len_val = match interp.get_object_property(o, "length", array_like) {
        Completion::Normal(v) => v,
        Completion::Throw(e) => return Err(Completion::Throw(e)),
        _ => JsValue::UNDEFINED,
    };
    let n = interp
        .to_number_value(&len_val)
        .map_err(Completion::Throw)?;
    let int_len = to_integer_or_infinity(n);
    Ok(if int_len < 0.0 {
        0
    } else {
        int_len.min(9007199254740991.0) as usize
    })
}

/// IteratorToList over `iter_fn.call(source)`. The iterator, its cached `next` and
/// every collected value stay rooted for the enclosing root scope, because
/// `next()` and the `done`/`value` getters run user code.
fn collect_iterable(
    interp: &mut Interpreter,
    source: &JsValue,
    iter_fn: &JsValue,
) -> Result<Vec<JsValue>, Completion> {
    if !interp.is_callable(iter_fn) {
        return Err(Completion::Throw(interp.typed_array_iterator_error(false)));
    }
    let iter = match interp.call_function(iter_fn, source, &[]) {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    let Some(iter_id) = iter.as_object_id() else {
        return Err(Completion::Throw(interp.create_type_error(
            "Result of the Symbol.iterator method is not an object",
        )));
    };
    interp.gc_root_value(&iter);
    let next_fn = match interp.get_object_property(iter_id, "next", &iter) {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    interp.gc_root_value(&next_fn);
    let mut values = Vec::new();
    loop {
        let result = match interp.call_function(&next_fn, &iter, &[]) {
            Completion::Normal(v) => v,
            other => return Err(other),
        };
        let Some(result_id) = result.as_object_id() else {
            return Err(Completion::Throw(
                interp.create_type_error("Iterator result is not an object"),
            ));
        };
        interp.gc_root_value(&result);
        let done = match interp.get_object_property(result_id, "done", &result) {
            Completion::Normal(v) => interp.to_boolean_val(&v),
            _ => true,
        };
        if done {
            return Ok(values);
        }
        let value = match interp.get_object_property(result_id, "value", &result) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(Completion::Throw(e)),
            _ => JsValue::UNDEFINED,
        };
        interp.gc_root_value(&value);
        values.push(value);
    }
}

fn from_iterable(
    interp: &mut Interpreter,
    ctor: &JsValue,
    source: &JsValue,
    iter_fn: &JsValue,
    map: Option<(&JsValue, &JsValue)>,
) -> Result<JsValue, Completion> {
    let values = collect_iterable(interp, source, iter_fn)?;
    let target = create_target(interp, ctor, values.len())?;
    for (k, val) in values.into_iter().enumerate() {
        map_and_store(interp, map, &target, k, val)?;
    }
    Ok(target.0)
}

fn from_array_like(
    interp: &mut Interpreter,
    ctor: &JsValue,
    source: &JsValue,
    map: Option<(&JsValue, &JsValue)>,
) -> Result<JsValue, Completion> {
    let array_like = match interp.to_object(source) {
        Completion::Normal(v) => v,
        Completion::Throw(e) => return Err(Completion::Throw(e)),
        _ => {
            return Err(Completion::Throw(
                interp.create_type_error("Cannot convert to object"),
            ));
        }
    };
    interp.gc_root_value(&array_like);
    let len = array_like_length(interp, &array_like)?;
    let target = create_target(interp, ctor, len)?;
    for k in 0..len {
        let val = match array_like.as_object_id() {
            Some(o) => match interp.get_object_property(o, &k.to_string(), &array_like) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(Completion::Throw(e)),
                _ => JsValue::UNDEFINED,
            },
            None => JsValue::UNDEFINED,
        };
        map_and_store(interp, map, &target, k, val)?;
    }
    Ok(target.0)
}

/// The collected values, the source object and the target typed array stay
/// rooted across `mapfn` calls and the `valueOf` hooks of ToNumber/ToBigInt.
pub(super) fn typed_array_from(
    interp: &mut Interpreter,
    this_val: &JsValue,
    args: &[JsValue],
) -> Completion {
    let source = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
    let map_fn = args.get(1).filter(|mf| !mf.is_undefined());
    let this_arg = args.get(2).cloned().unwrap_or(JsValue::UNDEFINED);

    if !interp.is_constructor(this_val) {
        return Completion::Throw(
            interp.create_type_error("TypedArray.from requires a constructor"),
        );
    }
    if let Some(mf) = map_fn
        && !interp.is_callable(mf)
    {
        return Completion::Throw(interp.callback_error(mf, "mapfn is not a function"));
    }
    if source.is_nullish() {
        return Completion::Throw(
            interp.create_type_error("Cannot convert undefined or null to object"),
        );
    }
    let using_iterator = match interp
        .access_property_on_value(&source, &JsPropertyKey::well_known_symbol("iterator"))
    {
        Completion::Normal(v) if !v.is_nullish() => Some(v),
        Completion::Throw(e) => return Completion::Throw(e),
        _ => None,
    };
    let map = map_fn.map(|mf| (mf, &this_arg));

    interp.with_gc_root_scope(|interp| {
        let created = match &using_iterator {
            Some(iter_fn) => from_iterable(interp, this_val, &source, iter_fn, map),
            None => from_array_like(interp, this_val, &source, map),
        };
        match created {
            Ok(target) => Completion::Normal(target),
            Err(c) => c,
        }
    })
}
