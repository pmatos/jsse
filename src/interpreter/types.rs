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
}

pub type EnvRef = Rc<RefCell<Environment>>;

#[derive(Debug)]
pub struct Environment {
    pub(crate) bindings: HashMap<String, Binding>,
    pub(crate) parent: Option<EnvRef>,
    pub strict: bool,
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
        let strict = parent.as_ref().map_or(false, |p| p.borrow().strict);
        Rc::new(RefCell::new(Environment {
            bindings: HashMap::new(),
            parent,
            strict,
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

    pub fn get(&self, name: &str) -> Option<JsValue> {
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
    },
    Native(
        String,
        usize,
        Rc<dyn Fn(&mut super::Interpreter, &JsValue, &[JsValue]) -> Completion>,
    ),
}

impl JsFunction {
    pub fn native(
        name: String,
        arity: usize,
        f: impl Fn(&mut super::Interpreter, &JsValue, &[JsValue]) -> Completion + 'static,
    ) -> Self {
        JsFunction::Native(name, arity, Rc::new(f))
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
            } => JsFunction::User {
                name: name.clone(),
                params: params.clone(),
                body: body.clone(),
                closure: closure.clone(),
                is_arrow: *is_arrow,
                is_strict: *is_strict,
                is_generator: *is_generator,
            },
            JsFunction::Native(name, arity, f) => {
                JsFunction::Native(name.clone(), *arity, f.clone())
            }
        }
    }
}

impl std::fmt::Debug for JsFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsFunction::User { name, .. } => write!(f, "JsFunction::User({name:?})"),
            JsFunction::Native(name, arity, _) => {
                write!(f, "JsFunction::Native({name:?}, {arity})")
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
    RegExpStringIterator {
        source: String,
        flags: String,
        string: String,
        global: bool,
        last_index: usize,
        done: bool,
    },
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
        }
    }

    pub fn is_proxy(&self) -> bool {
        self.proxy_target.is_some()
    }

    pub fn get_property(&self, key: &str) -> JsValue {
        if let Some(ref map) = self.parameter_map {
            if let Some((env_ref, param_name)) = map.get(key) {
                if let Some(val) = env_ref.borrow().get(param_name) {
                    return val;
                }
            }
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
        if let Some(proto) = &self.prototype {
            return proto.borrow().get_property(key);
        }
        JsValue::Undefined
    }

    pub fn get_property_descriptor(&self, key: &str) -> Option<PropertyDescriptor> {
        if let Some(desc) = self.properties.get(key) {
            let mut d = desc.clone();
            if let Some(ref map) = self.parameter_map {
                if let Some((env_ref, param_name)) = map.get(key) {
                    if let Some(val) = env_ref.borrow().get(param_name) {
                        d.value = Some(val);
                    }
                }
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
            if let Some(ref mut map) = self.parameter_map {
                if map.contains_key(&key) {
                    if let Some(ref val) = desc.value {
                        if let Some((env_ref, param_name)) = map.get(&key) {
                            let _ = env_ref.borrow_mut().set(param_name, val.clone());
                        }
                    }
                    if desc_has_get || desc_has_set {
                        map.remove(&key);
                    } else if desc_writable == Some(false) {
                        if let Some(ref val) = desc.value {
                            if let Some((env_ref, param_name)) = map.get(&key) {
                                let _ = env_ref.borrow_mut().set(param_name, val.clone());
                            }
                        }
                        map.remove(&key);
                    }
                }
            }

            let current_is_data = current.is_data_descriptor();
            let current_is_accessor = current.is_accessor_descriptor();

            // Build merged descriptor
            let merged = if desc_is_data && !desc_is_accessor
                && current_is_accessor && !current_is_data {
                // Changing from accessor to data
                PropertyDescriptor {
                    value: desc.value.or(Some(JsValue::Undefined)),
                    writable: desc.writable.or(Some(false)),
                    get: None,
                    set: None,
                    enumerable: desc.enumerable.or(current.enumerable),
                    configurable: desc.configurable.or(current.configurable),
                }
            } else if desc_is_accessor && !desc_is_data
                && current_is_data && !current_is_accessor {
                // Changing from data to accessor
                PropertyDescriptor {
                    value: None,
                    writable: None,
                    get: desc.get,
                    set: desc.set,
                    enumerable: desc.enumerable.or(current.enumerable),
                    configurable: desc.configurable.or(current.configurable),
                }
            } else {
                // Normal merge: unspecified fields retain current values
                PropertyDescriptor {
                    value: desc.value.or(current.value),
                    writable: desc.writable.or(current.writable),
                    get: desc.get.or(current.get),
                    set: desc.set.or(current.set),
                    enumerable: desc.enumerable.or(current.enumerable),
                    configurable: desc.configurable.or(current.configurable),
                }
            };

            self.properties.insert(key, merged);
        } else {
            if !self.extensible {
                return false;
            }
            // Handle parameter map for new properties
            if let Some(ref mut map) = self.parameter_map {
                if map.contains_key(&key) {
                    if let Some(ref val) = desc.value {
                        if let Some((env_ref, param_name)) = map.get(&key) {
                            let _ = env_ref.borrow_mut().set(param_name, val.clone());
                        }
                    }
                }
            }
            self.property_order.push(key.clone());
            // For new property, fill in defaults per spec
            let is_accessor = desc.is_accessor_descriptor();
            let new_desc = PropertyDescriptor {
                value: desc.value.or(if !is_accessor { Some(JsValue::Undefined) } else { None }),
                writable: desc.writable.or(if !is_accessor { Some(false) } else { None }),
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
        if let Some(ref map) = self.parameter_map {
            if let Some((env_ref, param_name)) = map.get(key) {
                let _ = env_ref.borrow_mut().set(param_name, value.clone());
            }
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

pub(crate) const GC_THRESHOLD: usize = 4096;

pub(crate) struct SetRecord {
    pub(crate) has: JsValue,
    pub(crate) keys: JsValue,
    pub(crate) size: f64,
}
