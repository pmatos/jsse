use crate::ast::*;
use crate::interpreter::helpers::same_value;
use crate::types::{JsString, JsValue};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug)]
pub enum Completion {
    Normal(JsValue),
    Return(JsValue),
    Throw(JsValue),
    Break(Option<String>),
    Continue(Option<String>),
    Yield(JsValue),
}

impl Completion {
    pub(crate) fn is_abrupt(&self) -> bool {
        !matches!(self, Completion::Normal(_))
    }
}

pub(crate) struct GeneratorContext {
    pub(crate) target_yield: usize,
    pub(crate) current_yield: usize,
    pub(crate) sent_value: JsValue,
    pub(crate) is_async: bool,
}

pub type EnvRef = Rc<RefCell<Environment>>;

pub struct Environment {
    pub(crate) bindings: HashMap<String, Binding>,
    pub(crate) parent: Option<EnvRef>,
    pub strict: bool,
    pub(crate) with_object: Option<WithObject>,
}

impl std::fmt::Debug for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Environment")
            .field("bindings", &self.bindings)
            .field("strict", &self.strict)
            .field("has_with_object", &self.with_object.is_some())
            .finish()
    }
}

pub(crate) struct WithObject {
    pub(crate) object: Rc<RefCell<JsObjectData>>,
    pub(crate) unscopables: Option<Rc<RefCell<JsObjectData>>>,
}

#[derive(Debug, Clone)]
pub(crate) struct Binding {
    pub(crate) value: JsValue,
    pub(crate) kind: BindingKind,
    pub(crate) initialized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum BindingKind {
    Var,
    Let,
    Const,
}

impl Environment {
    pub fn new(parent: Option<EnvRef>) -> EnvRef {
        let strict = parent.as_ref().is_some_and(|p| p.borrow().strict);
        Rc::new(RefCell::new(Environment {
            bindings: HashMap::new(),
            parent,
            strict,
            with_object: None,
        }))
    }

    pub fn declare(&mut self, name: &str, kind: BindingKind) {
        self.bindings.insert(
            name.to_string(),
            Binding {
                value: JsValue::Undefined,
                kind,
                initialized: kind == BindingKind::Var,
            },
        );
    }

    pub fn set(&mut self, name: &str, value: JsValue) -> Result<(), JsValue> {
        if let Some(ref with_obj) = self.with_object {
            let obj = with_obj.object.borrow();
            if obj.has_property(name) && !Self::is_unscopable(with_obj, name) {
                drop(obj);
                with_obj.object.borrow_mut().set_property_value(name, value);
                return Ok(());
            }
        }
        if let Some(binding) = self.bindings.get_mut(name) {
            if binding.kind == BindingKind::Const && binding.initialized {
                return Err(JsValue::String(JsString::from_str(
                    "Assignment to constant variable.",
                )));
            }
            binding.value = value;
            binding.initialized = true;
            Ok(())
        } else if let Some(parent) = &self.parent {
            parent.borrow_mut().set(name, value)
        } else {
            // Global implicit declaration (sloppy mode)
            self.bindings.insert(
                name.to_string(),
                Binding {
                    value,
                    kind: BindingKind::Var,
                    initialized: true,
                },
            );
            Ok(())
        }
    }

    fn is_unscopable(with: &WithObject, name: &str) -> bool {
        if let Some(ref unscopables) = with.unscopables {
            let u = unscopables.borrow();
            if let Some(desc) = u.properties.get(name)
                && let Some(ref val) = desc.value
            {
                return match val {
                    JsValue::Undefined | JsValue::Null => false,
                    JsValue::Boolean(b) => *b,
                    JsValue::Number(n) => *n != 0.0 && !n.is_nan(),
                    JsValue::String(s) => !s.is_empty(),
                    _ => true,
                };
            }
        }
        false
    }

    pub fn get(&self, name: &str) -> Option<JsValue> {
        if let Some(ref with) = self.with_object {
            let obj = with.object.borrow();
            if obj.has_property(name) && !Self::is_unscopable(with, name) {
                return Some(obj.get_property(name));
            }
        }
        if let Some(binding) = self.bindings.get(name) {
            if !binding.initialized {
                return None; // TDZ
            }
            Some(binding.value.clone())
        } else if let Some(parent) = &self.parent {
            parent.borrow().get(name)
        } else {
            None
        }
    }

    pub fn has(&self, name: &str) -> bool {
        if let Some(ref with) = self.with_object {
            let obj = with.object.borrow();
            if obj.has_property(name) && !Self::is_unscopable(with, name) {
                return true;
            }
        }
        if self.bindings.contains_key(name) {
            true
        } else if let Some(parent) = &self.parent {
            parent.borrow().has(name)
        } else {
            false
        }
    }
}

pub enum JsFunction {
    User {
        name: Option<String>,
        params: Vec<Pattern>,
        body: Vec<Statement>,
        closure: EnvRef,
        is_arrow: bool,
        is_strict: bool,
        is_generator: bool,
        is_async: bool,
    },
    Native(
        String,
        usize,
        Rc<dyn Fn(&mut super::Interpreter, &JsValue, &[JsValue]) -> Completion>,
        bool, // is_constructor
    ),
}

impl JsFunction {
    pub fn native(
        name: String,
        arity: usize,
        f: impl Fn(&mut super::Interpreter, &JsValue, &[JsValue]) -> Completion + 'static,
    ) -> Self {
        JsFunction::Native(name, arity, Rc::new(f), false)
    }

    pub fn constructor(
        name: String,
        arity: usize,
        f: impl Fn(&mut super::Interpreter, &JsValue, &[JsValue]) -> Completion + 'static,
    ) -> Self {
        JsFunction::Native(name, arity, Rc::new(f), true)
    }
}

impl Clone for JsFunction {
    fn clone(&self) -> Self {
        match self {
            JsFunction::User {
                name,
                params,
                body,
                closure,
                is_arrow,
                is_strict,
                is_generator,
                is_async,
            } => JsFunction::User {
                name: name.clone(),
                params: params.clone(),
                body: body.clone(),
                closure: closure.clone(),
                is_arrow: *is_arrow,
                is_strict: *is_strict,
                is_generator: *is_generator,
                is_async: *is_async,
            },
            JsFunction::Native(name, arity, f, is_ctor) => {
                JsFunction::Native(name.clone(), *arity, f.clone(), *is_ctor)
            }
        }
    }
}

impl std::fmt::Debug for JsFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsFunction::User { name, .. } => write!(f, "JsFunction::User({name:?})"),
            JsFunction::Native(name, arity, _, is_ctor) => {
                write!(f, "JsFunction::Native({name:?}, {arity}, ctor={is_ctor})")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct PropertyDescriptor {
    pub value: Option<JsValue>,
    pub writable: Option<bool>,
    pub get: Option<JsValue>,
    pub set: Option<JsValue>,
    pub enumerable: Option<bool>,
    pub configurable: Option<bool>,
}

impl PropertyDescriptor {
    pub fn data(value: JsValue, writable: bool, enumerable: bool, configurable: bool) -> Self {
        Self {
            value: Some(value),
            writable: Some(writable),
            get: None,
            set: None,
            enumerable: Some(enumerable),
            configurable: Some(configurable),
        }
    }

    pub fn data_default(value: JsValue) -> Self {
        Self::data(value, true, true, true)
    }

    pub fn is_data_descriptor(&self) -> bool {
        self.value.is_some() || self.writable.is_some()
    }

    pub fn is_accessor_descriptor(&self) -> bool {
        self.get.is_some() || self.set.is_some()
    }
}

#[derive(Debug, Clone)]
pub enum PrivateFieldDef {
    Field {
        name: String,
        initializer: Option<Expression>,
    },
    Method {
        name: String,
        value: JsValue,
    },
    Accessor {
        name: String,
        get: Option<JsValue>,
        set: Option<JsValue>,
    },
}

#[derive(Debug, Clone)]
pub enum PrivateElement {
    Field(JsValue),
    Method(JsValue),
    Accessor {
        get: Option<JsValue>,
        set: Option<JsValue>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IteratorKind {
    Key,
    Value,
    KeyValue,
}

#[derive(Debug, Clone)]
pub enum IteratorState {
    ArrayIterator {
        array_id: u64,
        index: usize,
        kind: IteratorKind,
        done: bool,
    },
    StringIterator {
        string: JsString,
        position: usize,
        done: bool,
    },
    MapIterator {
        map_id: u64,
        index: usize,
        kind: IteratorKind,
        done: bool,
    },
    SetIterator {
        set_id: u64,
        index: usize,
        kind: IteratorKind,
        done: bool,
    },
    Generator {
        body: Vec<Statement>,
        params: Vec<Pattern>,
        closure: EnvRef,
        is_strict: bool,
        args: Vec<JsValue>,
        this_val: JsValue,
        target_yield: usize,
        done: bool,
    },
    AsyncGenerator {
        body: Vec<Statement>,
        params: Vec<Pattern>,
        closure: EnvRef,
        is_strict: bool,
        args: Vec<JsValue>,
        this_val: JsValue,
        target_yield: usize,
        done: bool,
    },
    RegExpStringIterator {
        source: String,
        flags: String,
        string: String,
        global: bool,
        last_index: usize,
        done: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TypedArrayKind {
    Int8,
    Uint8,
    Uint8Clamped,
    Int16,
    Uint16,
    Int32,
    Uint32,
    Float32,
    Float64,
    BigInt64,
    BigUint64,
}

impl TypedArrayKind {
    pub fn bytes_per_element(&self) -> usize {
        match self {
            TypedArrayKind::Int8 | TypedArrayKind::Uint8 | TypedArrayKind::Uint8Clamped => 1,
            TypedArrayKind::Int16 | TypedArrayKind::Uint16 => 2,
            TypedArrayKind::Int32 | TypedArrayKind::Uint32 | TypedArrayKind::Float32 => 4,
            TypedArrayKind::Float64 | TypedArrayKind::BigInt64 | TypedArrayKind::BigUint64 => 8,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            TypedArrayKind::Int8 => "Int8Array",
            TypedArrayKind::Uint8 => "Uint8Array",
            TypedArrayKind::Uint8Clamped => "Uint8ClampedArray",
            TypedArrayKind::Int16 => "Int16Array",
            TypedArrayKind::Uint16 => "Uint16Array",
            TypedArrayKind::Int32 => "Int32Array",
            TypedArrayKind::Uint32 => "Uint32Array",
            TypedArrayKind::Float32 => "Float32Array",
            TypedArrayKind::Float64 => "Float64Array",
            TypedArrayKind::BigInt64 => "BigInt64Array",
            TypedArrayKind::BigUint64 => "BigUint64Array",
        }
    }

    pub fn is_bigint(&self) -> bool {
        matches!(self, TypedArrayKind::BigInt64 | TypedArrayKind::BigUint64)
    }
}

#[derive(Debug, Clone)]
pub struct TypedArrayInfo {
    pub kind: TypedArrayKind,
    pub buffer: Rc<RefCell<Vec<u8>>>,
    pub byte_offset: usize,
    pub byte_length: usize,
    pub array_length: usize,
}

#[derive(Debug, Clone)]
pub struct DataViewInfo {
    pub buffer: Rc<RefCell<Vec<u8>>>,
    pub byte_offset: usize,
    pub byte_length: usize,
}

pub struct JsObjectData {
    pub id: Option<u64>,
    pub properties: HashMap<String, PropertyDescriptor>,
    pub property_order: Vec<String>,
    pub prototype: Option<Rc<RefCell<JsObjectData>>>,
    pub callable: Option<JsFunction>,
    pub array_elements: Option<Vec<JsValue>>,
    pub class_name: String,
    pub extensible: bool,
    pub primitive_value: Option<JsValue>,
    pub private_fields: HashMap<String, PrivateElement>,
    pub class_private_field_defs: Vec<PrivateFieldDef>,
    pub class_public_field_defs: Vec<(String, Option<crate::ast::Expression>)>,
    pub iterator_state: Option<IteratorState>,
    pub parameter_map: Option<HashMap<String, (EnvRef, String)>>,
    pub map_data: Option<Vec<Option<(JsValue, JsValue)>>>,
    pub set_data: Option<Vec<Option<JsValue>>>,
    pub proxy_target: Option<Rc<RefCell<JsObjectData>>>,
    pub proxy_handler: Option<Rc<RefCell<JsObjectData>>>,
    pub proxy_revoked: bool,
    pub arraybuffer_data: Option<Rc<RefCell<Vec<u8>>>>,
    pub typed_array_info: Option<TypedArrayInfo>,
    pub data_view_info: Option<DataViewInfo>,
    pub promise_data: Option<PromiseData>,
    pub is_raw_json: bool,
}

impl JsObjectData {
    pub(crate) fn new() -> Self {
        Self {
            id: None,
            properties: HashMap::new(),
            property_order: Vec::new(),
            prototype: None,
            callable: None,
            array_elements: None,
            class_name: "Object".to_string(),
            extensible: true,
            primitive_value: None,
            private_fields: HashMap::new(),
            class_private_field_defs: Vec::new(),
            class_public_field_defs: Vec::new(),
            iterator_state: None,
            parameter_map: None,
            map_data: None,
            set_data: None,
            proxy_target: None,
            proxy_handler: None,
            proxy_revoked: false,
            arraybuffer_data: None,
            typed_array_info: None,
            data_view_info: None,
            promise_data: None,
            is_raw_json: false,
        }
    }

    pub fn is_proxy(&self) -> bool {
        self.proxy_target.is_some()
    }

    pub fn get_property(&self, key: &str) -> JsValue {
        if let Some(ref map) = self.parameter_map
            && let Some((env_ref, param_name)) = map.get(key)
            && let Some(val) = env_ref.borrow().get(param_name)
        {
            return val;
        }
        if let Some(desc) = self.properties.get(key) {
            if let Some(ref val) = desc.value {
                return val.clone();
            }
            return JsValue::Undefined;
        }
        if let Some(ref elems) = self.array_elements
            && let Ok(idx) = key.parse::<usize>()
            && idx < elems.len()
        {
            return elems[idx].clone();
        }
        if let Some(ref ta) = self.typed_array_info
            && let Ok(idx) = key.parse::<usize>()
        {
            if idx < ta.array_length {
                return typed_array_get_index(ta, idx);
            }
            return JsValue::Undefined;
        }
        if let Some(proto) = &self.prototype {
            return proto.borrow().get_property(key);
        }
        JsValue::Undefined
    }

    pub fn get_property_descriptor(&self, key: &str) -> Option<PropertyDescriptor> {
        if let Some(desc) = self.properties.get(key) {
            let mut d = desc.clone();
            if let Some(ref map) = self.parameter_map
                && let Some((env_ref, param_name)) = map.get(key)
                && let Some(val) = env_ref.borrow().get(param_name)
            {
                d.value = Some(val);
            }
            return Some(d);
        }
        if let Some(ref elems) = self.array_elements
            && let Ok(idx) = key.parse::<usize>()
            && idx < elems.len()
        {
            return Some(PropertyDescriptor {
                value: Some(elems[idx].clone()),
                writable: Some(true),
                enumerable: Some(true),
                configurable: Some(true),
                get: None,
                set: None,
            });
        }
        if let Some(ref ta) = self.typed_array_info
            && let Ok(idx) = key.parse::<usize>()
        {
            if idx < ta.array_length {
                return Some(PropertyDescriptor {
                    value: Some(typed_array_get_index(ta, idx)),
                    writable: Some(true),
                    enumerable: Some(true),
                    configurable: Some(true),
                    get: None,
                    set: None,
                });
            }
            return None;
        }
        if let Some(proto) = &self.prototype {
            return proto.borrow().get_property_descriptor(key);
        }
        None
    }

    pub fn get_own_property(&self, key: &str) -> Option<&PropertyDescriptor> {
        self.properties.get(key)
    }

    pub fn has_own_property(&self, key: &str) -> bool {
        self.properties.contains_key(key)
    }

    pub fn enumerable_keys_with_proto(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut keys = Vec::new();
        // Own enumerable properties (in insertion order)
        for k in &self.property_order {
            if let Some(desc) = self.properties.get(k)
                && desc.enumerable != Some(false)
                && seen.insert(k.clone())
            {
                keys.push(k.clone());
            }
        }
        // Prototype chain
        if let Some(ref proto) = self.prototype {
            for k in proto.borrow().enumerable_keys_with_proto() {
                if seen.insert(k.clone()) {
                    keys.push(k);
                }
            }
        }
        keys
    }

    pub fn has_property(&self, key: &str) -> bool {
        if self.properties.contains_key(key) {
            return true;
        }
        if let Some(proto) = &self.prototype {
            return proto.borrow().has_property(key);
        }
        false
    }

    pub fn define_own_property(&mut self, key: String, desc: PropertyDescriptor) -> bool {
        if let Some(current) = self.properties.get(&key).cloned() {
            // §10.1.6.3 step 2: if every field of desc is absent, return true
            if desc.value.is_none()
                && desc.writable.is_none()
                && desc.get.is_none()
                && desc.set.is_none()
                && desc.enumerable.is_none()
                && desc.configurable.is_none()
            {
                return true;
            }

            if current.configurable == Some(false) {
                if desc.configurable == Some(true) {
                    return false;
                }
                if desc.enumerable.is_some() && desc.enumerable != current.enumerable {
                    return false;
                }

                let current_is_data = current.is_data_descriptor();
                let current_is_accessor = current.is_accessor_descriptor();
                let desc_is_data = desc.is_data_descriptor();
                let desc_is_accessor = desc.is_accessor_descriptor();

                // Cannot change between data and accessor on non-configurable
                if current_is_data && !current_is_accessor && desc_is_accessor && !desc_is_data {
                    return false;
                }
                if current_is_accessor && !current_is_data && desc_is_data && !desc_is_accessor {
                    return false;
                }

                if current_is_data && !current_is_accessor {
                    // Non-configurable data property
                    if current.writable == Some(false) {
                        if desc.writable == Some(true) {
                            return false;
                        }
                        if let Some(ref new_val) = desc.value {
                            if let Some(ref cur_val) = current.value {
                                if !same_value(new_val, cur_val) {
                                    return false;
                                }
                            } else {
                                return false;
                            }
                        }
                    }
                } else if current_is_accessor {
                    // Non-configurable accessor property
                    if let Some(ref new_get) = desc.get {
                        let cur_get = current.get.as_ref().unwrap_or(&JsValue::Undefined);
                        if !same_value(new_get, cur_get) {
                            return false;
                        }
                    }
                    if let Some(ref new_set) = desc.set {
                        let cur_set = current.set.as_ref().unwrap_or(&JsValue::Undefined);
                        if !same_value(new_set, cur_set) {
                            return false;
                        }
                    }
                }
            }

            // Precompute type info before consuming desc
            let desc_is_data = desc.is_data_descriptor();
            let desc_is_accessor = desc.is_accessor_descriptor();
            let desc_has_get = desc.get.is_some();
            let desc_has_set = desc.set.is_some();
            let desc_writable = desc.writable;

            // Handle parameter map before consuming desc
            if let Some(ref mut map) = self.parameter_map
                && map.contains_key(&key)
            {
                if let Some(ref val) = desc.value
                    && let Some((env_ref, param_name)) = map.get(&key)
                {
                    let _ = env_ref.borrow_mut().set(param_name, val.clone());
                }
                if desc_has_get || desc_has_set {
                    map.remove(&key);
                } else if desc_writable == Some(false) {
                    if let Some(ref val) = desc.value
                        && let Some((env_ref, param_name)) = map.get(&key)
                    {
                        let _ = env_ref.borrow_mut().set(param_name, val.clone());
                    }
                    map.remove(&key);
                }
            }

            let current_is_data = current.is_data_descriptor();
            let current_is_accessor = current.is_accessor_descriptor();

            // Build merged descriptor
            let merged = if desc_is_data
                && !desc_is_accessor
                && current_is_accessor
                && !current_is_data
            {
                // Changing from accessor to data
                PropertyDescriptor {
                    value: desc.value.or(Some(JsValue::Undefined)),
                    writable: desc.writable.or(Some(false)),
                    get: None,
                    set: None,
                    enumerable: desc.enumerable.or(current.enumerable),
                    configurable: desc.configurable.or(current.configurable),
                }
            } else if desc_is_accessor && !desc_is_data && current_is_data && !current_is_accessor {
                // Changing from data to accessor
                PropertyDescriptor {
                    value: None,
                    writable: None,
                    get: desc.get.or(Some(JsValue::Undefined)),
                    set: desc.set.or(Some(JsValue::Undefined)),
                    enumerable: desc.enumerable.or(current.enumerable),
                    configurable: desc.configurable.or(current.configurable),
                }
            } else {
                // Normal merge: unspecified fields retain current values
                // but don't leak accessor fields into data or vice versa
                let result_is_accessor = if desc_is_accessor {
                    true
                } else if desc_is_data {
                    false
                } else {
                    current_is_accessor
                };
                if result_is_accessor {
                    PropertyDescriptor {
                        value: None,
                        writable: None,
                        get: desc.get.or(current.get),
                        set: desc.set.or(current.set),
                        enumerable: desc.enumerable.or(current.enumerable),
                        configurable: desc.configurable.or(current.configurable),
                    }
                } else {
                    PropertyDescriptor {
                        value: desc.value.or(current.value),
                        writable: desc.writable.or(current.writable),
                        get: None,
                        set: None,
                        enumerable: desc.enumerable.or(current.enumerable),
                        configurable: desc.configurable.or(current.configurable),
                    }
                }
            };

            self.properties.insert(key, merged);
        } else {
            if !self.extensible {
                return false;
            }
            // Handle parameter map for new properties
            if let Some(ref mut map) = self.parameter_map
                && map.contains_key(&key)
                && let Some(ref val) = desc.value
                && let Some((env_ref, param_name)) = map.get(&key)
            {
                let _ = env_ref.borrow_mut().set(param_name, val.clone());
            }
            self.property_order.push(key.clone());
            // For new property, fill in defaults per spec
            let is_accessor = desc.is_accessor_descriptor();
            let new_desc = PropertyDescriptor {
                value: desc.value.or(if !is_accessor {
                    Some(JsValue::Undefined)
                } else {
                    None
                }),
                writable: desc
                    .writable
                    .or(if !is_accessor { Some(false) } else { None }),
                get: desc.get,
                set: desc.set,
                enumerable: desc.enumerable.or(Some(false)),
                configurable: desc.configurable.or(Some(false)),
            };
            self.properties.insert(key, new_desc);
        }
        true
    }

    pub fn set_property_value(&mut self, key: &str, value: JsValue) -> bool {
        if let Some(ref ta) = self.typed_array_info
            && let Ok(idx) = key.parse::<usize>()
        {
            let ta_clone = ta.clone();
            return typed_array_set_index(&ta_clone, idx, &value);
        }
        if let Some(ref map) = self.parameter_map
            && let Some((env_ref, param_name)) = map.get(key)
        {
            let _ = env_ref.borrow_mut().set(param_name, value.clone());
        }
        if let Some(desc) = self.properties.get_mut(key) {
            if desc.writable == Some(false) {
                return false;
            }
            desc.value = Some(value);
            true
        } else {
            if !self.extensible {
                return false;
            }
            if !key.starts_with("Symbol(") {
                self.property_order.push(key.to_string());
            }
            self.properties
                .insert(key.to_string(), PropertyDescriptor::data_default(value));
            true
        }
    }

    pub fn insert_value(&mut self, key: String, value: JsValue) {
        if !self.properties.contains_key(&key) {
            self.property_order.push(key.clone());
        }
        self.properties
            .insert(key, PropertyDescriptor::data_default(value));
    }

    pub fn insert_builtin(&mut self, key: String, value: JsValue) {
        if !self.properties.contains_key(&key) {
            self.property_order.push(key.clone());
        }
        self.properties
            .insert(key, PropertyDescriptor::data(value, true, false, true));
    }

    pub fn insert_property(&mut self, key: String, desc: PropertyDescriptor) {
        if !self.properties.contains_key(&key) {
            self.property_order.push(key.clone());
        }
        self.properties.insert(key, desc);
    }

    pub fn get_property_value(&self, key: &str) -> Option<JsValue> {
        self.properties.get(key).and_then(|d| d.value.clone())
    }
}

pub(crate) fn typed_array_get_index(ta: &TypedArrayInfo, idx: usize) -> JsValue {
    let buf = ta.buffer.borrow();
    let offset = ta.byte_offset + idx * ta.kind.bytes_per_element();
    match ta.kind {
        TypedArrayKind::Int8 => JsValue::Number(buf[offset] as i8 as f64),
        TypedArrayKind::Uint8 | TypedArrayKind::Uint8Clamped => JsValue::Number(buf[offset] as f64),
        TypedArrayKind::Int16 => {
            let v = i16::from_ne_bytes([buf[offset], buf[offset + 1]]);
            JsValue::Number(v as f64)
        }
        TypedArrayKind::Uint16 => {
            let v = u16::from_ne_bytes([buf[offset], buf[offset + 1]]);
            JsValue::Number(v as f64)
        }
        TypedArrayKind::Int32 => {
            let v = i32::from_ne_bytes([
                buf[offset],
                buf[offset + 1],
                buf[offset + 2],
                buf[offset + 3],
            ]);
            JsValue::Number(v as f64)
        }
        TypedArrayKind::Uint32 => {
            let v = u32::from_ne_bytes([
                buf[offset],
                buf[offset + 1],
                buf[offset + 2],
                buf[offset + 3],
            ]);
            JsValue::Number(v as f64)
        }
        TypedArrayKind::Float32 => {
            let v = f32::from_ne_bytes([
                buf[offset],
                buf[offset + 1],
                buf[offset + 2],
                buf[offset + 3],
            ]);
            JsValue::Number(v as f64)
        }
        TypedArrayKind::Float64 => {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&buf[offset..offset + 8]);
            JsValue::Number(f64::from_ne_bytes(bytes))
        }
        TypedArrayKind::BigInt64 => {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&buf[offset..offset + 8]);
            JsValue::BigInt(crate::types::JsBigInt {
                value: num_bigint::BigInt::from(i64::from_ne_bytes(bytes)),
            })
        }
        TypedArrayKind::BigUint64 => {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&buf[offset..offset + 8]);
            JsValue::BigInt(crate::types::JsBigInt {
                value: num_bigint::BigInt::from(u64::from_ne_bytes(bytes)),
            })
        }
    }
}

pub(crate) fn typed_array_set_index(ta: &TypedArrayInfo, idx: usize, value: &JsValue) -> bool {
    if idx >= ta.array_length {
        return false;
    }
    let mut buf = ta.buffer.borrow_mut();
    let offset = ta.byte_offset + idx * ta.kind.bytes_per_element();
    match ta.kind {
        TypedArrayKind::Int8 => {
            let v = to_int8(value);
            buf[offset] = v as u8;
        }
        TypedArrayKind::Uint8 => {
            let v = to_uint8(value);
            buf[offset] = v;
        }
        TypedArrayKind::Uint8Clamped => {
            let v = to_uint8_clamped(value);
            buf[offset] = v;
        }
        TypedArrayKind::Int16 => {
            let v = to_int16(value);
            buf[offset..offset + 2].copy_from_slice(&v.to_ne_bytes());
        }
        TypedArrayKind::Uint16 => {
            let v = to_uint16(value);
            buf[offset..offset + 2].copy_from_slice(&v.to_ne_bytes());
        }
        TypedArrayKind::Int32 => {
            let v = to_int32(value);
            buf[offset..offset + 4].copy_from_slice(&v.to_ne_bytes());
        }
        TypedArrayKind::Uint32 => {
            let v = to_uint32(value);
            buf[offset..offset + 4].copy_from_slice(&v.to_ne_bytes());
        }
        TypedArrayKind::Float32 => {
            let n = to_number(value);
            buf[offset..offset + 4].copy_from_slice(&(n as f32).to_ne_bytes());
        }
        TypedArrayKind::Float64 => {
            let n = to_number(value);
            buf[offset..offset + 8].copy_from_slice(&n.to_ne_bytes());
        }
        TypedArrayKind::BigInt64 => {
            let v = to_bigint64(value);
            buf[offset..offset + 8].copy_from_slice(&v.to_ne_bytes());
        }
        TypedArrayKind::BigUint64 => {
            let v = to_biguint64(value);
            buf[offset..offset + 8].copy_from_slice(&v.to_ne_bytes());
        }
    }
    true
}

fn to_number(v: &JsValue) -> f64 {
    match v {
        JsValue::Number(n) => *n,
        JsValue::Boolean(true) => 1.0,
        JsValue::Boolean(false) | JsValue::Null => 0.0,
        JsValue::Undefined => f64::NAN,
        JsValue::String(s) => s.to_string().parse::<f64>().unwrap_or(f64::NAN),
        _ => f64::NAN,
    }
}

fn to_int8(v: &JsValue) -> i8 {
    to_number(v) as i32 as i8
}
fn to_uint8(v: &JsValue) -> u8 {
    to_number(v) as i32 as u8
}
fn to_uint8_clamped(v: &JsValue) -> u8 {
    let n = to_number(v);
    if n.is_nan() {
        0
    } else if n <= 0.0 {
        0
    } else if n >= 255.0 {
        255
    } else {
        (n + 0.5).floor() as u8
    }
}
fn to_int16(v: &JsValue) -> i16 {
    to_number(v) as i32 as i16
}
fn to_uint16(v: &JsValue) -> u16 {
    to_number(v) as i32 as u16
}
fn to_int32(v: &JsValue) -> i32 {
    to_number(v) as i32
}
fn to_uint32(v: &JsValue) -> u32 {
    to_number(v) as u32
}
fn to_bigint64(v: &JsValue) -> i64 {
    match v {
        JsValue::BigInt(b) => {
            i64::try_from(&b.value).unwrap_or_else(|_| {
                // Truncate to 64 bits
                let bytes = b.value.to_signed_bytes_le();
                let mut result = [0u8; 8];
                let len = bytes.len().min(8);
                result[..len].copy_from_slice(&bytes[..len]);
                if bytes.len() < 8 && !bytes.is_empty() && (bytes[bytes.len() - 1] & 0x80) != 0 {
                    for byte in result.iter_mut().skip(len) {
                        *byte = 0xFF;
                    }
                }
                i64::from_le_bytes(result)
            })
        }
        _ => 0,
    }
}
fn to_biguint64(v: &JsValue) -> u64 {
    match v {
        JsValue::BigInt(b) => u64::try_from(&b.value).unwrap_or_else(|_| {
            let bytes = b.value.to_signed_bytes_le();
            let mut result = [0u8; 8];
            let len = bytes.len().min(8);
            result[..len].copy_from_slice(&bytes[..len]);
            u64::from_le_bytes(result)
        }),
        _ => 0,
    }
}

#[derive(Debug, Clone)]
pub enum PromiseState {
    Pending,
    Fulfilled(JsValue),
    Rejected(JsValue),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PromiseReactionType {
    Fulfill,
    Reject,
}

#[derive(Debug, Clone)]
pub struct PromiseReaction {
    pub handler: Option<JsValue>,
    pub promise_id: Option<u64>,
    pub resolve: JsValue,
    pub reject: JsValue,
    pub reaction_type: PromiseReactionType,
}

#[derive(Debug, Clone)]
pub struct PromiseData {
    pub state: PromiseState,
    pub fulfill_reactions: Vec<PromiseReaction>,
    pub reject_reactions: Vec<PromiseReaction>,
    pub is_handled: bool,
}

impl PromiseData {
    pub fn new() -> Self {
        Self {
            state: PromiseState::Pending,
            fulfill_reactions: Vec::new(),
            reject_reactions: Vec::new(),
            is_handled: false,
        }
    }
}

pub(crate) const GC_THRESHOLD: usize = 4096;

pub(crate) struct SetRecord {
    pub(crate) has: JsValue,
    pub(crate) keys: JsValue,
    pub(crate) size: f64,
}
