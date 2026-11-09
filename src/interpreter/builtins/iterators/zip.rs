//! Shared machinery behind `Iterator.zip` and `Iterator.zipKeyed`: option
//! parsing, one step of the zipped iteration, and helper construction. The
//! two entry points differ only in how they collect sources and shape results.

use super::*;

pub(super) struct ZipOptions {
    pub(super) mode: String,
    pub(super) padding: Option<JsValue>,
}

pub(super) struct ZipSources {
    keys: Vec<JsPropertyKey>,
    iters: Vec<(JsValue, JsValue)>,
    exhausted: Vec<bool>,
    mode: String,
    padding: Vec<JsValue>,
}

impl ZipSources {
    pub(super) fn new(
        keys: Vec<JsPropertyKey>,
        iters: Vec<(JsValue, JsValue)>,
        mode: String,
        padding: Vec<JsValue>,
    ) -> Self {
        let exhausted = vec![false; iters.len()];
        Self {
            keys,
            iters,
            exhausted,
            mode,
            padding,
        }
    }

    fn roots(&self) -> Vec<JsValue> {
        let mut roots = Vec::with_capacity(self.iters.len() * 2 + self.padding.len());
        for (iter, next) in &self.iters {
            roots.push(iter.clone());
            roots.push(next.clone());
        }
        roots.extend(self.padding.iter().cloned());
        roots
    }
}

fn type_error_for_mode(interp: &mut Interpreter) -> Completion {
    Completion::Throw(interp.create_type_error("mode must be 'shortest', 'longest', or 'strict'"))
}

fn get_option(
    interp: &mut Interpreter,
    options: &JsValue,
    name: &str,
) -> Result<JsValue, Completion> {
    let Some(options_id) = options.as_object_id() else {
        return Ok(JsValue::UNDEFINED);
    };
    match interp.get_object_property(options_id, name, options) {
        Completion::Normal(v) => Ok(v),
        Completion::Throw(e) => Err(Completion::Throw(e)),
        _ => Ok(JsValue::UNDEFINED),
    }
}

/// GetOptionsObject plus the `mode` and (for `longest`) `padding` reads.
pub(super) fn parse_zip_options(
    interp: &mut Interpreter,
    options: JsValue,
) -> Result<ZipOptions, Completion> {
    if !options.is_undefined() && !options.is_object() {
        let err = interp.create_type_error("options must be an object or undefined");
        return Err(Completion::Throw(err));
    }

    let mode_val = get_option(interp, &options, "mode")?;
    let mode = if mode_val.is_undefined() {
        "shortest".to_string()
    } else {
        match mode_val.as_string().map(|s| s.to_rust_string()) {
            Some(mode) if matches!(mode.as_str(), "shortest" | "longest" | "strict") => mode,
            _ => return Err(type_error_for_mode(interp)),
        }
    };

    let mut padding = None;
    if mode == "longest" {
        let value = get_option(interp, &options, "padding")?;
        if !value.is_undefined() {
            if !value.is_object() {
                let err = interp.create_type_error("padding must be an object or undefined");
                return Err(Completion::Throw(err));
            }
            interp.gc_root_value(&value);
            padding = Some(value);
        }
    }
    Ok(ZipOptions { mode, padding })
}

fn open_iterators(
    iters: &[(JsValue, JsValue)],
    exhausted: &[bool],
    except: Option<usize>,
) -> Vec<(JsValue, JsValue)> {
    iters
        .iter()
        .enumerate()
        .filter(|(j, _)| !exhausted[*j] && Some(*j) != except)
        .map(|(_, pair)| pair.clone())
        .collect()
}

fn close_after_error(
    interp: &mut Interpreter,
    open: &[(JsValue, JsValue)],
    err: JsValue,
) -> Completion {
    let _ = iterator_close_all(interp, open, Err(err.clone()));
    Completion::Throw(err)
}

/// The remaining sources must also be exhausted once the first one is.
fn check_strict_lengths(
    interp: &mut Interpreter,
    name: &str,
    iters: &[(JsValue, JsValue)],
    exhausted: &mut [bool],
    first_exhausted: usize,
) -> Result<(), Completion> {
    let length_error = |interp: &mut Interpreter, exhausted: &[bool]| {
        let err = interp.create_type_error(&format!(
            "Iterators passed to {name} with {{ mode: \"strict\" }} have different lengths"
        ));
        close_after_error(interp, &open_iterators(iters, exhausted, None), err)
    };
    if first_exhausted != 0 {
        return Err(length_error(interp, exhausted));
    }
    for k in 1..iters.len() {
        if exhausted[k] {
            continue;
        }
        match interp.iterator_step_value_direct(&iters[k].0, &iters[k].1) {
            Ok(None) => exhausted[k] = true,
            Ok(Some(_)) => return Err(length_error(interp, exhausted)),
            Err(e) => {
                exhausted[k] = true;
                return Err(close_after_error(
                    interp,
                    &open_iterators(iters, exhausted, None),
                    e,
                ));
            }
        }
    }
    Ok(())
}

/// One round of values from every source, or `None` once the zip is done.
fn zip_step(
    interp: &mut Interpreter,
    state: &Rc<RefCell<ZipSources>>,
    name: &str,
) -> Result<Option<Vec<JsValue>>, Completion> {
    let (iters, exhausted, mode, padding) = {
        let s = state.borrow();
        (
            s.iters.clone(),
            s.exhausted.clone(),
            s.mode.clone(),
            s.padding.clone(),
        )
    };
    if iters.is_empty() {
        return Ok(None);
    }
    let pad = |i: usize| padding.get(i).cloned().unwrap_or(JsValue::UNDEFINED);

    let mut values = Vec::with_capacity(iters.len());
    let mut new_exhausted = exhausted.clone();

    for (i, (it, nm)) in iters.iter().enumerate() {
        if exhausted[i] {
            values.push(pad(i));
            continue;
        }
        match interp.iterator_step_value_direct(it, nm) {
            Ok(Some(v)) => {
                interp.gc_root_value(&v);
                values.push(v);
            }
            Ok(None) => {
                new_exhausted[i] = true;
                match mode.as_str() {
                    "shortest" => {
                        let open = open_iterators(&iters, &new_exhausted, None);
                        return iterator_close_all(interp, &open, Ok(()))
                            .map(|()| None)
                            .map_err(Completion::Throw);
                    }
                    "strict" => {
                        check_strict_lengths(interp, name, &iters, &mut new_exhausted, i)?;
                        return Ok(None);
                    }
                    _ => values.push(pad(i)),
                }
            }
            Err(e) => {
                state.borrow_mut().exhausted = new_exhausted.clone();
                let open = open_iterators(&iters, &new_exhausted, Some(i));
                return Err(close_after_error(interp, &open, e));
            }
        }
    }

    let all_exhausted = new_exhausted.iter().all(|e| *e);
    state.borrow_mut().exhausted = new_exhausted;
    if mode == "longest" && all_exhausted {
        return Ok(None);
    }
    Ok(Some(values))
}

/// Builds the iterator helper that yields `shape(keys, values)` per round.
pub(super) fn create_zip_helper(
    interp: &mut Interpreter,
    sources: ZipSources,
    name: &'static str,
    shape: fn(&mut Interpreter, &[JsPropertyKey], Vec<JsValue>) -> JsValue,
) -> JsValue {
    let roots = sources.roots();
    let state = Rc::new(RefCell::new(sources));
    let step_state = state.clone();
    interp.create_iterator_helper(
        roots,
        move |interp| {
            interp.with_gc_root_scope(|interp| {
                let Some(values) = zip_step(interp, &step_state, name)? else {
                    return Ok(HelperStep::Done);
                };
                let keys = step_state.borrow().keys.clone();
                Ok(HelperStep::Yield(shape(interp, &keys, values)))
            })
        },
        move |interp| {
            let open = {
                let s = state.borrow();
                open_iterators(&s.iters, &s.exhausted, None)
            };
            iterator_close_all(interp, &open, Ok(()))
        },
    )
}

pub(super) fn shape_zip_array(
    interp: &mut Interpreter,
    _keys: &[JsPropertyKey],
    values: Vec<JsValue>,
) -> JsValue {
    interp.create_array(values)
}

pub(super) fn shape_zip_keyed(
    interp: &mut Interpreter,
    keys: &[JsPropertyKey],
    values: Vec<JsValue>,
) -> JsValue {
    let id = interp.create_object_id();
    let cell = interp.get_object_cell_expect(id);
    let mut object = cell.borrow_mut();
    object.set_proto(None);
    for (key, value) in keys.iter().zip(values) {
        object.insert_property(
            key.clone(),
            PropertyDescriptor::data(value, true, true, true),
        );
    }
    JsValue::object(id)
}
