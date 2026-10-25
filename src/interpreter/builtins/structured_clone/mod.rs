//! `structuredClone(value [, options])` — the HTML StructuredSerialize /
//! StructuredDeserialize pair fused into one pass, since the intermediate
//! record is unobservable. Handles the ECMAScript types this engine has; every
//! other object throws a `DataCloneError`.

mod buffers;

use super::super::*;
use super::array::create_data_property_or_throw;
use super::errors::error_class_name;
use crate::interpreter::helpers::enumerable_own_keys;
use crate::interpreter::types::{DataViewInfo, ObjectKind, TypedArrayInfo};

const NATIVE_ERROR_NAMES: [&str; 7] = [
    "Error",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
];

type CloneResult = Result<JsValue, JsValue>;

/// What a source object is, decided under one borrow so that no user code
/// runs while the object is borrowed.
enum Shape {
    Reject,
    Wrapper(JsValue),
    Date(JsValue),
    RegExp(JsString, JsString),
    Buffer,
    TypedArray(TypedArrayInfo),
    DataView(DataViewInfo),
    Map(Vec<(JsValue, JsValue)>),
    Set(Vec<JsValue>),
    Error,
    Array,
    Plain,
}

fn classify(interp: &Interpreter, id: u64) -> Shape {
    let Some(cell) = interp.get_object_cell(id) else {
        return Shape::Reject;
    };
    let b = cell.borrow();
    if b.callable.is_some() || b.proxy().is_some() {
        return Shape::Reject;
    }
    match &b.kind {
        ObjectKind::RegExp(data) => Shape::RegExp(data.source.clone(), data.flags.clone()),
        ObjectKind::ArrayBuffer(_) => Shape::Buffer,
        ObjectKind::TypedArray(info) => Shape::TypedArray((**info).clone()),
        ObjectKind::DataView(info) => Shape::DataView((**info).clone()),
        ObjectKind::Map(entries) if !b.is_weak_map() => {
            Shape::Map(entries.iter().cloned().collect())
        }
        ObjectKind::Set(entries) if !b.is_weak_set() => {
            Shape::Set(entries.iter().cloned().collect())
        }
        ObjectKind::Array(_) => Shape::Array,
        ObjectKind::Ordinary | ObjectKind::PrimitiveWrapper(_) => {
            match (b.class_name, &b.primitive_value) {
                ("Date", Some(t)) if t.is_number() => Shape::Date(t.clone()),
                ("String" | "Number" | "Boolean" | "BigInt", Some(p)) => Shape::Wrapper(p.clone()),
                (name, _) if NATIVE_ERROR_NAMES.contains(&name) => Shape::Error,
                ("AggregateError" | "SuppressedError", _) => Shape::Error,
                ("Object" | "Math", _) if matches!(b.kind, ObjectKind::Ordinary) => Shape::Plain,
                _ => Shape::Reject,
            }
        }
        _ => Shape::Reject,
    }
}

impl Interpreter {
    fn data_clone_error(&mut self, message: &str) -> JsValue {
        self.create_dom_exception("DataCloneError", 25, message)
    }

    fn new_object_with(
        &mut self,
        class_name: &'static str,
        proto: Option<u64>,
        kind: ObjectKind,
    ) -> u64 {
        let id = self.create_object_id();
        let mut o = self.get_object_cell_expect(id).borrow_mut();
        o.class_name = class_name;
        o.prototype_id = proto;
        o.kind = kind;
        drop(o);
        id
    }
}

struct Cloner {
    memory: HashMap<u64, u64>,
    depth: usize,
}

impl Cloner {
    /// Record `clone` as the image of `source` and keep both alive until the
    /// enclosing root scope ends: a getter or toString run later in the walk
    /// can collect, and neither id is reachable from anywhere else yet.
    fn register(&mut self, interp: &mut Interpreter, source: u64, clone: u64) {
        self.memory.insert(source, clone);
        interp.gc_root_id(source);
        interp.gc_root_id(clone);
    }

    fn clone_value(&mut self, interp: &mut Interpreter, value: &JsValue) -> CloneResult {
        if value.is_symbol() {
            return Err(interp.data_clone_error(&format!("{value} could not be cloned.")));
        }
        let Some(id) = value.as_object_id() else {
            return Ok(value.clone());
        };
        if let Some(&clone) = self.memory.get(&id) {
            return Ok(JsValue::object(clone));
        }
        if interp.call_depth + self.depth >= CALL_DEPTH_SOFT_LIMIT {
            return Err(interp.create_range_error("Maximum call stack size exceeded"));
        }
        self.depth += 1;
        let result = self.clone_object(interp, id);
        self.depth -= 1;
        result
    }

    fn clone_object(&mut self, interp: &mut Interpreter, id: u64) -> CloneResult {
        let realm = interp.realm();
        let (object_proto, date_proto, regexp_proto, map_proto, set_proto) = (
            realm.object_prototype,
            realm.date_prototype,
            realm.regexp_prototype,
            realm.map_prototype,
            realm.set_prototype,
        );
        let clone = match classify(interp, id) {
            Shape::Reject => {
                return Err(interp.data_clone_error("#<Object> could not be cloned."));
            }
            Shape::Wrapper(prim) => {
                if prim.is_symbol() {
                    return Err(interp.data_clone_error("#<Symbol> could not be cloned."));
                }
                let Completion::Normal(wrapper) = interp.to_object(&prim) else {
                    return Err(interp.data_clone_error("#<Object> could not be cloned."));
                };
                let clone = wrapper.as_object_id().unwrap_or(id);
                self.register(interp, id, clone);
                clone
            }
            Shape::Date(time) => {
                let clone = interp.new_object_with("Date", date_proto, ObjectKind::Ordinary);
                interp
                    .get_object_cell_expect(clone)
                    .borrow_mut()
                    .primitive_value = Some(time);
                self.register(interp, id, clone);
                clone
            }
            Shape::RegExp(source, flags) => {
                let data = crate::interpreter::types::RegExpData::new(source, flags);
                let clone =
                    interp.new_object_with("RegExp", regexp_proto, ObjectKind::RegExp(data));
                interp
                    .get_object_cell_expect(clone)
                    .borrow_mut()
                    .insert_property(
                        "lastIndex".to_string(),
                        PropertyDescriptor::data(JsValue::number(0.0), true, false, false),
                    );
                self.register(interp, id, clone);
                clone
            }
            Shape::Buffer => self.clone_buffer(interp, id)?,
            Shape::TypedArray(info) => self.clone_typed_array(interp, id, info)?,
            Shape::DataView(info) => self.clone_data_view(interp, id, info)?,
            Shape::Map(entries) => {
                let clone =
                    interp.new_object_with("Map", map_proto, ObjectKind::Map(Box::default()));
                self.register(interp, id, clone);
                for (k, v) in &entries {
                    interp.gc_root_value(k);
                    interp.gc_root_value(v);
                }
                for (k, v) in entries {
                    let entry = (self.clone_value(interp, &k)?, self.clone_value(interp, &v)?);
                    if let Some(map) = interp
                        .get_object_cell_expect(clone)
                        .borrow_mut()
                        .map_data_mut()
                    {
                        map.upsert(entry, |old, new| *old = new);
                    }
                }
                clone
            }
            Shape::Set(entries) => {
                let clone =
                    interp.new_object_with("Set", set_proto, ObjectKind::Set(Box::default()));
                self.register(interp, id, clone);
                for v in &entries {
                    interp.gc_root_value(v);
                }
                for v in entries {
                    let entry = self.clone_value(interp, &v)?;
                    if let Some(set) = interp
                        .get_object_cell_expect(clone)
                        .borrow_mut()
                        .set_data_mut()
                    {
                        set.upsert(entry, |_, _| {});
                    }
                }
                clone
            }
            Shape::Error => return self.clone_error(interp, id),
            Shape::Array => {
                let source = JsValue::object(id);
                let len = match interp.get_object_property(id, "length", &source) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::UNDEFINED,
                };
                let array = interp.create_array(Vec::new());
                let clone = array.as_object_id().unwrap_or(id);
                self.register(interp, id, clone);
                interp.set_object_property(clone, "length", len, &array)?;
                self.copy_properties(interp, id, clone)?;
                clone
            }
            Shape::Plain => {
                let clone = interp.new_object_with("Object", object_proto, ObjectKind::Ordinary);
                self.register(interp, id, clone);
                self.copy_properties(interp, id, clone)?;
                clone
            }
        };
        Ok(JsValue::object(clone))
    }

    /// Copy `source`'s own enumerable String-keyed properties onto `clone` as
    /// data properties, reading each through [[Get]] so accessors run.
    fn copy_properties(
        &mut self,
        interp: &mut Interpreter,
        source: u64,
        clone: u64,
    ) -> Result<(), JsValue> {
        let source_val = JsValue::object(source);
        let clone_val = JsValue::object(clone);
        for key in enumerable_own_keys(interp, source)? {
            let still_own = interp
                .get_object_cell(source)
                .is_some_and(|cell| cell.borrow().get_own_property(&key).is_some());
            if !still_own {
                continue;
            }
            let value = match interp.get_object_property(source, &key, &source_val) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::UNDEFINED,
            };
            let cloned = self.clone_value(interp, &value)?;
            create_data_property_or_throw(interp, &clone_val, &key, cloned)?;
        }
        Ok(())
    }

    fn clone_error(&mut self, interp: &mut Interpreter, id: u64) -> CloneResult {
        let source = JsValue::object(id);
        let name_val = match interp.get_object_property(id, "name", &source) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Err(e),
            _ => JsValue::UNDEFINED,
        };
        let name = name_val
            .as_string()
            .map(|s| s.to_rust_string())
            .filter(|n| NATIVE_ERROR_NAMES.contains(&n.as_str()))
            .unwrap_or_else(|| "Error".to_string());
        let realm = interp.realm();
        let proto = match name.as_str() {
            "EvalError" => realm.eval_error_prototype,
            "RangeError" => realm.range_error_prototype,
            "ReferenceError" => realm.reference_error_prototype,
            "SyntaxError" => realm.syntax_error_prototype,
            "TypeError" => realm.type_error_prototype,
            "URIError" => realm.uri_error_prototype,
            _ => realm.error_prototype,
        };
        let clone = interp.new_object_with(error_class_name(&name), proto, ObjectKind::Ordinary);
        self.register(interp, id, clone);

        let own_data = |interp: &Interpreter, key: &str| {
            interp
                .get_object_cell(id)
                .and_then(|cell| cell.borrow().get_own_property(key))
                .and_then(|desc| desc.value)
        };
        if let Some(stack) = own_data(interp, "stack").filter(JsValue::is_string) {
            interp
                .get_object_cell_expect(clone)
                .borrow_mut()
                .insert_builtin("stack".to_string(), stack);
        }
        if let Some(message) = own_data(interp, "message") {
            let text = interp.to_js_string(&message)?;
            interp
                .get_object_cell_expect(clone)
                .borrow_mut()
                .insert_builtin("message".to_string(), JsValue::string(text));
        }
        if let Some(cause) = own_data(interp, "cause") {
            interp.gc_root_value(&cause);
            let cloned = self.clone_value(interp, &cause)?;
            interp
                .get_object_cell_expect(clone)
                .borrow_mut()
                .insert_builtin("cause".to_string(), cloned);
        }
        Ok(JsValue::object(clone))
    }
}

/// Validate and pre-clone the `transfer` list. Each ArrayBuffer is copied into
/// the memory map up front so every view of it resolves to the transferred
/// buffer; the sources are detached only once the whole clone has succeeded.
fn prepare_transfer(
    interp: &mut Interpreter,
    cloner: &mut Cloner,
    items: &[JsValue],
) -> Result<Vec<JsValue>, JsValue> {
    let mut sources = Vec::new();
    for item in items {
        let Some(id) = item.as_object_id() else {
            return Err(interp.create_type_error("transfer list entries must be objects"));
        };
        if cloner.memory.contains_key(&id) {
            return Err(interp.data_clone_error("Transfer list contains duplicate ArrayBuffer"));
        }
        let is_buffer = interp.get_object_cell(id).is_some_and(|cell| {
            let b = cell.borrow();
            b.arraybuffer_data().is_some() && !b.arraybuffer_is_shared()
        });
        if !is_buffer {
            return Err(interp.data_clone_error("Cannot transfer object of unsupported type."));
        }
        cloner.clone_buffer(interp, id)?;
        sources.push(item.clone());
    }
    Ok(sources)
}

fn structured_clone(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    if args.is_empty() {
        return Completion::Throw(
            interp.create_type_error("structuredClone: 1 argument required, but only 0 present."),
        );
    }
    interp.with_gc_root_scope(|interp| match run_clone(interp, args) {
        Ok(v) => Completion::Normal(v),
        Err(e) => Completion::Throw(e),
    })
}

fn transfer_list(interp: &mut Interpreter, options: &JsValue) -> Result<Vec<JsValue>, JsValue> {
    if options.is_nullish() {
        return Ok(Vec::new());
    }
    let Some(options_id) = options.as_object_id() else {
        return Err(interp.create_type_error("Options cannot be converted to a dictionary"));
    };
    let list = match interp.get_object_property(options_id, "transfer", options) {
        Completion::Normal(v) => v,
        Completion::Throw(e) => return Err(e),
        _ => JsValue::UNDEFINED,
    };
    if list.is_undefined() {
        return Ok(Vec::new());
    }
    if !list.is_object() {
        return Err(interp.create_type_error("transfer in Options cannot be converted to sequence"));
    }
    interp.gc_root_value(&list);
    let items = interp.iterate_to_vec(&list)?;
    for item in &items {
        interp.gc_root_value(item);
    }
    Ok(items)
}

fn run_clone(interp: &mut Interpreter, args: &[JsValue]) -> CloneResult {
    let value = &args[0];
    interp.gc_root_value(value);
    let items = transfer_list(interp, args.get(1).unwrap_or(JsValue::undefined_ref()))?;
    let mut cloner = Cloner {
        memory: HashMap::new(),
        depth: 0,
    };
    let transferred = prepare_transfer(interp, &mut cloner, &items)?;
    let result = cloner.clone_value(interp, value)?;
    for buffer in &transferred {
        if let Completion::Throw(e) = interp.detach_arraybuffer(buffer) {
            return Err(e);
        }
    }
    Ok(result)
}

impl Interpreter {
    pub(super) fn setup_structured_clone(&mut self) {
        self.register_global_fn(
            "structuredClone",
            BindingKind::Var,
            JsFunction::native("structuredClone".to_string(), 1, structured_clone),
        );
    }
}
