//! Object introspection for Node host mode (issue #1224): the `__host_util_*`
//! floor primitives behind `util.inspect` and `util.types`. Node reads these
//! from V8 (`internalBinding('util')`/`('types')`); JavaScript cannot see an
//! object's internal slots (a Promise's state, a Proxy's handler, a Map
//! iterator's cursor) without running user code, so each is a narrow,
//! trap-free native. Nothing here exists without `--node`.

use super::super::*;

fn string_value(s: &str) -> JsValue {
    JsValue::string(JsString::from_str(s))
}

fn arg_object<'a>(interp: &'a Interpreter, args: &[JsValue]) -> Option<&'a ObjectHandle> {
    let id = args.first().and_then(JsValue::as_object_id)?;
    interp.get_object_cell(id)
}

fn function_kind(o: &JsObjectData) -> &'static str {
    match o.callable.as_deref() {
        Some(JsFunction::User {
            is_async,
            is_generator,
            ..
        }) => match (*is_async, *is_generator) {
            (true, true) => "AsyncGeneratorFunction",
            (true, false) => "AsyncFunction",
            (false, true) => "GeneratorFunction",
            _ => "",
        },
        _ => "",
    }
}

fn wrapper_kind(o: &JsObjectData) -> &'static str {
    let Some(value) = o.primitive_value() else {
        return "";
    };
    match o.class_name {
        "Boolean" if value.is_boolean() => "BooleanObject",
        "Number" if value.is_number() => "NumberObject",
        "String" if value.is_string() => "StringObject",
        "Symbol" if value.is_symbol() => "SymbolObject",
        "BigInt" if value.is_bigint() => "BigIntObject",
        "Date" if value.is_number() => "Date",
        _ => "",
    }
}

/// The internal-slot brand of an object, as `util.types` distinguishes them;
/// the empty string when it has none of them.
fn object_kind(o: &JsObjectData) -> &'static str {
    if o.proxy().is_some() {
        return "Proxy";
    }
    if o.callable.is_some() {
        return function_kind(o);
    }
    match &o.kind {
        ObjectKind::Map(_) => match o.class_name {
            "Map" => "Map",
            "WeakMap" => "WeakMap",
            _ => "",
        },
        ObjectKind::Set(_) => match o.class_name {
            "Set" => "Set",
            "WeakSet" => "WeakSet",
            _ => "",
        },
        ObjectKind::Promise(_) => "Promise",
        ObjectKind::RegExp(_) => "RegExp",
        ObjectKind::ArrayBuffer(b) if b.is_shared => "SharedArrayBuffer",
        ObjectKind::ArrayBuffer(_) => "ArrayBuffer",
        ObjectKind::DataView(_) => "DataView",
        ObjectKind::ModuleNamespace(_) => "ModuleNamespace",
        ObjectKind::Arguments(_) => "Arguments",
        ObjectKind::Iterator(state) => match &**state {
            IteratorState::MapIterator { .. } => "MapIterator",
            IteratorState::SetIterator { .. } => "SetIterator",
            IteratorState::Generator { .. }
            | IteratorState::StateMachineGenerator { .. }
            | IteratorState::StateMachineAsyncGenerator { .. }
            | IteratorState::BytecodeGenerator(_) => "GeneratorObject",
            _ => "",
        },
        _ => match o.class_name {
            "Arguments" => "Arguments",
            name if super::error_data::has_error_data_slot(name) => "Error",
            _ => wrapper_kind(o),
        },
    }
}

/// Own non-index property keys: `length` of a String wrapper first, then the
/// property map in creation order (String keys, then Symbols). Index
/// properties are never visited, so an Array of a million elements costs
/// only its named properties.
fn own_non_index_keys(
    o: &JsObjectData,
    only_enumerable: bool,
) -> (Vec<JsPropertyKey>, Vec<JsPropertyKey>) {
    let mut strings = Vec::new();
    let mut symbols = Vec::new();
    let is_string_wrapper =
        o.class_name == "String" && o.primitive_value().is_some_and(JsValue::is_string);
    if is_string_wrapper && !only_enumerable {
        strings.push(JsPropertyKey::from("length".to_string()));
    }
    for (key, desc) in o.properties.iter() {
        if only_enumerable && desc.enumerable() == Some(false) {
            continue;
        }
        if key.is_symbol() {
            symbols.push(key.clone());
        } else if parse_array_index(key).is_none() && !(is_string_wrapper && key.eq_str("length")) {
            strings.push(key.clone());
        }
    }
    (strings, symbols)
}

impl Interpreter {
    pub(super) fn setup_node_host_util(&mut self) {
        // __host_util_kind(value) -> brand string, or undefined for a primitive
        let kind_fn = self.create_function(JsFunction::native(
            "__host_util_kind".to_string(),
            1,
            |interp, _this, args| {
                let Some(cell) = arg_object(interp, args) else {
                    return Completion::Normal(JsValue::UNDEFINED);
                };
                let kind = object_kind(&cell.borrow());
                Completion::Normal(string_value(kind))
            },
        ));
        self.install_host_global("__host_util_kind", kind_fn);

        // __host_util_promise_details(promise) -> [state, result] | undefined
        // with state 0 pending, 1 fulfilled, 2 rejected
        let promise_fn = self.create_function(JsFunction::native(
            "__host_util_promise_details".to_string(),
            1,
            |interp, _this, args| {
                let Some(cell) = arg_object(interp, args) else {
                    return Completion::Normal(JsValue::UNDEFINED);
                };
                let details = match &cell.borrow().kind {
                    ObjectKind::Promise(p) => Some(match &p.state {
                        PromiseState::Pending => (0.0, JsValue::UNDEFINED),
                        PromiseState::Fulfilled(v) => (1.0, v.clone()),
                        PromiseState::Rejected(v) => (2.0, v.clone()),
                    }),
                    _ => None,
                };
                Completion::Normal(match details {
                    Some((state, result)) => {
                        interp.create_array(vec![JsValue::number(state), result])
                    }
                    None => JsValue::UNDEFINED,
                })
            },
        ));
        self.install_host_global("__host_util_promise_details", promise_fn);

        // __host_util_proxy_details(value) -> [target, handler] | undefined;
        // both are null once the proxy is revoked
        let proxy_fn = self.create_function(JsFunction::native(
            "__host_util_proxy_details".to_string(),
            1,
            |interp, _this, args| {
                let Some(id) = args.first().and_then(JsValue::as_object_id) else {
                    return Completion::Normal(JsValue::UNDEFINED);
                };
                Completion::Normal(match interp.get_proxy_info(id) {
                    None => JsValue::UNDEFINED,
                    Some((false, Some(target), Some(handler))) => {
                        interp.create_array(vec![JsValue::object(target), JsValue::object(handler)])
                    }
                    Some(_) => interp.create_array(vec![JsValue::NULL, JsValue::NULL]),
                })
            },
        ));
        self.install_host_global("__host_util_proxy_details", proxy_fn);

        // __host_util_preview_entries(object, isIterator): the entries of a
        // Map, Set, WeakMap or WeakSet (flat key, value pairs for the maps),
        // or with isIterator the entries a Map/Set iterator has yet to yield
        // as [entries, isKeyValue]. Reads state only; nothing is consumed.
        let preview_fn = self.create_function(JsFunction::native(
            "__host_util_preview_entries".to_string(),
            2,
            |interp, _this, args| {
                let Some(cell) = arg_object(interp, args) else {
                    return Completion::Normal(JsValue::UNDEFINED);
                };
                let iterator = args.get(1).is_some_and(to_boolean);
                let preview = preview_entries(interp, cell, iterator);
                Completion::Normal(match preview {
                    Some((entries, key_value)) => {
                        let list = interp.create_array(entries);
                        if iterator {
                            interp.create_array(vec![list, JsValue::boolean(key_value)])
                        } else {
                            list
                        }
                    }
                    None => JsValue::UNDEFINED,
                })
            },
        ));
        self.install_host_global("__host_util_preview_entries", preview_fn);

        // __host_util_own_non_index(object, onlyEnumerable) -> Array of keys
        let keys_fn = self.create_function(JsFunction::native(
            "__host_util_own_non_index".to_string(),
            2,
            |interp, _this, args| {
                let Some(cell) = arg_object(interp, args) else {
                    return Completion::Normal(interp.create_array(Vec::new()));
                };
                let only_enumerable = args.get(1).is_some_and(to_boolean);
                let (strings, symbols) = own_non_index_keys(&cell.borrow(), only_enumerable);
                let mut keys: Vec<JsValue> = strings
                    .iter()
                    .map(|k| JsValue::string(crate::interpreter::key_intern::key_to_js_string(k)))
                    .collect();
                for symbol in &symbols {
                    keys.push(interp.symbol_key_to_jsvalue(symbol));
                }
                Completion::Normal(interp.create_array(keys))
            },
        ));
        self.install_host_global("__host_util_own_non_index", keys_fn);

        // __host_util_class_name(object) -> the engine's class of the object,
        // the stand-in for V8's JSReceiver::GetConstructorName
        let class_fn = self.create_function(JsFunction::native(
            "__host_util_class_name".to_string(),
            1,
            |interp, _this, args| {
                let Some(cell) = arg_object(interp, args) else {
                    return Completion::Normal(JsValue::UNDEFINED);
                };
                let o = cell.borrow();
                let name = if o.callable.is_some() {
                    "Function"
                } else {
                    o.class_name
                };
                Completion::Normal(string_value(name))
            },
        ));
        self.install_host_global("__host_util_class_name", class_fn);
    }
}

fn preview_entries(
    interp: &Interpreter,
    cell: &ObjectHandle,
    iterator: bool,
) -> Option<(Vec<JsValue>, bool)> {
    let o = cell.borrow();
    if !iterator {
        return match &o.kind {
            ObjectKind::Map(m) => Some((
                m.iter().flat_map(|(k, v)| [k.clone(), v.clone()]).collect(),
                true,
            )),
            ObjectKind::Set(s) => Some((s.iter().cloned().collect(), false)),
            _ => None,
        };
    }
    let ObjectKind::Iterator(state) = &o.kind else {
        return None;
    };
    match &**state {
        IteratorState::MapIterator {
            map_id,
            index,
            kind,
            done,
        } => {
            let key_value = matches!(kind, IteratorKind::KeyValue);
            if *done {
                return Some((Vec::new(), key_value));
            }
            let map = interp.get_object_cell(*map_id)?.borrow();
            let ObjectKind::Map(entries) = &map.kind else {
                return None;
            };
            let mut cursor = *index;
            let mut out = Vec::new();
            while let Some((k, v)) = entries.next_live(&mut cursor) {
                match kind {
                    IteratorKind::Key => out.push(k.clone()),
                    IteratorKind::Value => out.push(v.clone()),
                    IteratorKind::KeyValue => {
                        out.push(k.clone());
                        out.push(v.clone());
                    }
                }
            }
            Some((out, key_value))
        }
        IteratorState::SetIterator {
            set_id,
            index,
            kind,
            done,
        } => {
            let key_value = matches!(kind, IteratorKind::KeyValue);
            if *done {
                return Some((Vec::new(), key_value));
            }
            let set = interp.get_object_cell(*set_id)?.borrow();
            let ObjectKind::Set(entries) = &set.kind else {
                return None;
            };
            let mut cursor = *index;
            let mut out = Vec::new();
            while let Some(v) = entries.next_live(&mut cursor) {
                out.push(v.clone());
                if key_value {
                    out.push(v.clone());
                }
            }
            Some((out, key_value))
        }
        _ => None,
    }
}
