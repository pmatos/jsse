//! Complete Array/TypedArray map and filter operations.

use super::super::*;
use super::{array, typedarray};

#[derive(Clone, Copy)]
pub(super) enum Transform {
    ArrayMap,
    ArrayFilter,
    TypedArrayMap,
    TypedArrayFilter,
}

impl Transform {
    fn is_filter(self) -> bool {
        matches!(self, Self::ArrayFilter | Self::TypedArrayFilter)
    }
}

enum Elements {
    Array,
    TypedArray(TypedArrayInfo),
}

impl Elements {
    fn read(
        &self,
        interp: &mut Interpreter,
        object: &JsValue,
        index: usize,
    ) -> Result<Option<JsValue>, Completion> {
        match self {
            Self::Array => array::get_if_present(interp, object, index),
            Self::TypedArray(ta) => Ok(Some(typed_array_get_index(ta, index))),
        }
    }
}

enum Output {
    Array(JsValue),
    TypedArray(JsValue, TypedArrayInfo),
}

impl Output {
    fn create(
        interp: &mut Interpreter,
        elements: &Elements,
        object: &JsValue,
        length: usize,
    ) -> Result<Self, Completion> {
        match elements {
            Elements::Array => {
                let value = array::array_species_create(interp, object, length)?;
                interp.gc_root_value(&value);
                Ok(Self::Array(value))
            }
            Elements::TypedArray(_) => {
                let value = interp
                    .typed_array_species_create(object, &[JsValue::number(length as f64)])
                    .map_err(Completion::Throw)?;
                interp.gc_root_value(&value);
                let ta = value
                    .as_object_id()
                    .and_then(|id| interp.get_object_cell(id))
                    .and_then(|cell| cell.borrow().typed_array_info().cloned())
                    .ok_or_else(|| {
                        Completion::Throw(interp.create_type_error("not a TypedArray"))
                    })?;
                typedarray::check_ta_buffer_writable(interp, &ta)?;
                Ok(Self::TypedArray(value, ta))
            }
        }
    }

    fn write(
        &self,
        interp: &mut Interpreter,
        index: usize,
        value: JsValue,
    ) -> Result<(), Completion> {
        match self {
            Self::Array(object) => array::create_data_property_at(interp, object, index, value)
                .map_err(Completion::Throw),
            Self::TypedArray(_, ta) => {
                // Preserve the existing raw element encoder and its conversion
                // behavior; this refactor does not replace it with general Set.
                typed_array_set_index(ta, index, &value);
                Ok(())
            }
        }
    }

    fn into_value(self) -> JsValue {
        match self {
            Self::Array(value) | Self::TypedArray(value, _) => value,
        }
    }
}

// Array.prototype.map/filter and %TypedArray%.prototype.map/filter.
pub(super) fn transform(
    interp: &mut Interpreter,
    method: Transform,
    this: &JsValue,
    args: &[JsValue],
) -> Completion {
    interp.with_gc_root_scope(|interp| {
        // Length/species access may run JS before the callback or thisArg is used.
        interp.gc_root_value(this);
        for value in args.iter().take(2) {
            interp.gc_root_value(value);
        }
        let (object, length, elements, callback) = match method {
            Transform::ArrayMap | Transform::ArrayFilter => {
                let object = propagate!(array::to_object_val(interp, this));
                interp.gc_root_value(&object);
                let length = propagate!(array::length_of_array_like(interp, &object));
                let callback = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if !interp.is_callable(&callback) {
                    let message = if method.is_filter() {
                        "filter callback is not a function"
                    } else {
                        "map callback is not a function"
                    };
                    return Completion::Throw(interp.create_type_error(message));
                }
                (object, length, Elements::Array, callback)
            }
            Transform::TypedArrayMap | Transform::TypedArrayFilter => {
                let (ta, callback) =
                    propagate!(typedarray::extract_ta_and_callback(interp, this, args));
                let length = typed_array_length(&ta);
                (this.clone(), length, Elements::TypedArray(ta), callback)
            }
        };
        let this_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
        // Only TypedArray filter defers species construction until all callbacks
        // finish. Its kept values are primitive Numbers/BigInts, with no GC edges.
        let deferred = matches!(method, Transform::TypedArrayFilter);
        let output = if deferred {
            None
        } else {
            let size = if method.is_filter() { 0 } else { length };
            Some(propagate!(Output::create(interp, &elements, &object, size)))
        };
        let mut kept = Vec::new();
        let mut to = 0;
        for index in 0..length {
            let visit = interp.with_gc_root_scope(|interp| -> Result<(), Completion> {
                let Some(value) = elements.read(interp, &object, index)? else {
                    return Ok(());
                };
                interp.gc_root_value(&value);
                let result = match interp.call_function(
                    &callback,
                    &this_arg,
                    &[value.clone(), JsValue::number(index as f64), object.clone()],
                ) {
                    Completion::Normal(result) => result,
                    other => return Err(other),
                };
                let (destination, value) = if method.is_filter() {
                    if !interp.to_boolean_val(&result) {
                        return Ok(());
                    }
                    let destination = to;
                    to += 1;
                    (destination, value)
                } else {
                    interp.gc_root_value(&result);
                    (index, result)
                };
                if let Some(output) = &output {
                    output.write(interp, destination, value)?;
                } else {
                    kept.push(value);
                }
                Ok(())
            });
            propagate!(visit);
        }
        let output = match output {
            Some(output) => output,
            None => {
                let output = propagate!(Output::create(interp, &elements, &object, kept.len()));
                for (index, value) in kept.into_iter().enumerate() {
                    propagate!(output.write(interp, index, value));
                }
                output
            }
        };
        Completion::Normal(output.into_value())
    })
}

#[cfg(test)]
mod tests;
