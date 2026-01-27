use crate::ast::*;
use crate::parser;
use crate::types::{JsString, JsValue, number_ops};
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
}

impl Completion {
    fn is_abrupt(&self) -> bool {
        !matches!(self, Completion::Normal(_))
    }
}

type EnvRef = Rc<RefCell<Environment>>;

#[derive(Debug)]
pub struct Environment {
    bindings: HashMap<String, Binding>,
    parent: Option<EnvRef>,
}

#[derive(Debug, Clone)]
struct Binding {
    value: JsValue,
    kind: BindingKind,
    initialized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum BindingKind {
    Var,
    Let,
    Const,
}

impl Environment {
    fn new(parent: Option<EnvRef>) -> EnvRef {
        Rc::new(RefCell::new(Environment {
            bindings: HashMap::new(),
            parent,
        }))
    }

    fn declare(&mut self, name: &str, kind: BindingKind) {
        self.bindings.insert(
            name.to_string(),
            Binding {
                value: JsValue::Undefined,
                kind,
                initialized: kind == BindingKind::Var,
            },
        );
    }

    fn set(&mut self, name: &str, value: JsValue) -> Result<(), JsValue> {
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

    fn get(&self, name: &str) -> Option<JsValue> {
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

    fn has(&self, name: &str) -> bool {
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
    },
    Native(
        String,
        Rc<dyn Fn(&mut Interpreter, &JsValue, &[JsValue]) -> Completion>,
    ),
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
            } => JsFunction::User {
                name: name.clone(),
                params: params.clone(),
                body: body.clone(),
                closure: closure.clone(),
                is_arrow: *is_arrow,
            },
            JsFunction::Native(name, f) => JsFunction::Native(name.clone(), f.clone()),
        }
    }
}

impl std::fmt::Debug for JsFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsFunction::User { name, .. } => write!(f, "JsFunction::User({name:?})"),
            JsFunction::Native(name, _) => write!(f, "JsFunction::Native({name:?})"),
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
pub struct JsObjectData {
    pub properties: HashMap<String, PropertyDescriptor>,
    pub property_order: Vec<String>,
    pub prototype: Option<Rc<RefCell<JsObjectData>>>,
    pub callable: Option<JsFunction>,
    pub array_elements: Option<Vec<JsValue>>,
    pub class_name: String,
    pub extensible: bool,
    pub primitive_value: Option<JsValue>,
}

impl JsObjectData {
    fn new() -> Self {
        Self {
            properties: HashMap::new(),
            property_order: Vec::new(),
            prototype: None,
            callable: None,
            array_elements: None,
            class_name: "Object".to_string(),
            extensible: true,
            primitive_value: None,
        }
    }

    pub fn get_property(&self, key: &str) -> JsValue {
        if let Some(desc) = self.properties.get(key) {
            if let Some(ref val) = desc.value {
                return val.clone();
            }
            // Accessor without value — return undefined (getter must be called by interpreter)
            return JsValue::Undefined;
        }
        if let Some(proto) = &self.prototype {
            return proto.borrow().get_property(key);
        }
        JsValue::Undefined
    }

    pub fn get_property_descriptor(&self, key: &str) -> Option<PropertyDescriptor> {
        if let Some(desc) = self.properties.get(key) {
            return Some(desc.clone());
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
            if let Some(desc) = self.properties.get(k) {
                if desc.enumerable != Some(false) {
                    if seen.insert(k.clone()) {
                        keys.push(k.clone());
                    }
                }
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
        if let Some(current) = self.properties.get(&key) {
            if current.configurable == Some(false) {
                if desc.configurable == Some(true) {
                    return false;
                }
                if desc.enumerable.is_some() && desc.enumerable != current.enumerable {
                    return false;
                }
                if current.is_data_descriptor() && desc.is_data_descriptor() {
                    if current.writable == Some(false) {
                        if desc.writable == Some(true) {
                            return false;
                        }
                        if desc.value.is_some() {
                            return false;
                        }
                    }
                }
            }
        } else if !self.extensible {
            return false;
        }
        if !self.properties.contains_key(&key) {
            self.property_order.push(key.clone());
        }
        self.properties.insert(key, desc);
        true
    }

    pub fn set_property_value(&mut self, key: &str, value: JsValue) {
        if let Some(desc) = self.properties.get_mut(key) {
            if desc.writable != Some(false) {
                desc.value = Some(value);
            }
        } else {
            self.properties
                .insert(key.to_string(), PropertyDescriptor::data_default(value));
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

pub struct Interpreter {
    global_env: EnvRef,
    objects: Vec<Rc<RefCell<JsObjectData>>>,
    object_prototype: Option<Rc<RefCell<JsObjectData>>>,
    array_prototype: Option<Rc<RefCell<JsObjectData>>>,
    string_prototype: Option<Rc<RefCell<JsObjectData>>>,
    number_prototype: Option<Rc<RefCell<JsObjectData>>>,
    boolean_prototype: Option<Rc<RefCell<JsObjectData>>>,
    regexp_prototype: Option<Rc<RefCell<JsObjectData>>>,
    next_symbol_id: u64,
}

impl Interpreter {
    pub fn new() -> Self {
        let global = Environment::new(None);

        {
            let mut env = global.borrow_mut();
            for (name, value) in [
                ("undefined", JsValue::Undefined),
                ("NaN", JsValue::Number(f64::NAN)),
                ("Infinity", JsValue::Number(f64::INFINITY)),
            ] {
                env.bindings.insert(
                    name.to_string(),
                    Binding {
                        value,
                        kind: BindingKind::Const,
                        initialized: true,
                    },
                );
            }
        }

        let mut interp = Self {
            global_env: global,
            objects: Vec::new(),
            object_prototype: None,
            array_prototype: None,
            string_prototype: None,
            number_prototype: None,
            boolean_prototype: None,
            regexp_prototype: None,
            next_symbol_id: 1,
        };
        interp.setup_globals();
        interp
    }

    fn register_global_fn(&mut self, name: &str, kind: BindingKind, func: JsFunction) {
        let val = self.create_function(func);
        self.global_env.borrow_mut().declare(name, kind);
        let _ = self.global_env.borrow_mut().set(name, val);
    }

    fn setup_globals(&mut self) {
        let console_id = self.objects.len() as u64;
        let console = self.create_object();
        {
            let log_fn = self.create_function(JsFunction::Native(
                "log".to_string(),
                Rc::new(|_interp, _this, args| {
                    let parts: Vec<String> = args.iter().map(|v| format!("{v}")).collect();
                    println!("{}", parts.join(" "));
                    Completion::Normal(JsValue::Undefined)
                }),
            ));
            console.borrow_mut().insert_value("log".to_string(), log_fn);
        }
        let console_val = JsValue::Object(crate::types::JsObject { id: console_id });
        self.global_env
            .borrow_mut()
            .declare("console", BindingKind::Const);
        let _ = self.global_env.borrow_mut().set("console", console_val);

        // Error constructor
        {
            let error_name = "Error".to_string();
            self.register_global_fn(
                "Error",
                BindingKind::Var,
                JsFunction::Native(
                    error_name.clone(),
                    Rc::new(move |interp, _this, args| {
                        let msg = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let obj = interp.create_object();
                        {
                            let mut o = obj.borrow_mut();
                            o.class_name = "Error".to_string();
                            o.insert_value("message".to_string(), msg);
                            o.insert_value(
                                "name".to_string(),
                                JsValue::String(JsString::from_str("Error")),
                            );
                        }
                        Completion::Normal(JsValue::Object(crate::types::JsObject {
                            id: interp.objects.len() as u64 - 1,
                        }))
                    }),
                ),
            );
        }

        // Get Error.prototype for inheritance
        let error_prototype = {
            let env = self.global_env.borrow();
            if let Some(error_val) = env.get("Error") {
                if let JsValue::Object(o) = &error_val {
                    if let Some(ctor) = self.get_object(o.id) {
                        let proto_val = ctor.borrow().get_property("prototype");
                        if let JsValue::Object(p) = &proto_val {
                            self.get_object(p.id)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        };

        // Add toString to Error.prototype
        if let Some(ref ep) = error_prototype {
            let tostring_fn = self.create_function(JsFunction::Native(
                "toString".to_string(),
                Rc::new(|interp, this_val, _args| {
                    if let JsValue::Object(o) = this_val {
                        if let Some(obj) = interp.get_object(o.id) {
                            let obj_ref = obj.borrow();
                            let name = match obj_ref.get_property("name") {
                                JsValue::Undefined => "Error".to_string(),
                                v => to_js_string(&v),
                            };
                            let msg = match obj_ref.get_property("message") {
                                JsValue::Undefined => String::new(),
                                v => to_js_string(&v),
                            };
                            return if msg.is_empty() {
                                Completion::Normal(JsValue::String(JsString::from_str(&name)))
                            } else {
                                Completion::Normal(JsValue::String(JsString::from_str(&format!(
                                    "{name}: {msg}"
                                ))))
                            };
                        }
                    }
                    Completion::Normal(JsValue::String(JsString::from_str("Error")))
                }),
            ));
            ep.borrow_mut()
                .insert_builtin("toString".to_string(), tostring_fn);
        }

        // Test262Error
        {
            let error_proto_clone = error_prototype.clone();
            self.register_global_fn(
                "Test262Error",
                BindingKind::Var,
                JsFunction::Native(
                    "Test262Error".to_string(),
                    Rc::new(move |interp, _this, args| {
                        let msg = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let obj = interp.create_object();
                        {
                            let mut o = obj.borrow_mut();
                            o.class_name = "Test262Error".to_string();
                            if let Some(ref ep) = error_proto_clone {
                                o.prototype = Some(ep.clone());
                            }
                            o.insert_value("message".to_string(), msg);
                            o.insert_value(
                                "name".to_string(),
                                JsValue::String(JsString::from_str("Test262Error")),
                            );
                        }
                        Completion::Normal(JsValue::Object(crate::types::JsObject {
                            id: interp.objects.len() as u64 - 1,
                        }))
                    }),
                ),
            );
        }

        // Error subtype constructors
        for name in [
            "SyntaxError",
            "TypeError",
            "ReferenceError",
            "RangeError",
            "URIError",
            "EvalError",
        ] {
            let error_name = name.to_string();
            let error_proto_clone = error_prototype.clone();
            self.register_global_fn(
                name,
                BindingKind::Var,
                JsFunction::Native(
                    error_name.clone(),
                    Rc::new(move |interp, _this, args| {
                        let msg = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let obj = interp.create_object();
                        {
                            let mut o = obj.borrow_mut();
                            o.class_name = error_name.clone();
                            if let Some(ref ep) = error_proto_clone {
                                o.prototype = Some(ep.clone());
                            }
                            o.insert_value("message".to_string(), msg);
                            o.insert_value(
                                "name".to_string(),
                                JsValue::String(JsString::from_str(&error_name)),
                            );
                        }
                        Completion::Normal(JsValue::Object(crate::types::JsObject {
                            id: interp.objects.len() as u64 - 1,
                        }))
                    }),
                ),
            );
        }

        // Object constructor (minimal)
        self.register_global_fn(
            "Object",
            BindingKind::Var,
            JsFunction::Native(
                "Object".to_string(),
                Rc::new(|interp, _this, args| {
                    if let Some(val) = args.first()
                        && matches!(val, JsValue::Object(_))
                    {
                        return Completion::Normal(val.clone());
                    }
                    let _obj = interp.create_object();
                    Completion::Normal(JsValue::Object(crate::types::JsObject {
                        id: interp.objects.len() as u64 - 1,
                    }))
                }),
            ),
        );

        self.setup_object_statics();
        self.setup_array_prototype();
        self.setup_string_prototype();

        // String constructor/converter
        self.register_global_fn(
            "String",
            BindingKind::Var,
            JsFunction::Native(
                "String".to_string(),
                Rc::new(|interp, this, args| {
                    let val = args
                        .first()
                        .cloned()
                        .unwrap_or(JsValue::String(JsString::from_str("")));
                    let s = to_js_string(&val);
                    if let JsValue::Object(o) = this {
                        if let Some(obj) = interp.get_object(o.id) {
                            obj.borrow_mut().primitive_value =
                                Some(JsValue::String(JsString::from_str(&s)));
                            obj.borrow_mut().class_name = "String".to_string();
                        }
                    }
                    Completion::Normal(JsValue::String(JsString::from_str(&s)))
                }),
            ),
        );

        // Number constructor/converter
        self.register_global_fn(
            "Number",
            BindingKind::Var,
            JsFunction::Native(
                "Number".to_string(),
                Rc::new(|interp, this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::Number(0.0));
                    let n = to_number(&val);
                    if let JsValue::Object(o) = this {
                        if let Some(obj) = interp.get_object(o.id) {
                            obj.borrow_mut().primitive_value = Some(JsValue::Number(n));
                            obj.borrow_mut().class_name = "Number".to_string();
                        }
                    }
                    Completion::Normal(JsValue::Number(n))
                }),
            ),
        );

        // Number static properties
        {
            let is_finite_fn = self.create_function(JsFunction::Native(
                "isFinite".to_string(),
                Rc::new(|_interp, _this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let result = matches!(&val, JsValue::Number(n) if n.is_finite());
                    Completion::Normal(JsValue::Boolean(result))
                }),
            ));
            let is_nan_fn = self.create_function(JsFunction::Native(
                "isNaN".to_string(),
                Rc::new(|_interp, _this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let result = matches!(&val, JsValue::Number(n) if n.is_nan());
                    Completion::Normal(JsValue::Boolean(result))
                }),
            ));
            let is_integer_fn = self.create_function(JsFunction::Native(
                "isInteger".to_string(),
                Rc::new(|_interp, _this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let result = if let JsValue::Number(n) = &val {
                        n.is_finite() && *n == n.trunc()
                    } else {
                        false
                    };
                    Completion::Normal(JsValue::Boolean(result))
                }),
            ));
            let is_safe_fn = self.create_function(JsFunction::Native(
                "isSafeInteger".to_string(),
                Rc::new(|_interp, _this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let result = if let JsValue::Number(n) = &val {
                        n.is_finite() && *n == n.trunc() && n.abs() <= 9007199254740991.0
                    } else {
                        false
                    };
                    Completion::Normal(JsValue::Boolean(result))
                }),
            ));
            let parse_int = self.global_env.borrow().get("parseInt");
            let parse_float = self.global_env.borrow().get("parseFloat");

            if let Some(num_val) = self.global_env.borrow().get("Number") {
                if let JsValue::Object(o) = &num_val {
                    if let Some(num_obj) = self.get_object(o.id) {
                        let mut n = num_obj.borrow_mut();
                        n.insert_value(
                            "POSITIVE_INFINITY".to_string(),
                            JsValue::Number(f64::INFINITY),
                        );
                        n.insert_value(
                            "NEGATIVE_INFINITY".to_string(),
                            JsValue::Number(f64::NEG_INFINITY),
                        );
                        n.insert_value("MAX_VALUE".to_string(), JsValue::Number(f64::MAX));
                        n.insert_value("MIN_VALUE".to_string(), JsValue::Number(f64::MIN_POSITIVE));
                        n.insert_value("NaN".to_string(), JsValue::Number(f64::NAN));
                        n.insert_value("EPSILON".to_string(), JsValue::Number(f64::EPSILON));
                        n.insert_value(
                            "MAX_SAFE_INTEGER".to_string(),
                            JsValue::Number(9007199254740991.0),
                        );
                        n.insert_value(
                            "MIN_SAFE_INTEGER".to_string(),
                            JsValue::Number(-9007199254740991.0),
                        );
                        n.insert_value("isFinite".to_string(), is_finite_fn);
                        n.insert_value("isNaN".to_string(), is_nan_fn);
                        n.insert_value("isInteger".to_string(), is_integer_fn);
                        n.insert_value("isSafeInteger".to_string(), is_safe_fn);
                        if let Some(pi) = parse_int {
                            n.insert_value("parseInt".to_string(), pi);
                        }
                        if let Some(pf) = parse_float {
                            n.insert_value("parseFloat".to_string(), pf);
                        }
                    }
                }
            }
        }

        // Boolean constructor/converter
        self.register_global_fn(
            "Boolean",
            BindingKind::Var,
            JsFunction::Native(
                "Boolean".to_string(),
                Rc::new(|interp, this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let b = to_boolean(&val);
                    if let JsValue::Object(o) = this {
                        if let Some(obj) = interp.get_object(o.id) {
                            obj.borrow_mut().primitive_value = Some(JsValue::Boolean(b));
                            obj.borrow_mut().class_name = "Boolean".to_string();
                        }
                    }
                    Completion::Normal(JsValue::Boolean(b))
                }),
            ),
        );

        self.setup_number_prototype();
        self.setup_boolean_prototype();

        // Array constructor
        self.register_global_fn(
            "Array",
            BindingKind::Var,
            JsFunction::Native(
                "Array".to_string(),
                Rc::new(|interp, _this, args| {
                    if args.len() == 1 {
                        if let JsValue::Number(n) = &args[0] {
                            let arr = interp.create_array(vec![JsValue::Undefined; *n as usize]);
                            return Completion::Normal(arr);
                        }
                    }
                    let arr = interp.create_array(args.to_vec());
                    Completion::Normal(arr)
                }),
            ),
        );

        // Global functions
        self.register_global_fn(
            "parseInt",
            BindingKind::Var,
            JsFunction::Native(
                "parseInt".to_string(),
                Rc::new(|_interp, _this, args| {
                    let s = args.first().map(to_js_string).unwrap_or_default();
                    let radix = args.get(1).map(|v| to_number(v) as i32).unwrap_or(10);
                    let s = s.trim();
                    let (negative, s) = if let Some(rest) = s.strip_prefix('-') {
                        (true, rest)
                    } else if let Some(rest) = s.strip_prefix('+') {
                        (false, rest)
                    } else {
                        (false, s)
                    };
                    let radix = if radix == 0 {
                        if s.starts_with("0x") || s.starts_with("0X") {
                            16
                        } else {
                            10
                        }
                    } else {
                        radix
                    };
                    let s = if radix == 16 {
                        s.strip_prefix("0x")
                            .or_else(|| s.strip_prefix("0X"))
                            .unwrap_or(s)
                    } else {
                        s
                    };
                    match i64::from_str_radix(s, radix as u32) {
                        Ok(n) => {
                            let n = if negative { -n } else { n };
                            Completion::Normal(JsValue::Number(n as f64))
                        }
                        Err(_) => Completion::Normal(JsValue::Number(f64::NAN)),
                    }
                }),
            ),
        );

        self.register_global_fn(
            "parseFloat",
            BindingKind::Var,
            JsFunction::Native(
                "parseFloat".to_string(),
                Rc::new(|_interp, _this, args| {
                    let s = args.first().map(to_js_string).unwrap_or_default();
                    let s = s.trim();
                    match s.parse::<f64>() {
                        Ok(n) => Completion::Normal(JsValue::Number(n)),
                        Err(_) => Completion::Normal(JsValue::Number(f64::NAN)),
                    }
                }),
            ),
        );

        self.register_global_fn(
            "isNaN",
            BindingKind::Var,
            JsFunction::Native(
                "isNaN".to_string(),
                Rc::new(|_interp, _this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let n = to_number(&val);
                    Completion::Normal(JsValue::Boolean(n.is_nan()))
                }),
            ),
        );

        self.register_global_fn(
            "isFinite",
            BindingKind::Var,
            JsFunction::Native(
                "isFinite".to_string(),
                Rc::new(|_interp, _this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let n = to_number(&val);
                    Completion::Normal(JsValue::Boolean(n.is_finite()))
                }),
            ),
        );

        // Math object
        let math_id = self.objects.len() as u64;
        let math_obj = self.create_object();
        {
            let mut m = math_obj.borrow_mut();
            m.class_name = "Math".to_string();
            m.insert_value("PI".to_string(), JsValue::Number(std::f64::consts::PI));
            m.insert_value("E".to_string(), JsValue::Number(std::f64::consts::E));
            m.insert_value("LN2".to_string(), JsValue::Number(std::f64::consts::LN_2));
            m.insert_value("LN10".to_string(), JsValue::Number(std::f64::consts::LN_10));
            m.insert_value(
                "LOG2E".to_string(),
                JsValue::Number(std::f64::consts::LOG2_E),
            );
            m.insert_value(
                "LOG10E".to_string(),
                JsValue::Number(std::f64::consts::LOG10_E),
            );
            m.insert_value(
                "SQRT2".to_string(),
                JsValue::Number(std::f64::consts::SQRT_2),
            );
            m.insert_value(
                "SQRT1_2".to_string(),
                JsValue::Number(std::f64::consts::FRAC_1_SQRT_2),
            );
        }
        // Add Math methods
        let math_fns: Vec<(&str, fn(f64) -> f64)> = vec![
            ("abs", f64::abs),
            ("ceil", f64::ceil),
            ("floor", f64::floor),
            ("round", f64::round),
            ("sqrt", f64::sqrt),
            ("sin", f64::sin),
            ("cos", f64::cos),
            ("tan", f64::tan),
            ("log", f64::ln),
            ("exp", f64::exp),
            ("asin", f64::asin),
            ("acos", f64::acos),
            ("atan", f64::atan),
            ("trunc", f64::trunc),
            ("sign", (|x: f64| {
                if x.is_nan() || x == 0.0 { x } else if x > 0.0 { 1.0 } else { -1.0 }
            }) as fn(f64) -> f64),
            ("cbrt", f64::cbrt),
        ];
        for (name, op) in math_fns {
            let fn_val = self.create_function(JsFunction::Native(
                name.to_string(),
                Rc::new(move |_interp, _this, args| {
                    let x = args.first().map(to_number).unwrap_or(f64::NAN);
                    Completion::Normal(JsValue::Number(op(x)))
                }),
            ));
            math_obj.borrow_mut().insert_value(name.to_string(), fn_val);
        }
        // Math.max, Math.min, Math.pow, Math.random, Math.atan2
        let max_fn = self.create_function(JsFunction::Native(
            "max".to_string(),
            Rc::new(|_interp, _this, args| {
                if args.is_empty() {
                    return Completion::Normal(JsValue::Number(f64::NEG_INFINITY));
                }
                let mut result = f64::NEG_INFINITY;
                for a in args {
                    let n = to_number(a);
                    if n.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    if n > result {
                        result = n;
                    }
                }
                Completion::Normal(JsValue::Number(result))
            }),
        ));
        math_obj
            .borrow_mut()
            .insert_builtin("max".to_string(), max_fn);
        let min_fn = self.create_function(JsFunction::Native(
            "min".to_string(),
            Rc::new(|_interp, _this, args| {
                if args.is_empty() {
                    return Completion::Normal(JsValue::Number(f64::INFINITY));
                }
                let mut result = f64::INFINITY;
                for a in args {
                    let n = to_number(a);
                    if n.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    if n < result {
                        result = n;
                    }
                }
                Completion::Normal(JsValue::Number(result))
            }),
        ));
        math_obj
            .borrow_mut()
            .insert_builtin("min".to_string(), min_fn);
        let pow_fn = self.create_function(JsFunction::Native(
            "pow".to_string(),
            Rc::new(|_interp, _this, args| {
                let base = args.first().map(to_number).unwrap_or(f64::NAN);
                let exp = args.get(1).map(to_number).unwrap_or(f64::NAN);
                Completion::Normal(JsValue::Number(base.powf(exp)))
            }),
        ));
        math_obj
            .borrow_mut()
            .insert_builtin("pow".to_string(), pow_fn);
        let random_fn = self.create_function(JsFunction::Native(
            "random".to_string(),
            Rc::new(|_interp, _this, _args| {
                Completion::Normal(JsValue::Number(0.5)) // deterministic for testing
            }),
        ));
        math_obj
            .borrow_mut()
            .insert_builtin("random".to_string(), random_fn);

        // Math.atan2
        let atan2_fn = self.create_function(JsFunction::Native(
            "atan2".to_string(),
            Rc::new(|_interp, _this, args| {
                let y = args.first().map(to_number).unwrap_or(f64::NAN);
                let x = args.get(1).map(to_number).unwrap_or(f64::NAN);
                Completion::Normal(JsValue::Number(y.atan2(x)))
            }),
        ));
        math_obj
            .borrow_mut()
            .insert_builtin("atan2".to_string(), atan2_fn);

        // Math.hypot
        let hypot_fn = self.create_function(JsFunction::Native(
            "hypot".to_string(),
            Rc::new(|_interp, _this, args| {
                if args.is_empty() {
                    return Completion::Normal(JsValue::Number(0.0));
                }
                let mut sum = 0.0f64;
                for a in args {
                    let n = to_number(a);
                    if n.is_infinite() {
                        return Completion::Normal(JsValue::Number(f64::INFINITY));
                    }
                    if n.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    sum += n * n;
                }
                Completion::Normal(JsValue::Number(sum.sqrt()))
            }),
        ));
        math_obj
            .borrow_mut()
            .insert_builtin("hypot".to_string(), hypot_fn);

        // Math.log2, Math.log10
        let log2_fn = self.create_function(JsFunction::Native(
            "log2".to_string(),
            Rc::new(|_interp, _this, args| {
                let x = args.first().map(to_number).unwrap_or(f64::NAN);
                Completion::Normal(JsValue::Number(x.log2()))
            }),
        ));
        math_obj
            .borrow_mut()
            .insert_builtin("log2".to_string(), log2_fn);
        let log10_fn = self.create_function(JsFunction::Native(
            "log10".to_string(),
            Rc::new(|_interp, _this, args| {
                let x = args.first().map(to_number).unwrap_or(f64::NAN);
                Completion::Normal(JsValue::Number(x.log10()))
            }),
        ));
        math_obj
            .borrow_mut()
            .insert_builtin("log10".to_string(), log10_fn);

        // Math.fround
        let fround_fn = self.create_function(JsFunction::Native(
            "fround".to_string(),
            Rc::new(|_interp, _this, args| {
                let x = args.first().map(to_number).unwrap_or(f64::NAN);
                Completion::Normal(JsValue::Number((x as f32) as f64))
            }),
        ));
        math_obj
            .borrow_mut()
            .insert_builtin("fround".to_string(), fround_fn);

        // Math.clz32
        let clz32_fn = self.create_function(JsFunction::Native(
            "clz32".to_string(),
            Rc::new(|_interp, _this, args| {
                let x = args.first().map(to_number).unwrap_or(0.0);
                let n = number_ops::to_uint32(x);
                Completion::Normal(JsValue::Number(n.leading_zeros() as f64))
            }),
        ));
        math_obj
            .borrow_mut()
            .insert_builtin("clz32".to_string(), clz32_fn);

        // Math.imul
        let imul_fn = self.create_function(JsFunction::Native(
            "imul".to_string(),
            Rc::new(|_interp, _this, args| {
                let a = args.first().map(to_number).unwrap_or(0.0);
                let b = args.get(1).map(to_number).unwrap_or(0.0);
                let ia = number_ops::to_int32(a);
                let ib = number_ops::to_int32(b);
                Completion::Normal(JsValue::Number(ia.wrapping_mul(ib) as f64))
            }),
        ));
        math_obj
            .borrow_mut()
            .insert_builtin("imul".to_string(), imul_fn);

        // Math.expm1, Math.log1p, Math.cosh, Math.sinh, Math.tanh, Math.acosh, Math.asinh, Math.atanh
        let extra_math_fns: Vec<(&str, fn(f64) -> f64)> = vec![
            ("expm1", f64::exp_m1),
            ("log1p", f64::ln_1p),
            ("cosh", f64::cosh),
            ("sinh", f64::sinh),
            ("tanh", f64::tanh),
            ("acosh", f64::acosh),
            ("asinh", f64::asinh),
            ("atanh", f64::atanh),
        ];
        for (name, op) in extra_math_fns {
            let fn_val = self.create_function(JsFunction::Native(
                name.to_string(),
                Rc::new(move |_interp, _this, args| {
                    let x = args.first().map(to_number).unwrap_or(f64::NAN);
                    Completion::Normal(JsValue::Number(op(x)))
                }),
            ));
            math_obj.borrow_mut().insert_value(name.to_string(), fn_val);
        }

        let math_val = JsValue::Object(crate::types::JsObject { id: math_id });
        self.global_env
            .borrow_mut()
            .declare("Math", BindingKind::Const);
        let _ = self.global_env.borrow_mut().set("Math", math_val);

        // eval
        self.register_global_fn(
            "eval",
            BindingKind::Var,
            JsFunction::Native(
                "eval".to_string(),
                Rc::new(|interp, _this, args| {
                    let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                    if !matches!(&arg, JsValue::String(_)) {
                        return Completion::Normal(arg);
                    }
                    let code = to_js_string(&arg);
                    let mut p = match parser::Parser::new(&code) {
                        Ok(p) => p,
                        Err(_) => {
                            return Completion::Throw(
                                interp.create_error("SyntaxError", "Invalid eval source"),
                            );
                        }
                    };
                    let program = match p.parse_program() {
                        Ok(prog) => prog,
                        Err(_) => {
                            return Completion::Throw(
                                interp.create_error("SyntaxError", "Invalid eval source"),
                            );
                        }
                    };
                    let env = interp.global_env.clone();
                    let mut last = JsValue::Undefined;
                    for stmt in &program.body {
                        match interp.exec_statement(stmt, &env) {
                            Completion::Normal(v) => {
                                if !matches!(v, JsValue::Undefined) {
                                    last = v;
                                }
                            }
                            other => return other,
                        }
                    }
                    Completion::Normal(last)
                }),
            ),
        );

        // Symbol
        {
            let symbol_fn = self.create_function(JsFunction::Native(
                "Symbol".to_string(),
                Rc::new(|interp, _this, args| {
                    let desc = args.first().and_then(|v| {
                        if matches!(v, JsValue::Undefined) {
                            None
                        } else {
                            Some(JsString::from_str(&to_js_string(v)))
                        }
                    });
                    let id = interp.next_symbol_id;
                    interp.next_symbol_id += 1;
                    Completion::Normal(JsValue::Symbol(crate::types::JsSymbol {
                        id,
                        description: desc,
                    }))
                }),
            ));
            if let JsValue::Object(ref o) = symbol_fn {
                if let Some(obj) = self.get_object(o.id) {
                    let well_known = [
                        ("iterator", "Symbol.iterator"),
                        ("hasInstance", "Symbol.hasInstance"),
                        ("toPrimitive", "Symbol.toPrimitive"),
                        ("toStringTag", "Symbol.toStringTag"),
                        ("isConcatSpreadable", "Symbol.isConcatSpreadable"),
                        ("species", "Symbol.species"),
                        ("match", "Symbol.match"),
                        ("replace", "Symbol.replace"),
                        ("search", "Symbol.search"),
                        ("split", "Symbol.split"),
                        ("unscopables", "Symbol.unscopables"),
                    ];
                    for (name, desc) in well_known {
                        let id = self.next_symbol_id;
                        self.next_symbol_id += 1;
                        let sym = JsValue::Symbol(crate::types::JsSymbol {
                            id,
                            description: Some(JsString::from_str(desc)),
                        });
                        obj.borrow_mut().insert_value(name.to_string(), sym);
                    }
                }
            }
            self.global_env
                .borrow_mut()
                .declare("Symbol", BindingKind::Var);
            let _ = self.global_env.borrow_mut().set("Symbol", symbol_fn);
        }

        self.register_global_fn(
            "$DONOTEVALUATE",
            BindingKind::Var,
            JsFunction::Native(
                "$DONOTEVALUATE".to_string(),
                Rc::new(|_interp, _this, _args| {
                    Completion::Throw(JsValue::String(JsString::from_str(
                        "Test262: $DONOTEVALUATE was called",
                    )))
                }),
            ),
        );

        // JSON object
        let json_obj = self.create_object();
        let json_stringify = self.create_function(JsFunction::Native(
            "stringify".to_string(),
            Rc::new(|interp, _this, args: &[JsValue]| {
                let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let result = json_stringify_value(interp, &val);
                match result {
                    Some(s) => Completion::Normal(JsValue::String(JsString::from_str(&s))),
                    None => Completion::Normal(JsValue::Undefined),
                }
            }),
        ));
        let json_parse = self.create_function(JsFunction::Native(
            "parse".to_string(),
            Rc::new(|interp, _this, args: &[JsValue]| {
                let s = args.first().map(to_js_string).unwrap_or_default();
                json_parse_value(interp, &s)
            }),
        ));
        json_obj
            .borrow_mut()
            .insert_builtin("stringify".to_string(), json_stringify);
        json_obj
            .borrow_mut()
            .insert_builtin("parse".to_string(), json_parse);
        let json_val = JsValue::Object(crate::types::JsObject {
            id: self
                .objects
                .iter()
                .position(|o| Rc::ptr_eq(o, &json_obj))
                .unwrap() as u64,
        });
        self.global_env
            .borrow_mut()
            .declare("JSON", BindingKind::Var);
        let _ = self.global_env.borrow_mut().set("JSON", json_val);

        // String.fromCharCode
        {
            let string_ctor = self.global_env.borrow().get("String");
            if let Some(JsValue::Object(ref o)) = string_ctor {
                let from_char_code = self.create_function(JsFunction::Native(
                    "fromCharCode".to_string(),
                    Rc::new(|_interp, _this, args: &[JsValue]| {
                        let s: String = args
                            .iter()
                            .map(|a| {
                                let n = to_number(a) as u32;
                                char::from_u32(n).unwrap_or('\u{FFFD}')
                            })
                            .collect();
                        Completion::Normal(JsValue::String(JsString::from_str(&s)))
                    }),
                ));
                let from_code_point = self.create_function(JsFunction::Native(
                    "fromCodePoint".to_string(),
                    Rc::new(|_interp, _this, args: &[JsValue]| {
                        let mut s = String::new();
                        for a in args {
                            let n = to_number(a) as u32;
                            if let Some(c) = char::from_u32(n) {
                                s.push(c);
                            }
                        }
                        Completion::Normal(JsValue::String(JsString::from_str(&s)))
                    }),
                ));
                if let Some(obj) = self.get_object(o.id) {
                    obj.borrow_mut()
                        .insert_value("fromCharCode".to_string(), from_char_code);
                    obj.borrow_mut()
                        .insert_value("fromCodePoint".to_string(), from_code_point);
                }
            }
        }

        // RegExp constructor and prototype
        self.setup_regexp();

        // globalThis - create a global object
        let global_obj = self.create_object();
        let global_val = JsValue::Object(crate::types::JsObject {
            id: self
                .objects
                .iter()
                .position(|o| Rc::ptr_eq(o, &global_obj))
                .unwrap() as u64,
        });
        self.global_env
            .borrow_mut()
            .declare("globalThis", BindingKind::Var);
        let _ = self.global_env.borrow_mut().set("globalThis", global_val);
    }

    fn setup_regexp(&mut self) {
        let regexp_proto = self.create_object();
        regexp_proto.borrow_mut().class_name = "RegExp".to_string();

        // RegExp.prototype.test
        let test_fn = self.create_function(JsFunction::Native(
            "test".to_string(),
            Rc::new(|interp, this_val, args| {
                let input = args.first().map(to_js_string).unwrap_or_default();
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let source = if let JsValue::String(s) = obj.borrow().get_property("source")
                        {
                            s.to_rust_string()
                        } else {
                            String::new()
                        };
                        let flags = if let JsValue::String(s) = obj.borrow().get_property("flags") {
                            s.to_rust_string()
                        } else {
                            String::new()
                        };
                        let pattern = if flags.contains('i') {
                            format!("(?i){}", source)
                        } else {
                            source
                        };
                        if let Ok(re) = regex::Regex::new(&pattern) {
                            return Completion::Normal(JsValue::Boolean(re.is_match(&input)));
                        }
                    }
                }
                Completion::Normal(JsValue::Boolean(false))
            }),
        ));
        regexp_proto
            .borrow_mut()
            .insert_builtin("test".to_string(), test_fn);

        // RegExp.prototype.exec
        let exec_fn = self.create_function(JsFunction::Native(
            "exec".to_string(),
            Rc::new(|interp, this_val, args| {
                let input = args.first().map(to_js_string).unwrap_or_default();
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let source = if let JsValue::String(s) = obj.borrow().get_property("source")
                        {
                            s.to_rust_string()
                        } else {
                            String::new()
                        };
                        let flags = if let JsValue::String(s) = obj.borrow().get_property("flags") {
                            s.to_rust_string()
                        } else {
                            String::new()
                        };
                        let pattern = if flags.contains('i') {
                            format!("(?i){}", source)
                        } else {
                            source
                        };
                        if let Ok(re) = regex::Regex::new(&pattern) {
                            if let Some(m) = re.find(&input) {
                                let matched = JsValue::String(JsString::from_str(m.as_str()));
                                let result = interp.create_array(vec![matched]);
                                if let JsValue::Object(ref ro) = result {
                                    if let Some(robj) = interp.get_object(ro.id) {
                                        robj.borrow_mut().insert_value(
                                            "index".to_string(),
                                            JsValue::Number(m.start() as f64),
                                        );
                                        robj.borrow_mut().insert_value(
                                            "input".to_string(),
                                            JsValue::String(JsString::from_str(&input)),
                                        );
                                    }
                                }
                                return Completion::Normal(result);
                            }
                        }
                    }
                }
                Completion::Normal(JsValue::Null)
            }),
        ));
        regexp_proto
            .borrow_mut()
            .insert_builtin("exec".to_string(), exec_fn);

        // RegExp.prototype.toString
        let tostring_fn = self.create_function(JsFunction::Native(
            "toString".to_string(),
            Rc::new(|interp, this_val, _args| {
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let source = if let JsValue::String(s) = obj.borrow().get_property("source")
                        {
                            s.to_rust_string()
                        } else {
                            String::new()
                        };
                        let flags = if let JsValue::String(s) = obj.borrow().get_property("flags") {
                            s.to_rust_string()
                        } else {
                            String::new()
                        };
                        return Completion::Normal(JsValue::String(JsString::from_str(&format!(
                            "/{}/{}",
                            source, flags
                        ))));
                    }
                }
                Completion::Normal(JsValue::String(JsString::from_str("/(?:)/")))
            }),
        ));
        regexp_proto
            .borrow_mut()
            .insert_builtin("toString".to_string(), tostring_fn);

        let regexp_proto_rc = regexp_proto.clone();

        // RegExp constructor
        let regexp_ctor = self.create_function(JsFunction::Native(
            "RegExp".to_string(),
            Rc::new(move |interp, _this, args| {
                let pattern_str = args.first().map(to_js_string).unwrap_or_default();
                let flags_str = args.get(1).map(to_js_string).unwrap_or_default();
                let mut obj = JsObjectData::new();
                obj.prototype = Some(regexp_proto_rc.clone());
                obj.class_name = "RegExp".to_string();
                obj.insert_value(
                    "source".to_string(),
                    JsValue::String(JsString::from_str(&pattern_str)),
                );
                obj.insert_value(
                    "flags".to_string(),
                    JsValue::String(JsString::from_str(&flags_str)),
                );
                obj.insert_value(
                    "global".to_string(),
                    JsValue::Boolean(flags_str.contains('g')),
                );
                obj.insert_value(
                    "ignoreCase".to_string(),
                    JsValue::Boolean(flags_str.contains('i')),
                );
                obj.insert_value(
                    "multiline".to_string(),
                    JsValue::Boolean(flags_str.contains('m')),
                );
                obj.insert_value("lastIndex".to_string(), JsValue::Number(0.0));
                let rc = Rc::new(RefCell::new(obj));
                interp.objects.push(rc);
                Completion::Normal(JsValue::Object(crate::types::JsObject {
                    id: interp.objects.len() as u64 - 1,
                }))
            }),
        ));
        // Set prototype on constructor
        if let JsValue::Object(ref o) = regexp_ctor {
            if let Some(obj) = self.get_object(o.id) {
                obj.borrow_mut().insert_value(
                    "prototype".to_string(),
                    JsValue::Object(crate::types::JsObject {
                        id: self
                            .objects
                            .iter()
                            .position(|o| Rc::ptr_eq(o, &regexp_proto))
                            .unwrap() as u64,
                    }),
                );
            }
        }
        self.global_env
            .borrow_mut()
            .declare("RegExp", BindingKind::Var);
        let _ = self.global_env.borrow_mut().set("RegExp", regexp_ctor);

        self.regexp_prototype = Some(regexp_proto);
    }

    fn setup_object_statics(&mut self) {
        // Get the Object function from global env
        let obj_func_val = self
            .global_env
            .borrow()
            .get("Object")
            .unwrap_or(JsValue::Undefined);
        if let JsValue::Object(ref o) = obj_func_val {
            if let Some(obj_func) = self.get_object(o.id) {
                // Get prototype property
                let proto_val = obj_func.borrow().get_property_value("prototype");
                if let Some(JsValue::Object(ref proto_ref)) = proto_val {
                    if let Some(proto_obj) = self.get_object(proto_ref.id) {
                        self.object_prototype = Some(proto_obj.clone());

                        // Add hasOwnProperty to Object.prototype
                        let has_own_fn = self.create_function(JsFunction::Native(
                            "hasOwnProperty".to_string(),
                            Rc::new(|interp, this_val, args| {
                                let key = args.first().map(to_js_string).unwrap_or_default();
                                if let JsValue::Object(o) = this_val {
                                    if let Some(obj) = interp.get_object(o.id) {
                                        return Completion::Normal(JsValue::Boolean(
                                            obj.borrow().has_own_property(&key),
                                        ));
                                    }
                                }
                                Completion::Normal(JsValue::Boolean(false))
                            }),
                        ));
                        proto_obj
                            .borrow_mut()
                            .insert_builtin("hasOwnProperty".to_string(), has_own_fn);

                        // Object.prototype.toString
                        let obj_tostring_fn = self.create_function(JsFunction::Native(
                            "toString".to_string(),
                            Rc::new(|interp, this_val, _args| {
                                let tag = match this_val {
                                    JsValue::Object(o) => {
                                        if let Some(obj) = interp.get_object(o.id) {
                                            let cn = obj.borrow().class_name.clone();
                                            if cn == "Object" && obj.borrow().callable.is_some() {
                                                "Function".to_string()
                                            } else {
                                                cn
                                            }
                                        } else {
                                            "Object".to_string()
                                        }
                                    }
                                    JsValue::Undefined => "Undefined".to_string(),
                                    JsValue::Null => "Null".to_string(),
                                    JsValue::Boolean(_) => "Boolean".to_string(),
                                    JsValue::Number(_) => "Number".to_string(),
                                    JsValue::String(_) => "String".to_string(),
                                    JsValue::Symbol(_) => "Symbol".to_string(),
                                    JsValue::BigInt(_) => "BigInt".to_string(),
                                };
                                Completion::Normal(JsValue::String(JsString::from_str(&format!(
                                    "[object {tag}]"
                                ))))
                            }),
                        ));
                        proto_obj
                            .borrow_mut()
                            .insert_builtin("toString".to_string(), obj_tostring_fn);

                        // Object.prototype.valueOf
                        let obj_valueof_fn = self.create_function(JsFunction::Native(
                            "valueOf".to_string(),
                            Rc::new(|interp, this_val, _args| {
                                if let JsValue::Object(o) = this_val {
                                    if let Some(obj) = interp.get_object(o.id) {
                                        if let Some(pv) = obj.borrow().primitive_value.clone() {
                                            return Completion::Normal(pv);
                                        }
                                    }
                                }
                                Completion::Normal(this_val.clone())
                            }),
                        ));
                        proto_obj
                            .borrow_mut()
                            .insert_builtin("valueOf".to_string(), obj_valueof_fn);

                        // Object.prototype.propertyIsEnumerable
                        let pie_fn = self.create_function(JsFunction::Native(
                            "propertyIsEnumerable".to_string(),
                            Rc::new(|interp, this_val, args| {
                                let key = args.first().map(to_js_string).unwrap_or_default();
                                if let JsValue::Object(o) = this_val {
                                    if let Some(obj) = interp.get_object(o.id) {
                                        if let Some(desc) = obj.borrow().get_own_property(&key) {
                                            return Completion::Normal(JsValue::Boolean(
                                                desc.enumerable != Some(false),
                                            ));
                                        }
                                    }
                                }
                                Completion::Normal(JsValue::Boolean(false))
                            }),
                        ));
                        proto_obj
                            .borrow_mut()
                            .insert_builtin("propertyIsEnumerable".to_string(), pie_fn);

                        // Object.prototype.isPrototypeOf
                        let ipof_fn = self.create_function(JsFunction::Native(
                            "isPrototypeOf".to_string(),
                            Rc::new(|interp, this_val, args| {
                                let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                                if let (JsValue::Object(this_o), JsValue::Object(target_o)) =
                                    (this_val, &target)
                                {
                                    if let (Some(this_data), Some(target_data)) = (
                                        interp.get_object(this_o.id),
                                        interp.get_object(target_o.id),
                                    ) {
                                        let mut current = target_data.borrow().prototype.clone();
                                        while let Some(p) = current {
                                            if Rc::ptr_eq(&p, &this_data) {
                                                return Completion::Normal(JsValue::Boolean(true));
                                            }
                                            current = p.borrow().prototype.clone();
                                        }
                                    }
                                }
                                Completion::Normal(JsValue::Boolean(false))
                            }),
                        ));
                        proto_obj
                            .borrow_mut()
                            .insert_builtin("isPrototypeOf".to_string(), ipof_fn);

                        self.setup_function_prototype(&proto_obj);
                    }
                }

                // Add Object.defineProperty
                let define_property_fn = self.create_function(JsFunction::Native(
                    "defineProperty".to_string(),
                    Rc::new(|interp, _this, args| {
                        let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let key = args.get(1).map(to_js_string).unwrap_or_default();
                        let desc_val = args.get(2).cloned().unwrap_or(JsValue::Undefined);
                        if let JsValue::Object(ref o) = target {
                            if let Some(obj) = interp.get_object(o.id) {
                                let mut desc = PropertyDescriptor {
                                    value: None,
                                    writable: None,
                                    get: None,
                                    set: None,
                                    enumerable: None,
                                    configurable: None,
                                };
                                if let JsValue::Object(ref d) = desc_val {
                                    if let Some(desc_obj) = interp.get_object(d.id) {
                                        let b = desc_obj.borrow();
                                        let v = b.get_property("value");
                                        if !matches!(v, JsValue::Undefined)
                                            || b.has_own_property("value")
                                        {
                                            desc.value = Some(v);
                                        }
                                        let w = b.get_property("writable");
                                        if !matches!(w, JsValue::Undefined)
                                            || b.has_own_property("writable")
                                        {
                                            desc.writable = Some(to_boolean(&w));
                                        }
                                        let e = b.get_property("enumerable");
                                        if !matches!(e, JsValue::Undefined)
                                            || b.has_own_property("enumerable")
                                        {
                                            desc.enumerable = Some(to_boolean(&e));
                                        }
                                        let c = b.get_property("configurable");
                                        if !matches!(c, JsValue::Undefined)
                                            || b.has_own_property("configurable")
                                        {
                                            desc.configurable = Some(to_boolean(&c));
                                        }
                                        let g = b.get_property("get");
                                        if !matches!(g, JsValue::Undefined)
                                            || b.has_own_property("get")
                                        {
                                            desc.get = Some(g);
                                        }
                                        let s = b.get_property("set");
                                        if !matches!(s, JsValue::Undefined)
                                            || b.has_own_property("set")
                                        {
                                            desc.set = Some(s);
                                        }
                                    }
                                }
                                obj.borrow_mut().define_own_property(key, desc);
                            }
                        }
                        Completion::Normal(target)
                    }),
                ));
                obj_func
                    .borrow_mut()
                    .insert_value("defineProperty".to_string(), define_property_fn);

                // Add Object.getOwnPropertyDescriptor
                let get_own_prop_desc_fn = self.create_function(JsFunction::Native(
                    "getOwnPropertyDescriptor".to_string(),
                    Rc::new(|interp, _this, args| {
                        let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let key = args.get(1).map(to_js_string).unwrap_or_default();
                        if let JsValue::Object(ref o) = target {
                            if let Some(obj) = interp.get_object(o.id) {
                                if let Some(desc) = obj.borrow().get_own_property(&key).cloned() {
                                    let result = interp.create_object();
                                    {
                                        let mut r = result.borrow_mut();
                                        if let Some(val) = desc.value {
                                            r.insert_value("value".to_string(), val);
                                        }
                                        if let Some(w) = desc.writable {
                                            r.insert_value(
                                                "writable".to_string(),
                                                JsValue::Boolean(w),
                                            );
                                        }
                                        if let Some(e) = desc.enumerable {
                                            r.insert_value(
                                                "enumerable".to_string(),
                                                JsValue::Boolean(e),
                                            );
                                        }
                                        if let Some(c) = desc.configurable {
                                            r.insert_value(
                                                "configurable".to_string(),
                                                JsValue::Boolean(c),
                                            );
                                        }
                                        if let Some(g) = desc.get {
                                            r.insert_value("get".to_string(), g);
                                        }
                                        if let Some(s) = desc.set {
                                            r.insert_value("set".to_string(), s);
                                        }
                                    }
                                    return Completion::Normal(JsValue::Object(
                                        crate::types::JsObject {
                                            id: interp.objects.len() as u64 - 1,
                                        },
                                    ));
                                }
                            }
                        }
                        Completion::Normal(JsValue::Undefined)
                    }),
                ));
                obj_func
                    .borrow_mut()
                    .insert_value("getOwnPropertyDescriptor".to_string(), get_own_prop_desc_fn);

                // Add Object.keys
                let keys_fn = self.create_function(JsFunction::Native(
                    "keys".to_string(),
                    Rc::new(|interp, _this, args| {
                        let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if let JsValue::Object(ref o) = target {
                            if let Some(obj) = interp.get_object(o.id) {
                                let borrowed = obj.borrow();
                                let keys: Vec<JsValue> = borrowed
                                    .property_order
                                    .iter()
                                    .filter(|k| {
                                        borrowed
                                            .properties
                                            .get(*k)
                                            .map_or(false, |d| d.enumerable != Some(false))
                                    })
                                    .map(|k| JsValue::String(JsString::from_str(k)))
                                    .collect();
                                let arr = interp.create_array(keys);
                                return Completion::Normal(arr);
                            }
                        }
                        Completion::Normal(JsValue::Undefined)
                    }),
                ));
                obj_func
                    .borrow_mut()
                    .insert_value("keys".to_string(), keys_fn);

                // Add Object.freeze
                let freeze_fn = self.create_function(JsFunction::Native(
                    "freeze".to_string(),
                    Rc::new(|interp, _this, args| {
                        let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if let JsValue::Object(ref o) = target {
                            if let Some(obj) = interp.get_object(o.id) {
                                let mut o = obj.borrow_mut();
                                o.extensible = false;
                                for desc in o.properties.values_mut() {
                                    desc.configurable = Some(false);
                                    if desc.is_data_descriptor() {
                                        desc.writable = Some(false);
                                    }
                                }
                            }
                        }
                        Completion::Normal(target)
                    }),
                ));
                obj_func
                    .borrow_mut()
                    .insert_value("freeze".to_string(), freeze_fn);

                // Add Object.getPrototypeOf
                let get_proto_fn = self.create_function(JsFunction::Native(
                    "getPrototypeOf".to_string(),
                    Rc::new(|interp, _this, args| {
                        let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if let JsValue::Object(ref o) = target {
                            if let Some(obj) = interp.get_object(o.id) {
                                if let Some(proto) = &obj.borrow().prototype {
                                    // Find the id of the prototype object
                                    for (i, stored) in interp.objects.iter().enumerate() {
                                        if Rc::ptr_eq(stored, proto) {
                                            return Completion::Normal(JsValue::Object(
                                                crate::types::JsObject { id: i as u64 },
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                        Completion::Normal(JsValue::Null)
                    }),
                ));
                obj_func
                    .borrow_mut()
                    .insert_value("getPrototypeOf".to_string(), get_proto_fn);

                // Add Object.create
                let create_fn = self.create_function(JsFunction::Native(
                    "create".to_string(),
                    Rc::new(|interp, _this, args| {
                        let proto_arg = args.first().cloned().unwrap_or(JsValue::Null);
                        let new_obj = interp.create_object();
                        match &proto_arg {
                            JsValue::Object(o) => {
                                if let Some(proto_rc) = interp.get_object(o.id) {
                                    new_obj.borrow_mut().prototype = Some(proto_rc);
                                }
                            }
                            JsValue::Null => {
                                new_obj.borrow_mut().prototype = None;
                            }
                            _ => {}
                        }
                        Completion::Normal(JsValue::Object(crate::types::JsObject {
                            id: interp.objects.len() as u64 - 1,
                        }))
                    }),
                ));
                obj_func
                    .borrow_mut()
                    .insert_value("create".to_string(), create_fn);

                // Object.entries
                let entries_fn = self.create_function(JsFunction::Native(
                    "entries".to_string(),
                    Rc::new(|interp, _this, args| {
                        let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if let JsValue::Object(o) = &target {
                            if let Some(obj) = interp.get_object(o.id) {
                                let borrowed = obj.borrow();
                                let pairs: Vec<_> = borrowed
                                    .property_order
                                    .iter()
                                    .filter_map(|k| {
                                        let desc = borrowed.properties.get(k)?;
                                        if desc.enumerable == Some(false) {
                                            return None;
                                        }
                                        let key = JsValue::String(JsString::from_str(k));
                                        let val = desc.value.clone().unwrap_or(JsValue::Undefined);
                                        Some((key, val))
                                    })
                                    .collect();
                                drop(borrowed);
                                let entries: Vec<JsValue> = pairs
                                    .into_iter()
                                    .map(|(key, val)| interp.create_array(vec![key, val]))
                                    .collect();
                                let arr = interp.create_array(entries);
                                return Completion::Normal(arr);
                            }
                        }
                        Completion::Normal(interp.create_array(Vec::new()))
                    }),
                ));
                obj_func
                    .borrow_mut()
                    .insert_value("entries".to_string(), entries_fn);

                // Object.values
                let values_fn = self.create_function(JsFunction::Native(
                    "values".to_string(),
                    Rc::new(|interp, _this, args| {
                        let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if let JsValue::Object(o) = &target {
                            if let Some(obj) = interp.get_object(o.id) {
                                let borrowed = obj.borrow();
                                let values: Vec<JsValue> = borrowed
                                    .property_order
                                    .iter()
                                    .filter_map(|k| {
                                        let desc = borrowed.properties.get(k)?;
                                        if desc.enumerable == Some(false) {
                                            return None;
                                        }
                                        Some(desc.value.clone().unwrap_or(JsValue::Undefined))
                                    })
                                    .collect();
                                let arr = interp.create_array(values);
                                return Completion::Normal(arr);
                            }
                        }
                        Completion::Normal(interp.create_array(Vec::new()))
                    }),
                ));
                obj_func
                    .borrow_mut()
                    .insert_value("values".to_string(), values_fn);

                // Object.assign
                let assign_fn = self.create_function(JsFunction::Native(
                    "assign".to_string(),
                    Rc::new(|interp, _this, args| {
                        let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if let JsValue::Object(t) = &target {
                            for source in args.iter().skip(1) {
                                if let JsValue::Object(s) = source {
                                    if let Some(src_obj) = interp.get_object(s.id) {
                                        let borrowed = src_obj.borrow();
                                        let props: Vec<(String, JsValue)> = borrowed
                                            .property_order
                                            .iter()
                                            .filter_map(|k| {
                                                let desc = borrowed.properties.get(k)?;
                                                if desc.enumerable == Some(false) {
                                                    return None;
                                                }
                                                Some((
                                                    k.clone(),
                                                    desc.value
                                                        .clone()
                                                        .unwrap_or(JsValue::Undefined),
                                                ))
                                            })
                                            .collect();
                                        drop(borrowed);
                                        if let Some(tgt_obj) = interp.get_object(t.id) {
                                            let mut tgt = tgt_obj.borrow_mut();
                                            for (k, v) in props {
                                                tgt.insert_value(k, v);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Completion::Normal(target)
                    }),
                ));
                obj_func
                    .borrow_mut()
                    .insert_value("assign".to_string(), assign_fn);

                // Object.is
                let is_fn = self.create_function(JsFunction::Native(
                    "is".to_string(),
                    Rc::new(|_interp, _this, args| {
                        let a = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let b = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                        let result = match (&a, &b) {
                            (JsValue::Number(x), JsValue::Number(y)) => {
                                number_ops::same_value(*x, *y)
                            }
                            _ => strict_equality(&a, &b),
                        };
                        Completion::Normal(JsValue::Boolean(result))
                    }),
                ));
                obj_func.borrow_mut().insert_value("is".to_string(), is_fn);

                // Object.getOwnPropertyNames
                let gopn_fn = self.create_function(JsFunction::Native(
                    "getOwnPropertyNames".to_string(),
                    Rc::new(|interp, _this, args| {
                        let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if let JsValue::Object(o) = &target {
                            if let Some(obj) = interp.get_object(o.id) {
                                let names: Vec<JsValue> = obj
                                    .borrow()
                                    .property_order
                                    .iter()
                                    .map(|k| JsValue::String(JsString::from_str(k)))
                                    .collect();
                                let arr = interp.create_array(names);
                                return Completion::Normal(arr);
                            }
                        }
                        Completion::Normal(interp.create_array(Vec::new()))
                    }),
                ));
                obj_func
                    .borrow_mut()
                    .insert_value("getOwnPropertyNames".to_string(), gopn_fn);

                // Object.preventExtensions
                let pe_fn = self.create_function(JsFunction::Native(
                    "preventExtensions".to_string(),
                    Rc::new(|interp, _this, args| {
                        let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if let JsValue::Object(o) = &target {
                            if let Some(obj) = interp.get_object(o.id) {
                                obj.borrow_mut().extensible = false;
                            }
                        }
                        Completion::Normal(target)
                    }),
                ));
                obj_func
                    .borrow_mut()
                    .insert_value("preventExtensions".to_string(), pe_fn);

                // Object.isExtensible
                let ie_fn = self.create_function(JsFunction::Native(
                    "isExtensible".to_string(),
                    Rc::new(|interp, _this, args| {
                        let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if let JsValue::Object(o) = &target {
                            if let Some(obj) = interp.get_object(o.id) {
                                return Completion::Normal(JsValue::Boolean(
                                    obj.borrow().extensible,
                                ));
                            }
                        }
                        Completion::Normal(JsValue::Boolean(false))
                    }),
                ));
                obj_func
                    .borrow_mut()
                    .insert_value("isExtensible".to_string(), ie_fn);

                // Object.isFrozen
                let frozen_fn = self.create_function(JsFunction::Native(
                    "isFrozen".to_string(),
                    Rc::new(|interp, _this, args| {
                        let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if let JsValue::Object(o) = &target {
                            if let Some(obj) = interp.get_object(o.id) {
                                let obj_ref = obj.borrow();
                                if obj_ref.extensible {
                                    return Completion::Normal(JsValue::Boolean(false));
                                }
                                let all_frozen = obj_ref.properties.values().all(|d| {
                                    d.configurable == Some(false)
                                        && (!d.is_data_descriptor() || d.writable == Some(false))
                                });
                                return Completion::Normal(JsValue::Boolean(all_frozen));
                            }
                        }
                        Completion::Normal(JsValue::Boolean(true))
                    }),
                ));
                obj_func
                    .borrow_mut()
                    .insert_value("isFrozen".to_string(), frozen_fn);

                // Object.isSealed
                let sealed_fn = self.create_function(JsFunction::Native(
                    "isSealed".to_string(),
                    Rc::new(|interp, _this, args| {
                        let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if let JsValue::Object(o) = &target {
                            if let Some(obj) = interp.get_object(o.id) {
                                let obj_ref = obj.borrow();
                                if obj_ref.extensible {
                                    return Completion::Normal(JsValue::Boolean(false));
                                }
                                let all_sealed = obj_ref
                                    .properties
                                    .values()
                                    .all(|d| d.configurable == Some(false));
                                return Completion::Normal(JsValue::Boolean(all_sealed));
                            }
                        }
                        Completion::Normal(JsValue::Boolean(true))
                    }),
                ));
                obj_func
                    .borrow_mut()
                    .insert_value("isSealed".to_string(), sealed_fn);

                // Object.seal
                let seal_fn = self.create_function(JsFunction::Native(
                    "seal".to_string(),
                    Rc::new(|interp, _this, args| {
                        let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if let JsValue::Object(o) = &target {
                            if let Some(obj) = interp.get_object(o.id) {
                                let mut obj_mut = obj.borrow_mut();
                                obj_mut.extensible = false;
                                for desc in obj_mut.properties.values_mut() {
                                    desc.configurable = Some(false);
                                }
                            }
                        }
                        Completion::Normal(target)
                    }),
                ));
                obj_func
                    .borrow_mut()
                    .insert_value("seal".to_string(), seal_fn);

                // Object.hasOwn
                let has_own_fn = self.create_function(JsFunction::Native(
                    "hasOwn".to_string(),
                    Rc::new(|interp, _this, args| {
                        let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let key = args.get(1).map(to_js_string).unwrap_or_default();
                        if let JsValue::Object(o) = &target {
                            if let Some(obj) = interp.get_object(o.id) {
                                return Completion::Normal(JsValue::Boolean(
                                    obj.borrow().has_own_property(&key),
                                ));
                            }
                        }
                        Completion::Normal(JsValue::Boolean(false))
                    }),
                ));
                obj_func
                    .borrow_mut()
                    .insert_value("hasOwn".to_string(), has_own_fn);

                // Object.setPrototypeOf
                let set_proto_fn = self.create_function(JsFunction::Native(
                    "setPrototypeOf".to_string(),
                    Rc::new(|interp, _this, args| {
                        let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let proto = args.get(1).cloned().unwrap_or(JsValue::Null);
                        if let JsValue::Object(ref o) = target {
                            if let Some(obj) = interp.get_object(o.id) {
                                match &proto {
                                    JsValue::Null => {
                                        obj.borrow_mut().prototype = None;
                                    }
                                    JsValue::Object(p) => {
                                        if let Some(proto_obj) = interp.get_object(p.id) {
                                            obj.borrow_mut().prototype = Some(proto_obj);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Completion::Normal(target)
                    }),
                ));
                obj_func
                    .borrow_mut()
                    .insert_value("setPrototypeOf".to_string(), set_proto_fn);

                // Object.defineProperties
                let def_props_fn = self.create_function(JsFunction::Native(
                    "defineProperties".to_string(),
                    Rc::new(|interp, _this, args| {
                        let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let descs = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                        if let JsValue::Object(ref t) = target {
                            if let JsValue::Object(ref d) = descs {
                                if let Some(desc_obj) = interp.get_object(d.id) {
                                    let keys: Vec<String> =
                                        desc_obj.borrow().properties.keys().cloned().collect();
                                    for key in keys {
                                        let prop_desc_val = desc_obj.borrow().get_property(&key);
                                        if let JsValue::Object(ref pd) = prop_desc_val {
                                            if let Some(pd_obj) = interp.get_object(pd.id) {
                                                let b = pd_obj.borrow();
                                                let mut desc = PropertyDescriptor {
                                                    value: None,
                                                    writable: None,
                                                    get: None,
                                                    set: None,
                                                    enumerable: None,
                                                    configurable: None,
                                                };
                                                let v = b.get_property("value");
                                                if !matches!(v, JsValue::Undefined)
                                                    || b.has_own_property("value")
                                                {
                                                    desc.value = Some(v);
                                                }
                                                let w = b.get_property("writable");
                                                if !matches!(w, JsValue::Undefined)
                                                    || b.has_own_property("writable")
                                                {
                                                    desc.writable = Some(to_boolean(&w));
                                                }
                                                let e = b.get_property("enumerable");
                                                if !matches!(e, JsValue::Undefined)
                                                    || b.has_own_property("enumerable")
                                                {
                                                    desc.enumerable = Some(to_boolean(&e));
                                                }
                                                let c = b.get_property("configurable");
                                                if !matches!(c, JsValue::Undefined)
                                                    || b.has_own_property("configurable")
                                                {
                                                    desc.configurable = Some(to_boolean(&c));
                                                }
                                                let g = b.get_property("get");
                                                if !matches!(g, JsValue::Undefined)
                                                    || b.has_own_property("get")
                                                {
                                                    desc.get = Some(g);
                                                }
                                                let s = b.get_property("set");
                                                if !matches!(s, JsValue::Undefined)
                                                    || b.has_own_property("set")
                                                {
                                                    desc.set = Some(s);
                                                }
                                                drop(b);
                                                if let Some(target_obj) = interp.get_object(t.id) {
                                                    target_obj
                                                        .borrow_mut()
                                                        .insert_property(key, desc);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Completion::Normal(target)
                    }),
                ));
                obj_func
                    .borrow_mut()
                    .insert_value("defineProperties".to_string(), def_props_fn);

                // Object.getOwnPropertyDescriptors
                let get_descs_fn = self.create_function(JsFunction::Native(
                    "getOwnPropertyDescriptors".to_string(),
                    Rc::new(|interp, _this, args| {
                        let target = args.first().cloned().unwrap_or(JsValue::Undefined);
                        if let JsValue::Object(ref t) = target {
                            if let Some(obj) = interp.get_object(t.id) {
                                let result = interp.create_object();
                                let keys: Vec<String> =
                                    obj.borrow().properties.keys().cloned().collect();
                                for key in keys {
                                    let desc = obj.borrow().properties.get(&key).cloned();
                                    if let Some(d) = desc {
                                        let desc_result = interp.create_object();
                                        if let Some(ref v) = d.value {
                                            desc_result
                                                .borrow_mut()
                                                .insert_value("value".to_string(), v.clone());
                                        }
                                        if let Some(w) = d.writable {
                                            desc_result.borrow_mut().insert_value(
                                                "writable".to_string(),
                                                JsValue::Boolean(w),
                                            );
                                        }
                                        if let Some(e) = d.enumerable {
                                            desc_result.borrow_mut().insert_value(
                                                "enumerable".to_string(),
                                                JsValue::Boolean(e),
                                            );
                                        }
                                        if let Some(c) = d.configurable {
                                            desc_result.borrow_mut().insert_value(
                                                "configurable".to_string(),
                                                JsValue::Boolean(c),
                                            );
                                        }
                                        if let Some(ref g) = d.get {
                                            desc_result
                                                .borrow_mut()
                                                .insert_value("get".to_string(), g.clone());
                                        }
                                        if let Some(ref s) = d.set {
                                            desc_result
                                                .borrow_mut()
                                                .insert_value("set".to_string(), s.clone());
                                        }
                                        let did = interp
                                            .objects
                                            .iter()
                                            .position(|o| Rc::ptr_eq(o, &desc_result))
                                            .unwrap()
                                            as u64;
                                        let dval =
                                            JsValue::Object(crate::types::JsObject { id: did });
                                        result.borrow_mut().insert_value(key, dval);
                                    }
                                }
                                let id = interp
                                    .objects
                                    .iter()
                                    .position(|o| Rc::ptr_eq(o, &result))
                                    .unwrap() as u64;
                                return Completion::Normal(JsValue::Object(
                                    crate::types::JsObject { id },
                                ));
                            }
                        }
                        Completion::Normal(JsValue::Undefined)
                    }),
                ));
                obj_func
                    .borrow_mut()
                    .insert_value("getOwnPropertyDescriptors".to_string(), get_descs_fn);

                // Object.fromEntries
                let from_entries_fn = self.create_function(JsFunction::Native(
                    "fromEntries".to_string(),
                    Rc::new(|interp, _this, args| {
                        let iterable = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let obj = interp.create_object();
                        if let JsValue::Object(ref arr) = iterable {
                            if let Some(arr_obj) = interp.get_object(arr.id) {
                                let len = if let Some(JsValue::Number(n)) =
                                    arr_obj.borrow().get_property_value("length")
                                {
                                    n as usize
                                } else {
                                    0
                                };
                                for i in 0..len {
                                    let entry = arr_obj.borrow().get_property(&i.to_string());
                                    if let JsValue::Object(ref e) = entry {
                                        if let Some(e_obj) = interp.get_object(e.id) {
                                            let k = to_js_string(&e_obj.borrow().get_property("0"));
                                            let v = e_obj.borrow().get_property("1");
                                            obj.borrow_mut().insert_value(k, v);
                                        }
                                    }
                                }
                            }
                        }
                        let id = interp
                            .objects
                            .iter()
                            .position(|o| Rc::ptr_eq(o, &obj))
                            .unwrap() as u64;
                        Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
                    }),
                ));
                obj_func
                    .borrow_mut()
                    .insert_value("fromEntries".to_string(), from_entries_fn);
            }
        }
    }

    fn setup_array_prototype(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "Array".to_string();

        // Array.prototype.push
        let push_fn = self.create_function(JsFunction::Native(
            "push".to_string(),
            Rc::new(|interp, this_val, args| {
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let mut obj_mut = obj.borrow_mut();
                        if let Some(ref mut elems) = obj_mut.array_elements {
                            for arg in args {
                                elems.push(arg.clone());
                            }
                            let len = elems.len() as f64;
                            obj_mut.insert_value("length".to_string(), JsValue::Number(len));
                            return Completion::Normal(JsValue::Number(len));
                        }
                    }
                }
                Completion::Normal(JsValue::Undefined)
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("push".to_string(), push_fn);

        // Array.prototype.pop
        let pop_fn = self.create_function(JsFunction::Native(
            "pop".to_string(),
            Rc::new(|interp, this_val, args: &[JsValue]| {
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let mut obj_mut = obj.borrow_mut();
                        if let Some(ref mut elems) = obj_mut.array_elements {
                            let val = elems.pop().unwrap_or(JsValue::Undefined);
                            let len = elems.len() as f64;
                            obj_mut.insert_value("length".to_string(), JsValue::Number(len));
                            return Completion::Normal(val);
                        }
                    }
                }
                Completion::Normal(JsValue::Undefined)
            }),
        ));
        proto.borrow_mut().insert_builtin("pop".to_string(), pop_fn);

        // Array.prototype.shift
        let shift_fn = self.create_function(JsFunction::Native(
            "shift".to_string(),
            Rc::new(|interp, this_val, _args| {
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let mut obj_mut = obj.borrow_mut();
                        if let Some(ref mut elems) = obj_mut.array_elements {
                            if elems.is_empty() {
                                return Completion::Normal(JsValue::Undefined);
                            }
                            let val = elems.remove(0);
                            let len = elems.len() as f64;
                            obj_mut.insert_value("length".to_string(), JsValue::Number(len));
                            return Completion::Normal(val);
                        }
                    }
                }
                Completion::Normal(JsValue::Undefined)
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("shift".to_string(), shift_fn);

        // Array.prototype.unshift
        let unshift_fn = self.create_function(JsFunction::Native(
            "unshift".to_string(),
            Rc::new(|interp, this_val, args| {
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let mut obj_mut = obj.borrow_mut();
                        if let Some(ref mut elems) = obj_mut.array_elements {
                            for (i, arg) in args.iter().rev().enumerate() {
                                let _ = i;
                                elems.insert(0, arg.clone());
                            }
                            let len = elems.len() as f64;
                            obj_mut.insert_value("length".to_string(), JsValue::Number(len));
                            return Completion::Normal(JsValue::Number(len));
                        }
                    }
                }
                Completion::Normal(JsValue::Undefined)
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("unshift".to_string(), unshift_fn);

        // Array.prototype.indexOf
        let indexof_fn = self.create_function(JsFunction::Native(
            "indexOf".to_string(),
            Rc::new(|interp, this_val, args| {
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let obj_ref = obj.borrow();
                        if let Some(ref elems) = obj_ref.array_elements {
                            let search = args.first().cloned().unwrap_or(JsValue::Undefined);
                            let from = args.get(1).map(|v| to_number(v) as i64).unwrap_or(0);
                            let start = if from < 0 {
                                (elems.len() as i64 + from).max(0) as usize
                            } else {
                                from as usize
                            };
                            for i in start..elems.len() {
                                if strict_equality(&elems[i], &search) {
                                    return Completion::Normal(JsValue::Number(i as f64));
                                }
                            }
                        }
                    }
                }
                Completion::Normal(JsValue::Number(-1.0))
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("indexOf".to_string(), indexof_fn);

        // Array.prototype.lastIndexOf
        let lastindexof_fn = self.create_function(JsFunction::Native(
            "lastIndexOf".to_string(),
            Rc::new(|interp, this_val, args| {
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let obj_ref = obj.borrow();
                        if let Some(ref elems) = obj_ref.array_elements {
                            let search = args.first().cloned().unwrap_or(JsValue::Undefined);
                            let len = elems.len() as i64;
                            let from = args.get(1).map(|v| to_number(v) as i64).unwrap_or(len - 1);
                            let start = if from < 0 {
                                (len + from) as usize
                            } else {
                                from.min(len - 1) as usize
                            };
                            for i in (0..=start).rev() {
                                if strict_equality(&elems[i], &search) {
                                    return Completion::Normal(JsValue::Number(i as f64));
                                }
                            }
                        }
                    }
                }
                Completion::Normal(JsValue::Number(-1.0))
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("lastIndexOf".to_string(), lastindexof_fn);

        // Array.prototype.includes
        let includes_fn = self.create_function(JsFunction::Native(
            "includes".to_string(),
            Rc::new(|interp, this_val, args| {
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let obj_ref = obj.borrow();
                        if let Some(ref elems) = obj_ref.array_elements {
                            let search = args.first().cloned().unwrap_or(JsValue::Undefined);
                            for elem in elems {
                                if strict_equality(elem, &search)
                                    || (elem.is_nan() && search.is_nan())
                                {
                                    return Completion::Normal(JsValue::Boolean(true));
                                }
                            }
                        }
                    }
                }
                Completion::Normal(JsValue::Boolean(false))
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("includes".to_string(), includes_fn);

        // Array.prototype.join
        let join_fn = self.create_function(JsFunction::Native(
            "join".to_string(),
            Rc::new(|interp, this_val, args| {
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let obj_ref = obj.borrow();
                        if let Some(ref elems) = obj_ref.array_elements {
                            let sep = if let Some(s) = args.first() {
                                if matches!(s, JsValue::Undefined) {
                                    ",".to_string()
                                } else {
                                    to_js_string(s)
                                }
                            } else {
                                ",".to_string()
                            };
                            let parts: Vec<String> = elems
                                .iter()
                                .map(|v| {
                                    if v.is_undefined() || v.is_null() {
                                        String::new()
                                    } else {
                                        to_js_string(v)
                                    }
                                })
                                .collect();
                            return Completion::Normal(JsValue::String(JsString::from_str(
                                &parts.join(&sep),
                            )));
                        }
                    }
                }
                Completion::Normal(JsValue::String(JsString::from_str("")))
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("join".to_string(), join_fn);

        // Array.prototype.toString
        let tostring_fn = self.create_function(JsFunction::Native(
            "toString".to_string(),
            Rc::new(|interp, this_val, _args| {
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let obj_ref = obj.borrow();
                        if let Some(ref elems) = obj_ref.array_elements {
                            let parts: Vec<String> = elems
                                .iter()
                                .map(|v| {
                                    if v.is_undefined() || v.is_null() {
                                        String::new()
                                    } else {
                                        to_js_string(v)
                                    }
                                })
                                .collect();
                            return Completion::Normal(JsValue::String(JsString::from_str(
                                &parts.join(","),
                            )));
                        }
                    }
                }
                Completion::Normal(JsValue::String(JsString::from_str("[object Object]")))
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("toString".to_string(), tostring_fn);

        // Array.prototype.concat
        let concat_fn = self.create_function(JsFunction::Native(
            "concat".to_string(),
            Rc::new(|interp, this_val, args| {
                let mut result = Vec::new();
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        if let Some(ref elems) = obj.borrow().array_elements {
                            result.extend(elems.clone());
                        }
                    }
                }
                for arg in args {
                    if let JsValue::Object(o) = arg {
                        if let Some(obj) = interp.get_object(o.id) {
                            if let Some(ref elems) = obj.borrow().array_elements {
                                result.extend(elems.clone());
                                continue;
                            }
                        }
                    }
                    result.push(arg.clone());
                }
                let arr = interp.create_array(result);
                Completion::Normal(arr)
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("concat".to_string(), concat_fn);

        // Array.prototype.slice
        let slice_fn = self.create_function(JsFunction::Native(
            "slice".to_string(),
            Rc::new(|interp, this_val, args| {
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let obj_ref = obj.borrow();
                        if let Some(ref elems) = obj_ref.array_elements {
                            let len = elems.len() as i64;
                            let start = args
                                .first()
                                .map(|v| {
                                    let n = to_number(v) as i64;
                                    if n < 0 {
                                        (len + n).max(0) as usize
                                    } else {
                                        n.min(len) as usize
                                    }
                                })
                                .unwrap_or(0);
                            let end = args
                                .get(1)
                                .map(|v| {
                                    if matches!(v, JsValue::Undefined) {
                                        len as usize
                                    } else {
                                        let n = to_number(v) as i64;
                                        if n < 0 {
                                            (len + n).max(0) as usize
                                        } else {
                                            n.min(len) as usize
                                        }
                                    }
                                })
                                .unwrap_or(len as usize);
                            let sliced: Vec<JsValue> = if start < end {
                                elems[start..end].to_vec()
                            } else {
                                Vec::new()
                            };
                            drop(obj_ref);
                            let arr = interp.create_array(sliced);
                            return Completion::Normal(arr);
                        }
                    }
                }
                let arr = interp.create_array(Vec::new());
                Completion::Normal(arr)
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("slice".to_string(), slice_fn);

        // Array.prototype.reverse
        let reverse_fn = self.create_function(JsFunction::Native(
            "reverse".to_string(),
            Rc::new(|interp, this_val, _args| {
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let mut obj_mut = obj.borrow_mut();
                        if let Some(ref mut elems) = obj_mut.array_elements {
                            elems.reverse();
                        }
                    }
                }
                Completion::Normal(this_val.clone())
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("reverse".to_string(), reverse_fn);

        // Array.prototype.forEach
        let foreach_fn = self.create_function(JsFunction::Native(
            "forEach".to_string(),
            Rc::new(|interp, this_val, args| {
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                        for (i, elem) in elems.iter().enumerate() {
                            let call_args =
                                vec![elem.clone(), JsValue::Number(i as f64), this_val.clone()];
                            if let result @ Completion::Throw(_) =
                                interp.call_function(&callback, &JsValue::Undefined, &call_args)
                            {
                                return result;
                            }
                        }
                    }
                }
                Completion::Normal(JsValue::Undefined)
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("forEach".to_string(), foreach_fn);

        // Array.prototype.map
        let map_fn = self.create_function(JsFunction::Native(
            "map".to_string(),
            Rc::new(|interp, this_val, args| {
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                let mut result = Vec::new();
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                        for (i, elem) in elems.iter().enumerate() {
                            let call_args =
                                vec![elem.clone(), JsValue::Number(i as f64), this_val.clone()];
                            match interp.call_function(&callback, &JsValue::Undefined, &call_args) {
                                Completion::Normal(v) => result.push(v),
                                other => return other,
                            }
                        }
                    }
                }
                let arr = interp.create_array(result);
                Completion::Normal(arr)
            }),
        ));
        proto.borrow_mut().insert_builtin("map".to_string(), map_fn);

        // Array.prototype.filter
        let filter_fn = self.create_function(JsFunction::Native(
            "filter".to_string(),
            Rc::new(|interp, this_val, args| {
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                let mut result = Vec::new();
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                        for (i, elem) in elems.iter().enumerate() {
                            let call_args =
                                vec![elem.clone(), JsValue::Number(i as f64), this_val.clone()];
                            match interp.call_function(&callback, &JsValue::Undefined, &call_args) {
                                Completion::Normal(v) => {
                                    if to_boolean(&v) {
                                        result.push(elem.clone());
                                    }
                                }
                                other => return other,
                            }
                        }
                    }
                }
                let arr = interp.create_array(result);
                Completion::Normal(arr)
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("filter".to_string(), filter_fn);

        // Array.prototype.reduce
        let reduce_fn = self.create_function(JsFunction::Native(
            "reduce".to_string(),
            Rc::new(|interp, this_val, args| {
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                        let (mut acc, start) = if args.len() > 1 {
                            (args[1].clone(), 0)
                        } else if !elems.is_empty() {
                            (elems[0].clone(), 1)
                        } else {
                            let err = interp
                                .create_type_error("Reduce of empty array with no initial value");
                            return Completion::Throw(err);
                        };
                        for i in start..elems.len() {
                            let call_args = vec![
                                acc,
                                elems[i].clone(),
                                JsValue::Number(i as f64),
                                this_val.clone(),
                            ];
                            match interp.call_function(&callback, &JsValue::Undefined, &call_args) {
                                Completion::Normal(v) => acc = v,
                                other => return other,
                            }
                        }
                        return Completion::Normal(acc);
                    }
                }
                Completion::Normal(JsValue::Undefined)
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("reduce".to_string(), reduce_fn);

        // Array.prototype.some
        let some_fn = self.create_function(JsFunction::Native(
            "some".to_string(),
            Rc::new(|interp, this_val, args| {
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                        for (i, elem) in elems.iter().enumerate() {
                            let call_args =
                                vec![elem.clone(), JsValue::Number(i as f64), this_val.clone()];
                            match interp.call_function(&callback, &JsValue::Undefined, &call_args) {
                                Completion::Normal(v) => {
                                    if to_boolean(&v) {
                                        return Completion::Normal(JsValue::Boolean(true));
                                    }
                                }
                                other => return other,
                            }
                        }
                    }
                }
                Completion::Normal(JsValue::Boolean(false))
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("some".to_string(), some_fn);

        // Array.prototype.every
        let every_fn = self.create_function(JsFunction::Native(
            "every".to_string(),
            Rc::new(|interp, this_val, args| {
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                        for (i, elem) in elems.iter().enumerate() {
                            let call_args =
                                vec![elem.clone(), JsValue::Number(i as f64), this_val.clone()];
                            match interp.call_function(&callback, &JsValue::Undefined, &call_args) {
                                Completion::Normal(v) => {
                                    if !to_boolean(&v) {
                                        return Completion::Normal(JsValue::Boolean(false));
                                    }
                                }
                                other => return other,
                            }
                        }
                    }
                }
                Completion::Normal(JsValue::Boolean(true))
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("every".to_string(), every_fn);

        // Array.prototype.find
        let find_fn = self.create_function(JsFunction::Native(
            "find".to_string(),
            Rc::new(|interp, this_val, args| {
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                        for (i, elem) in elems.iter().enumerate() {
                            let call_args =
                                vec![elem.clone(), JsValue::Number(i as f64), this_val.clone()];
                            match interp.call_function(&callback, &JsValue::Undefined, &call_args) {
                                Completion::Normal(v) => {
                                    if to_boolean(&v) {
                                        return Completion::Normal(elem.clone());
                                    }
                                }
                                other => return other,
                            }
                        }
                    }
                }
                Completion::Normal(JsValue::Undefined)
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("find".to_string(), find_fn);

        // Array.prototype.findIndex
        let findindex_fn = self.create_function(JsFunction::Native(
            "findIndex".to_string(),
            Rc::new(|interp, this_val, args| {
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                        for (i, elem) in elems.iter().enumerate() {
                            let call_args =
                                vec![elem.clone(), JsValue::Number(i as f64), this_val.clone()];
                            match interp.call_function(&callback, &JsValue::Undefined, &call_args) {
                                Completion::Normal(v) => {
                                    if to_boolean(&v) {
                                        return Completion::Normal(JsValue::Number(i as f64));
                                    }
                                }
                                other => return other,
                            }
                        }
                    }
                }
                Completion::Normal(JsValue::Number(-1.0))
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("findIndex".to_string(), findindex_fn);

        // Array.prototype.splice
        let splice_fn = self.create_function(JsFunction::Native(
            "splice".to_string(),
            Rc::new(|interp, this_val, args| {
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let mut obj_mut = obj.borrow_mut();
                        if let Some(ref mut elems) = obj_mut.array_elements {
                            let len = elems.len() as i64;
                            let start = args
                                .first()
                                .map(|v| {
                                    let n = to_number(v) as i64;
                                    if n < 0 {
                                        (len + n).max(0) as usize
                                    } else {
                                        n.min(len) as usize
                                    }
                                })
                                .unwrap_or(0);
                            let delete_count = args
                                .get(1)
                                .map(|v| {
                                    (to_number(v) as i64).max(0).min(len - start as i64) as usize
                                })
                                .unwrap_or((len - start as i64) as usize);
                            let removed: Vec<JsValue> =
                                elems.drain(start..start + delete_count).collect();
                            let items: Vec<JsValue> = args.iter().skip(2).cloned().collect();
                            for (i, item) in items.into_iter().enumerate() {
                                elems.insert(start + i, item);
                            }
                            let new_len = elems.len() as f64;
                            obj_mut.insert_value("length".to_string(), JsValue::Number(new_len));
                            drop(obj_mut);
                            let arr = interp.create_array(removed);
                            return Completion::Normal(arr);
                        }
                    }
                }
                let arr = interp.create_array(Vec::new());
                Completion::Normal(arr)
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("splice".to_string(), splice_fn);

        // Array.prototype.fill
        let fill_fn = self.create_function(JsFunction::Native(
            "fill".to_string(),
            Rc::new(|interp, this_val, args| {
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let mut obj_mut = obj.borrow_mut();
                        if let Some(ref mut elems) = obj_mut.array_elements {
                            let value = args.first().cloned().unwrap_or(JsValue::Undefined);
                            let len = elems.len() as i64;
                            let start = args
                                .get(1)
                                .map(|v| {
                                    let n = to_number(v) as i64;
                                    if n < 0 {
                                        (len + n).max(0) as usize
                                    } else {
                                        n.min(len) as usize
                                    }
                                })
                                .unwrap_or(0);
                            let end = args
                                .get(2)
                                .map(|v| {
                                    if matches!(v, JsValue::Undefined) {
                                        len as usize
                                    } else {
                                        let n = to_number(v) as i64;
                                        if n < 0 {
                                            (len + n).max(0) as usize
                                        } else {
                                            n.min(len) as usize
                                        }
                                    }
                                })
                                .unwrap_or(len as usize);
                            for i in start..end {
                                elems[i] = value.clone();
                            }
                        }
                    }
                }
                Completion::Normal(this_val.clone())
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("fill".to_string(), fill_fn);

        // Array.isArray
        let is_array_fn = self.create_function(JsFunction::Native(
            "isArray".to_string(),
            Rc::new(|interp, _this, args| {
                let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = &val {
                    if let Some(obj) = interp.get_object(o.id) {
                        return Completion::Normal(JsValue::Boolean(
                            obj.borrow().array_elements.is_some(),
                        ));
                    }
                }
                Completion::Normal(JsValue::Boolean(false))
            }),
        ));

        // Array.from
        let array_from = self.create_function(JsFunction::Native(
            "from".to_string(),
            Rc::new(|interp, _this, args: &[JsValue]| {
                let source = args.first().cloned().unwrap_or(JsValue::Undefined);
                let map_fn = args.get(1).cloned();
                let mut values = Vec::new();
                match &source {
                    JsValue::String(s) => {
                        for ch in s.to_rust_string().chars() {
                            let v = JsValue::String(JsString::from_str(&ch.to_string()));
                            if let Some(ref mf) = map_fn {
                                match interp.call_function(
                                    mf,
                                    &JsValue::Undefined,
                                    &[v, JsValue::Number(values.len() as f64)],
                                ) {
                                    Completion::Normal(mapped) => values.push(mapped),
                                    other => return other,
                                }
                            } else {
                                values.push(v);
                            }
                        }
                    }
                    JsValue::Object(o) => {
                        if let Some(obj) = interp.get_object(o.id) {
                            let len = if let Some(JsValue::Number(n)) =
                                obj.borrow().get_property_value("length")
                            {
                                n as usize
                            } else {
                                0
                            };
                            for i in 0..len {
                                let v = obj.borrow().get_property(&i.to_string());
                                if let Some(ref mf) = map_fn {
                                    match interp.call_function(
                                        mf,
                                        &JsValue::Undefined,
                                        &[v, JsValue::Number(i as f64)],
                                    ) {
                                        Completion::Normal(mapped) => values.push(mapped),
                                        other => return other,
                                    }
                                } else {
                                    values.push(v);
                                }
                            }
                        }
                    }
                    _ => {}
                }
                Completion::Normal(interp.create_array(values))
            }),
        ));
        // Array.of
        let array_of = self.create_function(JsFunction::Native(
            "of".to_string(),
            Rc::new(|interp, _this, args: &[JsValue]| {
                Completion::Normal(interp.create_array(args.to_vec()))
            }),
        ));

        // Array.prototype.reduceRight
        let reduce_right_fn = self.create_function(JsFunction::Native(
            "reduceRight".to_string(),
            Rc::new(|interp, this_val, args| {
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                        let len = elems.len();
                        let (mut acc, start) =
                            if args.len() > 1 {
                                (args[1].clone(), len)
                            } else if len > 0 {
                                (elems[len - 1].clone(), len - 1)
                            } else {
                                return Completion::Throw(interp.create_type_error(
                                    "Reduce of empty array with no initial value",
                                ));
                            };
                        for i in (0..start).rev() {
                            let result = interp.call_function(
                                &callback,
                                &JsValue::Undefined,
                                &[
                                    acc,
                                    elems[i].clone(),
                                    JsValue::Number(i as f64),
                                    this_val.clone(),
                                ],
                            );
                            match result {
                                Completion::Normal(v) => acc = v,
                                other => return other,
                            }
                        }
                        return Completion::Normal(acc);
                    }
                }
                Completion::Normal(JsValue::Undefined)
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("reduceRight".to_string(), reduce_right_fn);

        // Array.prototype.at
        let at_fn = self.create_function(JsFunction::Native(
            "at".to_string(),
            Rc::new(|interp, this_val, args| {
                let idx = args.first().map(|v| to_number(v) as i64).unwrap_or(0);
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        if let Some(elems) = &obj.borrow().array_elements {
                            let len = elems.len() as i64;
                            let actual = if idx < 0 { len + idx } else { idx };
                            if actual >= 0 && actual < len {
                                return Completion::Normal(elems[actual as usize].clone());
                            }
                        }
                    }
                }
                Completion::Normal(JsValue::Undefined)
            }),
        ));
        proto.borrow_mut().insert_builtin("at".to_string(), at_fn);

        // Array.prototype.sort
        let sort_fn = self.create_function(JsFunction::Native(
            "sort".to_string(),
            Rc::new(|interp, this_val, args| {
                let compare_fn = args.first().cloned();
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        if let Some(ref mut elems) = obj.borrow_mut().array_elements {
                            let mut pairs: Vec<(usize, JsValue)> =
                                elems.drain(..).enumerate().collect();
                            pairs.sort_by(|a, b| {
                                let x = &a.1;
                                let y = &b.1;
                                if matches!(x, JsValue::Undefined)
                                    && matches!(y, JsValue::Undefined)
                                {
                                    return std::cmp::Ordering::Equal;
                                }
                                if matches!(x, JsValue::Undefined) {
                                    return std::cmp::Ordering::Greater;
                                }
                                if matches!(y, JsValue::Undefined) {
                                    return std::cmp::Ordering::Less;
                                }
                                if let Some(JsValue::Object(fo)) = &compare_fn {
                                    if let Some(fobj) = interp.get_object(fo.id) {
                                        if fobj.borrow().callable.is_some() {
                                            let result = interp.call_function(
                                                compare_fn.as_ref().unwrap(),
                                                &JsValue::Undefined,
                                                &[x.clone(), y.clone()],
                                            );
                                            if let Completion::Normal(v) = result {
                                                let n = to_number(&v);
                                                if n < 0.0 {
                                                    return std::cmp::Ordering::Less;
                                                }
                                                if n > 0.0 {
                                                    return std::cmp::Ordering::Greater;
                                                }
                                                return std::cmp::Ordering::Equal;
                                            }
                                        }
                                    }
                                }
                                let xs = to_js_string(x);
                                let ys = to_js_string(y);
                                xs.cmp(&ys)
                            });
                            *elems = pairs.into_iter().map(|(_, v)| v).collect();
                        }
                    }
                }
                Completion::Normal(this_val.clone())
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("sort".to_string(), sort_fn);

        // Array.prototype.flat
        let flat_fn = self.create_function(JsFunction::Native(
            "flat".to_string(),
            Rc::new(|interp, this_val, args| {
                let depth = args.first().map(|v| to_number(v) as i64).unwrap_or(1);
                fn flatten(
                    interp: &Interpreter,
                    val: &JsValue,
                    depth: i64,
                    result: &mut Vec<JsValue>,
                ) {
                    if let JsValue::Object(o) = val {
                        if let Some(obj) = interp.get_object(o.id) {
                            if let Some(elems) = &obj.borrow().array_elements {
                                for elem in elems {
                                    if depth > 0 {
                                        if let JsValue::Object(eo) = elem {
                                            if let Some(eobj) = interp.get_object(eo.id) {
                                                if eobj.borrow().array_elements.is_some() {
                                                    flatten(interp, elem, depth - 1, result);
                                                    continue;
                                                }
                                            }
                                        }
                                    }
                                    result.push(elem.clone());
                                }
                                return;
                            }
                        }
                    }
                    result.push(val.clone());
                }
                let mut result = Vec::new();
                flatten(interp, this_val, depth, &mut result);
                Completion::Normal(interp.create_array(result))
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("flat".to_string(), flat_fn);

        // Array.prototype.flatMap
        let flatmap_fn = self.create_function(JsFunction::Native(
            "flatMap".to_string(),
            Rc::new(|interp, this_val, args| {
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let mut result = Vec::new();
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                        for (i, elem) in elems.iter().enumerate() {
                            let mapped = interp.call_function(
                                &callback,
                                &this_arg,
                                &[elem.clone(), JsValue::Number(i as f64), this_val.clone()],
                            );
                            if let Completion::Normal(v) = mapped {
                                if let JsValue::Object(mo) = &v {
                                    if let Some(mobj) = interp.get_object(mo.id) {
                                        if let Some(melems) = &mobj.borrow().array_elements {
                                            result.extend(melems.iter().cloned());
                                            continue;
                                        }
                                    }
                                }
                                result.push(v);
                            } else {
                                return mapped;
                            }
                        }
                    }
                }
                Completion::Normal(interp.create_array(result))
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("flatMap".to_string(), flatmap_fn);

        // Array.prototype.findLast / findLastIndex
        let findlast_fn = self.create_function(JsFunction::Native(
            "findLast".to_string(),
            Rc::new(|interp, this_val, args| {
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                        for i in (0..elems.len()).rev() {
                            let result = interp.call_function(
                                &callback,
                                &this_arg,
                                &[
                                    elems[i].clone(),
                                    JsValue::Number(i as f64),
                                    this_val.clone(),
                                ],
                            );
                            if let Completion::Normal(v) = result {
                                if to_boolean(&v) {
                                    return Completion::Normal(elems[i].clone());
                                }
                            } else {
                                return result;
                            }
                        }
                    }
                }
                Completion::Normal(JsValue::Undefined)
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("findLast".to_string(), findlast_fn);

        let findlastidx_fn = self.create_function(JsFunction::Native(
            "findLastIndex".to_string(),
            Rc::new(|interp, this_val, args| {
                let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
                let this_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                        for i in (0..elems.len()).rev() {
                            let result = interp.call_function(
                                &callback,
                                &this_arg,
                                &[
                                    elems[i].clone(),
                                    JsValue::Number(i as f64),
                                    this_val.clone(),
                                ],
                            );
                            if let Completion::Normal(v) = result {
                                if to_boolean(&v) {
                                    return Completion::Normal(JsValue::Number(i as f64));
                                }
                            } else {
                                return result;
                            }
                        }
                    }
                }
                Completion::Normal(JsValue::Number(-1.0))
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("findLastIndex".to_string(), findlastidx_fn);

        // Array.prototype.copyWithin
        let copywithin_fn = self.create_function(JsFunction::Native(
            "copyWithin".to_string(),
            Rc::new(|interp, this_val, args| {
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let mut obj_mut = obj.borrow_mut();
                        if let Some(ref mut elems) = obj_mut.array_elements {
                            let len = elems.len() as i64;
                            let target = {
                                let t = args.first().map(|v| to_number(v) as i64).unwrap_or(0);
                                if t < 0 {
                                    (len + t).max(0) as usize
                                } else {
                                    t.min(len) as usize
                                }
                            };
                            let start = {
                                let s = args.get(1).map(|v| to_number(v) as i64).unwrap_or(0);
                                if s < 0 {
                                    (len + s).max(0) as usize
                                } else {
                                    s.min(len) as usize
                                }
                            };
                            let end = {
                                let e = args
                                    .get(2)
                                    .map(|v| {
                                        if matches!(v, JsValue::Undefined) {
                                            len
                                        } else {
                                            to_number(v) as i64
                                        }
                                    })
                                    .unwrap_or(len);
                                if e < 0 {
                                    (len + e).max(0) as usize
                                } else {
                                    e.min(len) as usize
                                }
                            };
                            let count = (end - start).min(len as usize - target);
                            let src: Vec<JsValue> = elems[start..start + count].to_vec();
                            for (i, v) in src.into_iter().enumerate() {
                                elems[target + i] = v;
                            }
                        }
                    }
                }
                Completion::Normal(this_val.clone())
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("copyWithin".to_string(), copywithin_fn);

        // Array.prototype.entries
        let entries_fn = self.create_function(JsFunction::Native(
            "entries".to_string(),
            Rc::new(|interp, this_val, _args| {
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                        let pairs: Vec<JsValue> = elems
                            .into_iter()
                            .enumerate()
                            .map(|(i, v)| interp.create_array(vec![JsValue::Number(i as f64), v]))
                            .collect();
                        let iter_arr = interp.create_array(pairs);
                        return Completion::Normal(iter_arr);
                    }
                }
                Completion::Normal(interp.create_array(Vec::new()))
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("entries".to_string(), entries_fn);

        // Array.prototype.keys
        let keys_fn = self.create_function(JsFunction::Native(
            "keys".to_string(),
            Rc::new(|interp, this_val, _args| {
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let len = obj
                            .borrow()
                            .array_elements
                            .as_ref()
                            .map(|e| e.len())
                            .unwrap_or(0);
                        let keys: Vec<JsValue> =
                            (0..len).map(|i| JsValue::Number(i as f64)).collect();
                        let arr = interp.create_array(keys);
                        return Completion::Normal(arr);
                    }
                }
                Completion::Normal(interp.create_array(Vec::new()))
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("keys".to_string(), keys_fn);

        // Array.prototype.values
        let values_fn = self.create_function(JsFunction::Native(
            "values".to_string(),
            Rc::new(|interp, this_val, _args| {
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        let elems = obj.borrow().array_elements.clone().unwrap_or_default();
                        let arr = interp.create_array(elems);
                        return Completion::Normal(arr);
                    }
                }
                Completion::Normal(interp.create_array(Vec::new()))
            }),
        ));
        proto
            .borrow_mut()
            .insert_builtin("values".to_string(), values_fn);

        // Set Array statics on the Array constructor
        if let Some(array_val) = self.global_env.borrow().get("Array") {
            if let JsValue::Object(o) = &array_val {
                if let Some(obj) = self.get_object(o.id) {
                    obj.borrow_mut()
                        .insert_value("isArray".to_string(), is_array_fn);
                    obj.borrow_mut()
                        .insert_value("from".to_string(), array_from);
                    obj.borrow_mut().insert_value("of".to_string(), array_of);
                }
            }
        }

        self.array_prototype = Some(proto);
    }

    fn create_array(&mut self, values: Vec<JsValue>) -> JsValue {
        let mut obj_data = JsObjectData::new();
        obj_data.prototype = self
            .array_prototype
            .clone()
            .or(self.object_prototype.clone());
        obj_data.class_name = "Array".to_string();
        for (i, v) in values.iter().enumerate() {
            obj_data.insert_value(i.to_string(), v.clone());
        }
        obj_data.insert_value("length".to_string(), JsValue::Number(values.len() as f64));
        obj_data.array_elements = Some(values);
        let obj = Rc::new(RefCell::new(obj_data));
        self.objects.push(obj);
        JsValue::Object(crate::types::JsObject {
            id: self.objects.len() as u64 - 1,
        })
    }

    fn setup_string_prototype(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "String".to_string();

        let methods: Vec<(
            &str,
            Rc<dyn Fn(&mut Interpreter, &JsValue, &[JsValue]) -> Completion>,
        )> = vec![
            (
                "charAt",
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let idx = args.first().map(|v| to_number(v) as usize).unwrap_or(0);
                    let ch = s
                        .chars()
                        .nth(idx)
                        .map(|c| c.to_string())
                        .unwrap_or_default();
                    Completion::Normal(JsValue::String(JsString::from_str(&ch)))
                }),
            ),
            (
                "charCodeAt",
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let idx = args.first().map(|v| to_number(v) as usize).unwrap_or(0);
                    let code = s
                        .encode_utf16()
                        .nth(idx)
                        .map(|c| c as f64)
                        .unwrap_or(f64::NAN);
                    Completion::Normal(JsValue::Number(code))
                }),
            ),
            (
                "codePointAt",
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let idx = args.first().map(|v| to_number(v) as usize).unwrap_or(0);
                    // Use UTF-16 code units for indexing
                    let utf16: Vec<u16> = s.encode_utf16().collect();
                    if idx >= utf16.len() {
                        return Completion::Normal(JsValue::Undefined);
                    }
                    let code = utf16[idx];
                    if (0xD800..=0xDBFF).contains(&code) && idx + 1 < utf16.len() {
                        let trail = utf16[idx + 1];
                        if (0xDC00..=0xDFFF).contains(&trail) {
                            let cp =
                                ((code as u32 - 0xD800) << 10) + (trail as u32 - 0xDC00) + 0x10000;
                            return Completion::Normal(JsValue::Number(cp as f64));
                        }
                    }
                    Completion::Normal(JsValue::Number(code as f64))
                }),
            ),
            (
                "indexOf",
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let search = args.first().map(to_js_string).unwrap_or_default();
                    let from = args.get(1).map(|v| to_number(v) as usize).unwrap_or(0);
                    let result = s[from..]
                        .find(&search)
                        .map(|i| (i + from) as f64)
                        .unwrap_or(-1.0);
                    Completion::Normal(JsValue::Number(result))
                }),
            ),
            (
                "lastIndexOf",
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let search = args.first().map(to_js_string).unwrap_or_default();
                    let result = s.rfind(&search).map(|i| i as f64).unwrap_or(-1.0);
                    Completion::Normal(JsValue::Number(result))
                }),
            ),
            (
                "includes",
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let search = args.first().map(to_js_string).unwrap_or_default();
                    Completion::Normal(JsValue::Boolean(s.contains(&search)))
                }),
            ),
            (
                "startsWith",
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let search = args.first().map(to_js_string).unwrap_or_default();
                    Completion::Normal(JsValue::Boolean(s.starts_with(&search)))
                }),
            ),
            (
                "endsWith",
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let search = args.first().map(to_js_string).unwrap_or_default();
                    Completion::Normal(JsValue::Boolean(s.ends_with(&search)))
                }),
            ),
            (
                "slice",
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let len = s.len() as i64;
                    let start = args
                        .first()
                        .map(|v| {
                            let n = to_number(v) as i64;
                            if n < 0 {
                                (len + n).max(0) as usize
                            } else {
                                n.min(len) as usize
                            }
                        })
                        .unwrap_or(0);
                    let end = args
                        .get(1)
                        .map(|v| {
                            if matches!(v, JsValue::Undefined) {
                                len as usize
                            } else {
                                let n = to_number(v) as i64;
                                if n < 0 {
                                    (len + n).max(0) as usize
                                } else {
                                    n.min(len) as usize
                                }
                            }
                        })
                        .unwrap_or(len as usize);
                    let result = if start < end { &s[start..end] } else { "" };
                    Completion::Normal(JsValue::String(JsString::from_str(result)))
                }),
            ),
            (
                "substring",
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let len = s.len();
                    let mut start = args
                        .first()
                        .map(|v| (to_number(v) as usize).min(len))
                        .unwrap_or(0);
                    let mut end = args
                        .get(1)
                        .map(|v| {
                            if matches!(v, JsValue::Undefined) {
                                len
                            } else {
                                (to_number(v) as usize).min(len)
                            }
                        })
                        .unwrap_or(len);
                    if start > end {
                        std::mem::swap(&mut start, &mut end);
                    }
                    Completion::Normal(JsValue::String(JsString::from_str(&s[start..end])))
                }),
            ),
            (
                "toLowerCase",
                Rc::new(|_interp, this_val, _args| {
                    Completion::Normal(JsValue::String(JsString::from_str(
                        &to_js_string(this_val).to_lowercase(),
                    )))
                }),
            ),
            (
                "toUpperCase",
                Rc::new(|_interp, this_val, _args| {
                    Completion::Normal(JsValue::String(JsString::from_str(
                        &to_js_string(this_val).to_uppercase(),
                    )))
                }),
            ),
            (
                "trim",
                Rc::new(|_interp, this_val, _args| {
                    Completion::Normal(JsValue::String(JsString::from_str(
                        to_js_string(this_val).trim(),
                    )))
                }),
            ),
            (
                "trimStart",
                Rc::new(|_interp, this_val, _args| {
                    Completion::Normal(JsValue::String(JsString::from_str(
                        to_js_string(this_val).trim_start(),
                    )))
                }),
            ),
            (
                "trimEnd",
                Rc::new(|_interp, this_val, _args| {
                    Completion::Normal(JsValue::String(JsString::from_str(
                        to_js_string(this_val).trim_end(),
                    )))
                }),
            ),
            (
                "repeat",
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let count = args.first().map(|v| to_number(v) as usize).unwrap_or(0);
                    Completion::Normal(JsValue::String(JsString::from_str(&s.repeat(count))))
                }),
            ),
            (
                "padStart",
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let target_len = args.first().map(|v| to_number(v) as usize).unwrap_or(0);
                    let fill = args
                        .get(1)
                        .map(to_js_string)
                        .unwrap_or_else(|| " ".to_string());
                    if s.len() >= target_len || fill.is_empty() {
                        return Completion::Normal(JsValue::String(JsString::from_str(&s)));
                    }
                    let pad_len = target_len - s.len();
                    let pad: String = fill.chars().cycle().take(pad_len).collect();
                    Completion::Normal(JsValue::String(JsString::from_str(&format!("{pad}{s}"))))
                }),
            ),
            (
                "padEnd",
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let target_len = args.first().map(|v| to_number(v) as usize).unwrap_or(0);
                    let fill = args
                        .get(1)
                        .map(to_js_string)
                        .unwrap_or_else(|| " ".to_string());
                    if s.len() >= target_len || fill.is_empty() {
                        return Completion::Normal(JsValue::String(JsString::from_str(&s)));
                    }
                    let pad_len = target_len - s.len();
                    let pad: String = fill.chars().cycle().take(pad_len).collect();
                    Completion::Normal(JsValue::String(JsString::from_str(&format!("{s}{pad}"))))
                }),
            ),
            (
                "concat",
                Rc::new(|_interp, this_val, args| {
                    let mut s = to_js_string(this_val);
                    for arg in args {
                        s.push_str(&to_js_string(arg));
                    }
                    Completion::Normal(JsValue::String(JsString::from_str(&s)))
                }),
            ),
            (
                "toString",
                Rc::new(|_interp, this_val, _args| {
                    Completion::Normal(JsValue::String(JsString::from_str(&to_js_string(this_val))))
                }),
            ),
            (
                "valueOf",
                Rc::new(|_interp, this_val, _args| {
                    Completion::Normal(JsValue::String(JsString::from_str(&to_js_string(this_val))))
                }),
            ),
            (
                "split",
                Rc::new(|interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let separator = args.first();
                    let parts: Vec<JsValue> = if let Some(sep) = separator {
                        if matches!(sep, JsValue::Undefined) {
                            vec![JsValue::String(JsString::from_str(&s))]
                        } else {
                            let sep_str = to_js_string(sep);
                            if sep_str.is_empty() {
                                s.chars()
                                    .map(|c| JsValue::String(JsString::from_str(&c.to_string())))
                                    .collect()
                            } else {
                                s.split(&sep_str)
                                    .map(|p| JsValue::String(JsString::from_str(p)))
                                    .collect()
                            }
                        }
                    } else {
                        vec![JsValue::String(JsString::from_str(&s))]
                    };
                    let arr = interp.create_array(parts);
                    Completion::Normal(arr)
                }),
            ),
            (
                "replace",
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let search = args.first().map(to_js_string).unwrap_or_default();
                    let replacement = args.get(1).map(to_js_string).unwrap_or_default();
                    let result = s.replacen(&search, &replacement, 1);
                    Completion::Normal(JsValue::String(JsString::from_str(&result)))
                }),
            ),
            (
                "replaceAll",
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let search = args.first().map(to_js_string).unwrap_or_default();
                    let replacement = args.get(1).map(to_js_string).unwrap_or_default();
                    Completion::Normal(JsValue::String(JsString::from_str(
                        &s.replace(&search, &replacement),
                    )))
                }),
            ),
            (
                "at",
                Rc::new(|_interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let len = s.len() as i64;
                    let idx = args.first().map(|v| to_number(v) as i64).unwrap_or(0);
                    let actual = if idx < 0 { len + idx } else { idx };
                    if actual < 0 || actual >= len {
                        return Completion::Normal(JsValue::Undefined);
                    }
                    let ch = s
                        .chars()
                        .nth(actual as usize)
                        .map(|c| c.to_string())
                        .unwrap_or_default();
                    Completion::Normal(JsValue::String(JsString::from_str(&ch)))
                }),
            ),
            (
                "search",
                Rc::new(|interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let (source, flags) = match args.first() {
                        Some(JsValue::Object(o)) => {
                            let obj = interp.get_object(o.id);
                            let src = obj
                                .as_ref()
                                .map(|ob| {
                                    if let JsValue::String(sv) = ob.borrow().get_property("source")
                                    {
                                        sv.to_rust_string()
                                    } else {
                                        String::new()
                                    }
                                })
                                .unwrap_or_default();
                            let fl = obj
                                .as_ref()
                                .map(|ob| {
                                    if let JsValue::String(sv) = ob.borrow().get_property("flags") {
                                        sv.to_rust_string()
                                    } else {
                                        String::new()
                                    }
                                })
                                .unwrap_or_default();
                            (src, fl)
                        }
                        Some(v) => (to_js_string(v), String::new()),
                        None => (String::new(), String::new()),
                    };
                    let pat = if flags.contains('i') {
                        format!("(?i){}", source)
                    } else {
                        source
                    };
                    if let Ok(re) = regex::Regex::new(&pat) {
                        if let Some(m) = re.find(&s) {
                            return Completion::Normal(JsValue::Number(m.start() as f64));
                        }
                    }
                    Completion::Normal(JsValue::Number(-1.0))
                }),
            ),
            (
                "match",
                Rc::new(|interp, this_val, args| {
                    let s = to_js_string(this_val);
                    let (source, flags) = match args.first() {
                        Some(JsValue::Object(o)) => {
                            let obj = interp.get_object(o.id);
                            let src = obj
                                .as_ref()
                                .map(|ob| {
                                    if let JsValue::String(sv) = ob.borrow().get_property("source")
                                    {
                                        sv.to_rust_string()
                                    } else {
                                        String::new()
                                    }
                                })
                                .unwrap_or_default();
                            let fl = obj
                                .as_ref()
                                .map(|ob| {
                                    if let JsValue::String(sv) = ob.borrow().get_property("flags") {
                                        sv.to_rust_string()
                                    } else {
                                        String::new()
                                    }
                                })
                                .unwrap_or_default();
                            (src, fl)
                        }
                        Some(v) => (to_js_string(v), String::new()),
                        None => (String::new(), String::new()),
                    };
                    let pat = if flags.contains('i') {
                        format!("(?i){}", source)
                    } else {
                        source
                    };
                    let re = match regex::Regex::new(&pat) {
                        Ok(r) => r,
                        Err(_) => return Completion::Normal(JsValue::Null),
                    };
                    if flags.contains('g') {
                        let matches: Vec<JsValue> = re
                            .find_iter(&s)
                            .map(|m| JsValue::String(JsString::from_str(m.as_str())))
                            .collect();
                        if matches.is_empty() {
                            Completion::Normal(JsValue::Null)
                        } else {
                            Completion::Normal(interp.create_array(matches))
                        }
                    } else {
                        if let Some(m) = re.find(&s) {
                            let matched = JsValue::String(JsString::from_str(m.as_str()));
                            let result = interp.create_array(vec![matched]);
                            if let JsValue::Object(ro) = &result {
                                if let Some(robj) = interp.get_object(ro.id) {
                                    robj.borrow_mut().insert_value(
                                        "index".to_string(),
                                        JsValue::Number(m.start() as f64),
                                    );
                                    robj.borrow_mut().insert_value(
                                        "input".to_string(),
                                        JsValue::String(JsString::from_str(&s)),
                                    );
                                }
                            }
                            Completion::Normal(result)
                        } else {
                            Completion::Normal(JsValue::Null)
                        }
                    }
                }),
            ),
        ];

        for (name, func) in methods {
            let fn_val = self.create_function(JsFunction::Native(name.to_string(), func));
            proto.borrow_mut().insert_builtin(name.to_string(), fn_val);
        }

        self.string_prototype = Some(proto);
    }

    fn setup_number_prototype(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "Number".to_string();
        proto.borrow_mut().primitive_value = Some(JsValue::Number(0.0));

        fn this_number_value(interp: &Interpreter, this: &JsValue) -> Option<f64> {
            match this {
                JsValue::Number(n) => Some(*n),
                JsValue::Object(o) => interp.get_object(o.id).and_then(|obj| {
                    let b = obj.borrow();
                    if b.class_name == "Number" {
                        if let Some(JsValue::Number(n)) = &b.primitive_value {
                            return Some(*n);
                        }
                    }
                    None
                }),
                _ => None,
            }
        }

        let methods: Vec<(
            &str,
            Rc<dyn Fn(&mut Interpreter, &JsValue, &[JsValue]) -> Completion>,
        )> = vec![
            (
                "toString",
                Rc::new(|interp, this, args| {
                    let Some(n) = this_number_value(interp, this) else {
                        let err =
                            interp.create_type_error("Number.prototype.toString requires a Number");
                        return Completion::Throw(err);
                    };
                    let radix = args
                        .first()
                        .map(|v| {
                            if v.is_undefined() {
                                10
                            } else {
                                to_number(v) as u32
                            }
                        })
                        .unwrap_or(10);
                    if radix < 2 || radix > 36 {
                        let err =
                            interp.create_error("RangeError", "radix must be between 2 and 36");
                        return Completion::Throw(err);
                    }
                    if radix == 10 {
                        Completion::Normal(JsValue::String(JsString::from_str(&to_js_string(
                            &JsValue::Number(n),
                        ))))
                    } else {
                        let s = format_radix(n as i64, radix);
                        Completion::Normal(JsValue::String(JsString::from_str(&s)))
                    }
                }),
            ),
            (
                "valueOf",
                Rc::new(|interp, this, _args| {
                    let Some(n) = this_number_value(interp, this) else {
                        let err =
                            interp.create_type_error("Number.prototype.valueOf requires a Number");
                        return Completion::Throw(err);
                    };
                    Completion::Normal(JsValue::Number(n))
                }),
            ),
            (
                "toFixed",
                Rc::new(|interp, this, args| {
                    let Some(n) = this_number_value(interp, this) else {
                        let err =
                            interp.create_type_error("Number.prototype.toFixed requires a Number");
                        return Completion::Throw(err);
                    };
                    let digits = args.first().map(|v| to_number(v) as usize).unwrap_or(0);
                    Completion::Normal(JsValue::String(JsString::from_str(&format!(
                        "{n:.digits$}"
                    ))))
                }),
            ),
            (
                "toExponential",
                Rc::new(|interp, this, args| {
                    let Some(n) = this_number_value(interp, this) else {
                        let err = interp
                            .create_type_error("Number.prototype.toExponential requires a Number");
                        return Completion::Throw(err);
                    };
                    let has_arg = args.first().is_some_and(|v| !v.is_undefined());
                    if has_arg {
                        let digits = to_number(args.first().unwrap()) as usize;
                        Completion::Normal(JsValue::String(JsString::from_str(&format!(
                            "{n:.digits$e}"
                        ))))
                    } else {
                        Completion::Normal(JsValue::String(JsString::from_str(&format!("{n:e}"))))
                    }
                }),
            ),
            (
                "toPrecision",
                Rc::new(|interp, this, args| {
                    let Some(n) = this_number_value(interp, this) else {
                        let err = interp
                            .create_type_error("Number.prototype.toPrecision requires a Number");
                        return Completion::Throw(err);
                    };
                    let has_arg = args.first().is_some_and(|v| !v.is_undefined());
                    if has_arg {
                        let precision = to_number(args.first().unwrap()) as usize;
                        Completion::Normal(JsValue::String(JsString::from_str(&format!(
                            "{n:.prec$}",
                            prec = precision.saturating_sub(1)
                        ))))
                    } else {
                        Completion::Normal(JsValue::String(JsString::from_str(&to_js_string(
                            &JsValue::Number(n),
                        ))))
                    }
                }),
            ),
            (
                "toLocaleString",
                Rc::new(|interp, this, _args| {
                    let Some(n) = this_number_value(interp, this) else {
                        let err = interp
                            .create_type_error("Number.prototype.toLocaleString requires a Number");
                        return Completion::Throw(err);
                    };
                    Completion::Normal(JsValue::String(JsString::from_str(&to_js_string(
                        &JsValue::Number(n),
                    ))))
                }),
            ),
        ];

        for (name, func) in methods {
            let fn_val = self.create_function(JsFunction::Native(name.to_string(), func));
            proto.borrow_mut().insert_builtin(name.to_string(), fn_val);
        }

        // Set Number.prototype on the Number constructor
        if let Some(num_val) = self.global_env.borrow().get("Number") {
            if let JsValue::Object(o) = &num_val {
                if let Some(num_obj) = self.get_object(o.id) {
                    let proto_val = JsValue::Object(crate::types::JsObject {
                        id: self
                            .objects
                            .iter()
                            .position(|o| Rc::ptr_eq(o, &proto))
                            .unwrap() as u64,
                    });
                    num_obj
                        .borrow_mut()
                        .insert_value("prototype".to_string(), proto_val);
                }
            }
        }

        self.number_prototype = Some(proto);
    }

    fn setup_boolean_prototype(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "Boolean".to_string();
        proto.borrow_mut().primitive_value = Some(JsValue::Boolean(false));

        fn this_boolean_value(interp: &Interpreter, this: &JsValue) -> Option<bool> {
            match this {
                JsValue::Boolean(b) => Some(*b),
                JsValue::Object(o) => interp.get_object(o.id).and_then(|obj| {
                    let b = obj.borrow();
                    if b.class_name == "Boolean" {
                        if let Some(JsValue::Boolean(v)) = &b.primitive_value {
                            return Some(*v);
                        }
                    }
                    None
                }),
                _ => None,
            }
        }

        let methods: Vec<(
            &str,
            Rc<dyn Fn(&mut Interpreter, &JsValue, &[JsValue]) -> Completion>,
        )> = vec![
            (
                "toString",
                Rc::new(|interp, this, _args| {
                    let Some(b) = this_boolean_value(interp, this) else {
                        let err = interp
                            .create_type_error("Boolean.prototype.toString requires a Boolean");
                        return Completion::Throw(err);
                    };
                    Completion::Normal(JsValue::String(JsString::from_str(if b {
                        "true"
                    } else {
                        "false"
                    })))
                }),
            ),
            (
                "valueOf",
                Rc::new(|interp, this, _args| {
                    let Some(b) = this_boolean_value(interp, this) else {
                        let err = interp
                            .create_type_error("Boolean.prototype.valueOf requires a Boolean");
                        return Completion::Throw(err);
                    };
                    Completion::Normal(JsValue::Boolean(b))
                }),
            ),
        ];

        for (name, func) in methods {
            let fn_val = self.create_function(JsFunction::Native(name.to_string(), func));
            proto.borrow_mut().insert_builtin(name.to_string(), fn_val);
        }

        // Set Boolean.prototype on the Boolean constructor
        if let Some(bool_val) = self.global_env.borrow().get("Boolean") {
            if let JsValue::Object(o) = &bool_val {
                if let Some(bool_obj) = self.get_object(o.id) {
                    let proto_val = JsValue::Object(crate::types::JsObject {
                        id: self
                            .objects
                            .iter()
                            .position(|o| Rc::ptr_eq(o, &proto))
                            .unwrap() as u64,
                    });
                    bool_obj
                        .borrow_mut()
                        .insert_value("prototype".to_string(), proto_val);
                }
            }
        }

        self.boolean_prototype = Some(proto);
    }

    fn create_type_error(&mut self, msg: &str) -> JsValue {
        self.create_error("TypeError", msg)
    }

    fn create_reference_error(&mut self, msg: &str) -> JsValue {
        self.create_error("ReferenceError", msg)
    }

    fn create_error(&mut self, name: &str, msg: &str) -> JsValue {
        let env = self.global_env.borrow();
        let error_proto = env.get(name).and_then(|v| {
            if let JsValue::Object(o) = &v {
                self.get_object(o.id).and_then(|ctor| {
                    let pv = ctor.borrow().get_property("prototype");
                    if let JsValue::Object(p) = &pv {
                        self.get_object(p.id)
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        });
        drop(env);
        let obj = self.create_object();
        {
            let mut o = obj.borrow_mut();
            o.class_name = name.to_string();
            if let Some(proto) = error_proto {
                o.prototype = Some(proto);
            }
            o.insert_value(
                "message".to_string(),
                JsValue::String(JsString::from_str(msg)),
            );
            o.insert_value(
                "name".to_string(),
                JsValue::String(JsString::from_str(name)),
            );
        }
        JsValue::Object(crate::types::JsObject {
            id: self.objects.len() as u64 - 1,
        })
    }

    fn setup_function_prototype(&mut self, obj_proto: &Rc<RefCell<JsObjectData>>) {
        // Add call to Object.prototype (simplified - applies to all functions via prototype chain)
        let call_fn = self.create_function(JsFunction::Native(
            "call".to_string(),
            Rc::new(|interp, _this, args| {
                let this_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let call_args = if args.len() > 1 { &args[1..] } else { &[] };
                interp.call_function(_this, &this_arg, call_args)
            }),
        ));
        obj_proto
            .borrow_mut()
            .insert_builtin("call".to_string(), call_fn);

        // Add apply
        let apply_fn = self.create_function(JsFunction::Native(
            "apply".to_string(),
            Rc::new(|interp, _this, args| {
                let this_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let arr_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let mut call_args = Vec::new();
                if let JsValue::Object(ref o) = arr_arg {
                    if let Some(arr_obj) = interp.get_object(o.id) {
                        let b = arr_obj.borrow();
                        if let Some(elems) = &b.array_elements {
                            call_args = elems.clone();
                        } else {
                            let len = to_number(&b.get_property("length")) as usize;
                            for i in 0..len {
                                call_args.push(b.get_property(&i.to_string()));
                            }
                        }
                    }
                }
                interp.call_function(_this, &this_arg, &call_args)
            }),
        ));
        obj_proto
            .borrow_mut()
            .insert_builtin("apply".to_string(), apply_fn);

        // Function.prototype.bind
        let bind_fn = self.create_function(JsFunction::Native(
            "bind".to_string(),
            Rc::new(|interp, this_val, args: &[JsValue]| {
                let bind_this = args.first().cloned().unwrap_or(JsValue::Undefined);
                let bound_args: Vec<JsValue> = args.iter().skip(1).cloned().collect();
                let func = this_val.clone();
                let bound = JsFunction::Native(
                    "bound".to_string(),
                    Rc::new(move |interp2, _this, call_args: &[JsValue]| {
                        let mut all_args = bound_args.clone();
                        all_args.extend_from_slice(call_args);
                        interp2.call_function(&func, &bind_this, &all_args)
                    }),
                );
                Completion::Normal(interp.create_function(bound))
            }),
        ));
        obj_proto
            .borrow_mut()
            .insert_builtin("bind".to_string(), bind_fn);

        // Merge Function.prototype.toString into Object.prototype.toString
        // The existing Object.prototype.toString already handles [object Type] for non-functions.
        // We override it with a combined version that handles both functions and objects.
        let combined_tostring = self.create_function(JsFunction::Native(
            "toString".to_string(),
            Rc::new(|interp, this_val, _args: &[JsValue]| {
                if let JsValue::Object(o) = this_val {
                    if let Some(obj) = interp.get_object(o.id) {
                        if obj.borrow().callable.is_some() {
                            return Completion::Normal(JsValue::String(JsString::from_str(
                                "function() { [native code] }",
                            )));
                        }
                        let cn = obj.borrow().class_name.clone();
                        return Completion::Normal(JsValue::String(JsString::from_str(&format!(
                            "[object {cn}]"
                        ))));
                    }
                }
                let tag = match this_val {
                    JsValue::Undefined => "Undefined",
                    JsValue::Null => "Null",
                    JsValue::Boolean(_) => "Boolean",
                    JsValue::Number(_) => "Number",
                    JsValue::String(_) => "String",
                    JsValue::Symbol(_) => "Symbol",
                    JsValue::BigInt(_) => "BigInt",
                    JsValue::Object(_) => "Object",
                };
                Completion::Normal(JsValue::String(JsString::from_str(&format!(
                    "[object {tag}]"
                ))))
            }),
        ));
        obj_proto
            .borrow_mut()
            .insert_builtin("toString".to_string(), combined_tostring);
    }

    fn create_object(&mut self) -> Rc<RefCell<JsObjectData>> {
        let mut data = JsObjectData::new();
        data.prototype = self.object_prototype.clone();
        let obj = Rc::new(RefCell::new(data));
        self.objects.push(obj.clone());
        obj
    }

    fn create_function(&mut self, func: JsFunction) -> JsValue {
        let is_arrow = matches!(&func, JsFunction::User { is_arrow: true, .. });
        let (fn_name, fn_length) = match &func {
            JsFunction::User { name, params, .. } => {
                let n = name.clone().unwrap_or_default();
                let len = params
                    .iter()
                    .filter(|p| !matches!(p, Pattern::Rest(_)))
                    .count();
                (n, len)
            }
            JsFunction::Native(name, _) => (name.clone(), 0),
        };
        let mut obj_data = JsObjectData::new();
        obj_data.prototype = self.object_prototype.clone();
        obj_data.callable = Some(func);
        obj_data.class_name = "Function".to_string();
        obj_data.insert_property(
            "length".to_string(),
            PropertyDescriptor::data(JsValue::Number(fn_length as f64), false, false, true),
        );
        obj_data.insert_property(
            "name".to_string(),
            PropertyDescriptor::data(
                JsValue::String(JsString::from_str(&fn_name)),
                false,
                false,
                true,
            ),
        );
        // Non-arrow functions get a prototype property
        if !is_arrow {
            let _proto = self.create_object();
            let proto_val = JsValue::Object(crate::types::JsObject {
                id: self.objects.len() as u64 - 1,
            });
            obj_data.insert_value("prototype".to_string(), proto_val.clone());
            // prototype.constructor will be set after we know the function's id
        }
        let obj = Rc::new(RefCell::new(obj_data));
        self.objects.push(obj.clone());
        let func_id = self.objects.len() as u64 - 1;
        let func_val = JsValue::Object(crate::types::JsObject { id: func_id });
        // Set prototype.constructor = func
        if !is_arrow
            && let Some(JsValue::Object(proto_ref)) = obj.borrow().get_property_value("prototype")
            && let Some(proto_obj) = self.get_object(proto_ref.id)
        {
            proto_obj
                .borrow_mut()
                .insert_builtin("constructor".to_string(), func_val.clone());
        }
        func_val
    }

    fn get_object(&self, id: u64) -> Option<Rc<RefCell<JsObjectData>>> {
        self.objects.get(id as usize).cloned()
    }

    fn create_arguments_object(&mut self, args: &[JsValue]) -> JsValue {
        let obj = self.create_object();
        {
            let mut o = obj.borrow_mut();
            o.class_name = "Arguments".to_string();
            o.insert_value("length".to_string(), JsValue::Number(args.len() as f64));
            for (i, val) in args.iter().enumerate() {
                o.insert_value(i.to_string(), val.clone());
            }
        }
        JsValue::Object(crate::types::JsObject {
            id: self.objects.len() as u64 - 1,
        })
    }

    pub fn run(&mut self, program: &Program) -> Completion {
        self.exec_statements(&program.body, &self.global_env.clone())
    }

    fn exec_statements(&mut self, stmts: &[Statement], env: &EnvRef) -> Completion {
        // Hoist var and function declarations
        for stmt in stmts {
            match stmt {
                Statement::Variable(decl) if decl.kind == VarKind::Var => {
                    for d in &decl.declarations {
                        self.hoist_pattern(&d.pattern, env);
                    }
                }
                Statement::FunctionDeclaration(f) => {
                    env.borrow_mut().declare(&f.name, BindingKind::Var);
                    let func = JsFunction::User {
                        name: Some(f.name.clone()),
                        params: f.params.clone(),
                        body: f.body.clone(),
                        closure: env.clone(),
                        is_arrow: false,
                    };
                    let val = self.create_function(func);
                    let _ = env.borrow_mut().set(&f.name, val);
                }
                _ => {}
            }
        }

        let mut result = JsValue::Undefined;
        for stmt in stmts {
            let comp = self.exec_statement(stmt, env);
            match comp {
                Completion::Normal(val) => result = val,
                other => return other,
            }
        }
        Completion::Normal(result)
    }

    fn hoist_pattern(&self, pat: &Pattern, env: &EnvRef) {
        match pat {
            Pattern::Identifier(name) => {
                if !env.borrow().bindings.contains_key(name) {
                    env.borrow_mut().declare(name, BindingKind::Var);
                }
            }
            Pattern::Array(elems) => {
                for elem in elems.iter().flatten() {
                    match elem {
                        ArrayPatternElement::Pattern(p) | ArrayPatternElement::Rest(p) => {
                            self.hoist_pattern(p, env);
                        }
                    }
                }
            }
            Pattern::Object(props) => {
                for prop in props {
                    match prop {
                        ObjectPatternProperty::KeyValue(_, p) | ObjectPatternProperty::Rest(p) => {
                            self.hoist_pattern(p, env);
                        }
                        ObjectPatternProperty::Shorthand(name) => {
                            if !env.borrow().bindings.contains_key(name) {
                                env.borrow_mut().declare(name, BindingKind::Var);
                            }
                        }
                    }
                }
            }
            Pattern::Assign(inner, _) | Pattern::Rest(inner) => {
                self.hoist_pattern(inner, env);
            }
        }
    }

    fn exec_statement(&mut self, stmt: &Statement, env: &EnvRef) -> Completion {
        match stmt {
            Statement::Empty => Completion::Normal(JsValue::Undefined),
            Statement::Expression(expr) => self.eval_expr(expr, env),
            Statement::Block(stmts) => {
                let block_env = Environment::new(Some(env.clone()));
                self.exec_statements(stmts, &block_env)
            }
            Statement::Variable(decl) => self.exec_variable_declaration(decl, env),
            Statement::If(if_stmt) => {
                let test = self.eval_expr(&if_stmt.test, env);
                let test = match test {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if to_boolean(&test) {
                    self.exec_statement(&if_stmt.consequent, env)
                } else if let Some(alt) = &if_stmt.alternate {
                    self.exec_statement(alt, env)
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            Statement::While(w) => self.exec_while(w, env),
            Statement::DoWhile(dw) => self.exec_do_while(dw, env),
            Statement::For(f) => self.exec_for(f, env),
            Statement::ForIn(fi) => self.exec_for_in(fi, env),
            Statement::ForOf(fo) => self.exec_for_of(fo, env),
            Statement::Return(expr) => {
                let val = if let Some(e) = expr {
                    match self.eval_expr(e, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    }
                } else {
                    JsValue::Undefined
                };
                Completion::Return(val)
            }
            Statement::Break(label) => Completion::Break(label.clone()),
            Statement::Continue(label) => Completion::Continue(label.clone()),
            Statement::Throw(expr) => {
                let val = match self.eval_expr(expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                Completion::Throw(val)
            }
            Statement::Try(t) => self.exec_try(t, env),
            Statement::Switch(s) => self.exec_switch(s, env),
            Statement::Labeled(label, stmt) => {
                let comp = self.exec_statement(stmt, env);
                match &comp {
                    Completion::Break(Some(l)) if l == label => {
                        Completion::Normal(JsValue::Undefined)
                    }
                    Completion::Continue(Some(l)) if l == label => {
                        Completion::Normal(JsValue::Undefined)
                    }
                    _ => comp,
                }
            }
            Statement::With(_, _) => Completion::Normal(JsValue::Undefined), // TODO
            Statement::Debugger => Completion::Normal(JsValue::Undefined),
            Statement::FunctionDeclaration(_) => Completion::Normal(JsValue::Undefined), // hoisted
            Statement::ClassDeclaration(cd) => {
                let class_val = self.eval_class(&cd.name, &cd.super_class, &cd.body, env);
                match class_val {
                    Completion::Normal(val) => {
                        env.borrow_mut().declare(&cd.name, BindingKind::Let);
                        let _ = env.borrow_mut().set(&cd.name, val);
                        Completion::Normal(JsValue::Undefined)
                    }
                    other => other,
                }
            }
        }
    }

    fn exec_variable_declaration(
        &mut self,
        decl: &VariableDeclaration,
        env: &EnvRef,
    ) -> Completion {
        let kind = match decl.kind {
            VarKind::Var => BindingKind::Var,
            VarKind::Let => BindingKind::Let,
            VarKind::Const => BindingKind::Const,
        };
        for d in &decl.declarations {
            if d.init.is_none() && decl.kind == VarKind::Var {
                if let Pattern::Identifier(ref name) = d.pattern {
                    if !env.borrow().bindings.contains_key(name) {
                        env.borrow_mut().declare(name, kind);
                    }
                    continue;
                }
            }
            let val = if let Some(init) = &d.init {
                match self.eval_expr(init, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                }
            } else {
                JsValue::Undefined
            };
            if let Err(e) = self.bind_pattern(&d.pattern, val, kind, env) {
                return Completion::Throw(e);
            }
        }
        Completion::Normal(JsValue::Undefined)
    }

    fn bind_pattern(
        &mut self,
        pat: &Pattern,
        val: JsValue,
        kind: BindingKind,
        env: &EnvRef,
    ) -> Result<(), JsValue> {
        match pat {
            Pattern::Identifier(name) => {
                if kind != BindingKind::Var || !env.borrow().bindings.contains_key(name) {
                    env.borrow_mut().declare(name, kind);
                }
                env.borrow_mut().set(name, val)
            }
            Pattern::Assign(inner, default) => {
                let v = if val.is_undefined() {
                    match self.eval_expr(default, env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Err(e),
                        _ => JsValue::Undefined,
                    }
                } else {
                    val
                };
                self.bind_pattern(inner, v, kind, env)
            }
            Pattern::Array(elements) => {
                for (i, elem) in elements.iter().enumerate() {
                    if let Some(elem) = elem {
                        match elem {
                            ArrayPatternElement::Pattern(p) => {
                                let item = if let JsValue::Object(o) = &val {
                                    if let Some(obj) = self.get_object(o.id) {
                                        obj.borrow()
                                            .array_elements
                                            .as_ref()
                                            .and_then(|e| e.get(i).cloned())
                                            .unwrap_or(JsValue::Undefined)
                                    } else {
                                        JsValue::Undefined
                                    }
                                } else {
                                    JsValue::Undefined
                                };
                                self.bind_pattern(p, item, kind, env)?;
                            }
                            ArrayPatternElement::Rest(p) => {
                                let rest = if let JsValue::Object(o) = &val {
                                    if let Some(obj) = self.get_object(o.id) {
                                        obj.borrow()
                                            .array_elements
                                            .as_ref()
                                            .map(|e| e.get(i..).unwrap_or(&[]).to_vec())
                                            .unwrap_or_default()
                                    } else {
                                        vec![]
                                    }
                                } else {
                                    vec![]
                                };
                                let arr = self.create_array(rest);
                                self.bind_pattern(p, arr, kind, env)?;
                                break;
                            }
                        }
                    }
                }
                Ok(())
            }
            Pattern::Object(props) => {
                for prop in props {
                    match prop {
                        ObjectPatternProperty::Shorthand(name) => {
                            let v = if let JsValue::Object(o) = &val {
                                if let Some(obj) = self.get_object(o.id) {
                                    obj.borrow().get_property(name)
                                } else {
                                    JsValue::Undefined
                                }
                            } else {
                                JsValue::Undefined
                            };
                            if kind != BindingKind::Var || !env.borrow().bindings.contains_key(name)
                            {
                                env.borrow_mut().declare(name, kind);
                            }
                            env.borrow_mut().set(name, v)?;
                        }
                        ObjectPatternProperty::KeyValue(key, pat) => {
                            let key_str = match key {
                                PropertyKey::Identifier(s) | PropertyKey::String(s) => s.clone(),
                                PropertyKey::Number(n) => {
                                    crate::interpreter::to_js_string(&JsValue::Number(*n))
                                }
                                PropertyKey::Computed(expr) => match self.eval_expr(expr, env) {
                                    Completion::Normal(v) => to_js_string(&v),
                                    Completion::Throw(e) => return Err(e),
                                    _ => String::new(),
                                },
                            };
                            let v = if let JsValue::Object(o) = &val {
                                if let Some(obj) = self.get_object(o.id) {
                                    obj.borrow().get_property(&key_str)
                                } else {
                                    JsValue::Undefined
                                }
                            } else {
                                JsValue::Undefined
                            };
                            self.bind_pattern(pat, v, kind, env)?;
                        }
                        ObjectPatternProperty::Rest(pat) => {
                            // TODO: rest in object destructuring
                            let _ = pat;
                        }
                    }
                }
                Ok(())
            }
            Pattern::Rest(inner) => self.bind_pattern(inner, val, kind, env),
        }
    }

    fn exec_while(&mut self, w: &WhileStatement, env: &EnvRef) -> Completion {
        loop {
            let test = match self.eval_expr(&w.test, env) {
                Completion::Normal(v) => v,
                other => return other,
            };
            if !to_boolean(&test) {
                break;
            }
            match self.exec_statement(&w.body, env) {
                Completion::Normal(_) | Completion::Continue(None) => {}
                Completion::Break(None) => break,
                other => return other,
            }
        }
        Completion::Normal(JsValue::Undefined)
    }

    fn exec_do_while(&mut self, dw: &DoWhileStatement, env: &EnvRef) -> Completion {
        loop {
            match self.exec_statement(&dw.body, env) {
                Completion::Normal(_) | Completion::Continue(None) => {}
                Completion::Break(None) => break,
                other => return other,
            }
            let test = match self.eval_expr(&dw.test, env) {
                Completion::Normal(v) => v,
                other => return other,
            };
            if !to_boolean(&test) {
                break;
            }
        }
        Completion::Normal(JsValue::Undefined)
    }

    fn exec_for(&mut self, f: &ForStatement, env: &EnvRef) -> Completion {
        let for_env = Environment::new(Some(env.clone()));
        if let Some(init) = &f.init {
            match init {
                ForInit::Variable(decl) => {
                    // var declarations should go in the parent scope (hoisting)
                    let decl_env = if decl.kind == VarKind::Var {
                        env
                    } else {
                        &for_env
                    };
                    let comp = self.exec_variable_declaration(decl, decl_env);
                    if comp.is_abrupt() {
                        return comp;
                    }
                }
                ForInit::Expression(expr) => {
                    let comp = self.eval_expr(expr, &for_env);
                    if comp.is_abrupt() {
                        return comp;
                    }
                }
            }
        }
        loop {
            if let Some(test) = &f.test {
                let val = match self.eval_expr(test, &for_env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if !to_boolean(&val) {
                    break;
                }
            }
            match self.exec_statement(&f.body, &for_env) {
                Completion::Normal(_) | Completion::Continue(None) => {}
                Completion::Break(None) => break,
                other => return other,
            }
            if let Some(update) = &f.update {
                let comp = self.eval_expr(update, &for_env);
                if comp.is_abrupt() {
                    return comp;
                }
            }
        }
        Completion::Normal(JsValue::Undefined)
    }

    fn exec_for_in(&mut self, fi: &ForInStatement, env: &EnvRef) -> Completion {
        let obj_val = match self.eval_expr(&fi.right, env) {
            Completion::Normal(v) => v,
            other => return other,
        };
        if obj_val.is_nullish() {
            return Completion::Normal(JsValue::Undefined);
        }
        if let JsValue::Object(ref o) = obj_val
            && let Some(obj) = self.get_object(o.id)
        {
            let keys = obj.borrow().enumerable_keys_with_proto();
            for key in keys {
                let key_val = JsValue::String(JsString::from_str(&key));
                let for_env = Environment::new(Some(env.clone()));
                match &fi.left {
                    ForInOfLeft::Variable(decl) => {
                        let kind = match decl.kind {
                            VarKind::Var => BindingKind::Var,
                            VarKind::Let => BindingKind::Let,
                            VarKind::Const => BindingKind::Const,
                        };
                        let bind_env = if decl.kind == VarKind::Var {
                            env
                        } else {
                            &for_env
                        };
                        if let Some(d) = decl.declarations.first()
                            && let Err(e) = self.bind_pattern(&d.pattern, key_val, kind, bind_env)
                        {
                            return Completion::Throw(e);
                        }
                    }
                    ForInOfLeft::Pattern(pat) => {
                        if let Pattern::Identifier(name) = pat {
                            let _ = env.borrow_mut().set(name, key_val);
                        }
                    }
                }
                match self.exec_statement(&fi.body, &for_env) {
                    Completion::Normal(_) | Completion::Continue(None) => {}
                    Completion::Break(None) => break,
                    other => return other,
                }
            }
        }
        Completion::Normal(JsValue::Undefined)
    }

    fn exec_for_of(&mut self, fo: &ForOfStatement, env: &EnvRef) -> Completion {
        let iterable = match self.eval_expr(&fo.right, env) {
            Completion::Normal(v) => v,
            other => return other,
        };
        // Get iterable values - check for Symbol.iterator first, then fallback to arrays/strings
        let values: Vec<JsValue> = if let JsValue::String(ref s) = iterable {
            s.to_rust_string()
                .chars()
                .map(|c| JsValue::String(JsString::from_str(&c.to_string())))
                .collect()
        } else if let JsValue::Object(ref o) = iterable {
            if let Some(obj) = self.get_object(o.id) {
                let has_array = obj.borrow().array_elements.is_some();
                if has_array {
                    obj.borrow().array_elements.clone().unwrap_or_default()
                } else {
                    // Try Symbol.iterator protocol
                    match self.call_iterator(&iterable) {
                        Some(vals) => vals,
                        None => {
                            let err = self.create_type_error("is not iterable");
                            return Completion::Throw(err);
                        }
                    }
                }
            } else {
                return Completion::Normal(JsValue::Undefined);
            }
        } else {
            let err = self.create_type_error("is not iterable");
            return Completion::Throw(err);
        };

        for val in values {
            let for_env = Environment::new(Some(env.clone()));
            match &fo.left {
                ForInOfLeft::Variable(decl) => {
                    let kind = match decl.kind {
                        VarKind::Var => BindingKind::Var,
                        VarKind::Let => BindingKind::Let,
                        VarKind::Const => BindingKind::Const,
                    };
                    let bind_env = if decl.kind == VarKind::Var {
                        env
                    } else {
                        &for_env
                    };
                    if let Some(d) = decl.declarations.first()
                        && let Err(e) = self.bind_pattern(&d.pattern, val, kind, bind_env)
                    {
                        return Completion::Throw(e);
                    }
                }
                ForInOfLeft::Pattern(pat) => {
                    if let Pattern::Identifier(name) = pat {
                        let _ = env.borrow_mut().set(name, val);
                    } else if let Err(e) = self.bind_pattern(pat, val, BindingKind::Let, &for_env) {
                        return Completion::Throw(e);
                    }
                }
            }
            match self.exec_statement(&fo.body, &for_env) {
                Completion::Normal(_) | Completion::Continue(None) => {}
                Completion::Break(None) => break,
                other => return other,
            }
        }
        Completion::Normal(JsValue::Undefined)
    }

    fn exec_try(&mut self, t: &TryStatement, env: &EnvRef) -> Completion {
        let block_env = Environment::new(Some(env.clone()));
        let result = self.exec_statements(&t.block, &block_env);
        let result = match result {
            Completion::Throw(val) => {
                if let Some(handler) = &t.handler {
                    let catch_env = Environment::new(Some(env.clone()));
                    if let Some(param) = &handler.param
                        && let Err(e) = self.bind_pattern(param, val, BindingKind::Let, &catch_env)
                    {
                        return Completion::Throw(e);
                    }
                    self.exec_statements(&handler.body, &catch_env)
                } else {
                    Completion::Throw(val)
                }
            }
            other => other,
        };
        if let Some(finalizer) = &t.finalizer {
            let fin_env = Environment::new(Some(env.clone()));
            let fin_result = self.exec_statements(finalizer, &fin_env);
            if fin_result.is_abrupt() {
                return fin_result;
            }
        }
        result
    }

    fn exec_switch(&mut self, s: &SwitchStatement, env: &EnvRef) -> Completion {
        let disc = match self.eval_expr(&s.discriminant, env) {
            Completion::Normal(v) => v,
            other => return other,
        };
        let switch_env = Environment::new(Some(env.clone()));
        let mut found = false;
        let mut default_idx = None;
        for (i, case) in s.cases.iter().enumerate() {
            if case.test.is_none() {
                default_idx = Some(i);
                continue;
            }
            if !found {
                let test = match self.eval_expr(case.test.as_ref().unwrap(), &switch_env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if strict_equality(&disc, &test) {
                    found = true;
                }
            }
            if found {
                for stmt in &case.consequent {
                    match self.exec_statement(stmt, &switch_env) {
                        Completion::Normal(_) => {}
                        Completion::Break(None) => return Completion::Normal(JsValue::Undefined),
                        other => return other,
                    }
                }
            }
        }
        if !found && let Some(idx) = default_idx {
            for case in &s.cases[idx..] {
                for stmt in &case.consequent {
                    match self.exec_statement(stmt, &switch_env) {
                        Completion::Normal(_) => {}
                        Completion::Break(None) => {
                            return Completion::Normal(JsValue::Undefined);
                        }
                        other => return other,
                    }
                }
            }
        }
        Completion::Normal(JsValue::Undefined)
    }

    fn eval_expr(&mut self, expr: &Expression, env: &EnvRef) -> Completion {
        match expr {
            Expression::Literal(lit) => Completion::Normal(self.eval_literal(lit)),
            Expression::Identifier(name) => match env.borrow().get(name) {
                Some(val) => Completion::Normal(val),
                None => {
                    let err = self.create_reference_error(&format!("{name} is not defined"));
                    Completion::Throw(err)
                }
            },
            Expression::This => {
                Completion::Normal(env.borrow().get("this").unwrap_or(JsValue::Undefined))
            }
            Expression::Super => {
                Completion::Normal(env.borrow().get("__super__").unwrap_or(JsValue::Undefined))
            }
            Expression::Unary(op, operand) => {
                let val = match self.eval_expr(operand, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                Completion::Normal(self.eval_unary(*op, &val))
            }
            Expression::Binary(op, left, right) => {
                let lval = match self.eval_expr(left, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let rval = match self.eval_expr(right, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                Completion::Normal(self.eval_binary(*op, &lval, &rval))
            }
            Expression::Logical(op, left, right) => self.eval_logical(*op, left, right, env),
            Expression::Update(op, prefix, arg) => self.eval_update(*op, *prefix, arg, env),
            Expression::Assign(op, left, right) => self.eval_assign(*op, left, right, env),
            Expression::Conditional(test, cons, alt) => {
                let test_val = match self.eval_expr(test, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if to_boolean(&test_val) {
                    self.eval_expr(cons, env)
                } else {
                    self.eval_expr(alt, env)
                }
            }
            Expression::Call(callee, args) => self.eval_call(callee, args, env),
            Expression::New(callee, args) => self.eval_new(callee, args, env),
            Expression::Member(obj, prop) => self.eval_member(obj, prop, env),
            Expression::Array(elements) => self.eval_array_literal(elements, env),
            Expression::Object(props) => self.eval_object_literal(props, env),
            Expression::Function(f) => {
                let func = JsFunction::User {
                    name: f.name.clone(),
                    params: f.params.clone(),
                    body: f.body.clone(),
                    closure: env.clone(),
                    is_arrow: false,
                };
                Completion::Normal(self.create_function(func))
            }
            Expression::ArrowFunction(af) => {
                let body_stmts = match &af.body {
                    ArrowBody::Block(stmts) => stmts.clone(),
                    ArrowBody::Expression(expr) => {
                        vec![Statement::Return(Some(*expr.clone()))]
                    }
                };
                let func = JsFunction::User {
                    name: None,
                    params: af.params.clone(),
                    body: body_stmts,
                    closure: env.clone(),
                    is_arrow: true,
                };
                Completion::Normal(self.create_function(func))
            }
            Expression::Class(ce) => {
                let name = ce.name.clone().unwrap_or_default();
                self.eval_class(&name, &ce.super_class, &ce.body, env)
            }
            Expression::Typeof(operand) => {
                // typeof on unresolvable reference returns "undefined"
                if let Expression::Identifier(name) = operand.as_ref() {
                    let val = env.borrow().get(name).unwrap_or(JsValue::Undefined);
                    return Completion::Normal(JsValue::String(JsString::from_str(typeof_val(
                        &val,
                        &self.objects,
                    ))));
                }
                let val = match self.eval_expr(operand, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                Completion::Normal(JsValue::String(JsString::from_str(typeof_val(
                    &val,
                    &self.objects,
                ))))
            }
            Expression::Void(operand) => {
                match self.eval_expr(operand, env) {
                    Completion::Normal(_) => {}
                    other => return other,
                }
                Completion::Normal(JsValue::Undefined)
            }
            Expression::Delete(expr) => match expr.as_ref() {
                Expression::Member(obj_expr, prop) => {
                    let obj_val = match self.eval_expr(obj_expr, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let key = match prop {
                        MemberProperty::Dot(name) => name.clone(),
                        MemberProperty::Computed(expr) => match self.eval_expr(expr, env) {
                            Completion::Normal(v) => to_js_string(&v),
                            other => return other,
                        },
                    };
                    if let JsValue::Object(o) = &obj_val {
                        if let Some(obj) = self.get_object(o.id) {
                            let mut obj_mut = obj.borrow_mut();
                            if let Some(desc) = obj_mut.properties.get(&key) {
                                if desc.configurable == Some(false) {
                                    return Completion::Normal(JsValue::Boolean(false));
                                }
                            }
                            obj_mut.properties.remove(&key);
                            obj_mut.property_order.retain(|k| k != &key);
                        }
                    }
                    Completion::Normal(JsValue::Boolean(true))
                }
                _ => Completion::Normal(JsValue::Boolean(true)),
            },
            Expression::Sequence(exprs) | Expression::Comma(exprs) => {
                let mut result = JsValue::Undefined;
                for e in exprs {
                    result = match self.eval_expr(e, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                }
                Completion::Normal(result)
            }
            Expression::Spread(_) => Completion::Normal(JsValue::Undefined), // handled by caller
            Expression::Yield(expr, _delegate) => {
                // Stub: generators not yet implemented, evaluate the expression and return it
                if let Some(expr) = expr {
                    self.eval_expr(expr, env)
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            Expression::Template(tmpl) => {
                let mut s = String::new();
                for (i, quasi) in tmpl.quasis.iter().enumerate() {
                    s.push_str(quasi);
                    if i < tmpl.expressions.len() {
                        let val = match self.eval_expr(&tmpl.expressions[i], env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        s.push_str(&format!("{val}"));
                    }
                }
                Completion::Normal(JsValue::String(JsString::from_str(&s)))
            }
            _ => Completion::Normal(JsValue::Undefined),
        }
    }

    fn eval_literal(&mut self, lit: &Literal) -> JsValue {
        match lit {
            Literal::Null => JsValue::Null,
            Literal::Boolean(b) => JsValue::Boolean(*b),
            Literal::Number(n) => JsValue::Number(*n),
            Literal::String(s) => JsValue::String(JsString::from_str(s)),
            Literal::BigInt(_) => JsValue::Undefined, // TODO
            Literal::RegExp(pattern, flags) => {
                let mut obj = JsObjectData::new();
                obj.prototype = self
                    .regexp_prototype
                    .clone()
                    .or(self.object_prototype.clone());
                obj.class_name = "RegExp".to_string();
                obj.insert_value(
                    "source".to_string(),
                    JsValue::String(JsString::from_str(pattern)),
                );
                obj.insert_value(
                    "flags".to_string(),
                    JsValue::String(JsString::from_str(flags)),
                );
                obj.insert_value("global".to_string(), JsValue::Boolean(flags.contains('g')));
                obj.insert_value(
                    "ignoreCase".to_string(),
                    JsValue::Boolean(flags.contains('i')),
                );
                obj.insert_value(
                    "multiline".to_string(),
                    JsValue::Boolean(flags.contains('m')),
                );
                obj.insert_value("dotAll".to_string(), JsValue::Boolean(flags.contains('s')));
                obj.insert_value("unicode".to_string(), JsValue::Boolean(flags.contains('u')));
                obj.insert_value("sticky".to_string(), JsValue::Boolean(flags.contains('y')));
                obj.insert_value("lastIndex".to_string(), JsValue::Number(0.0));
                let rc = Rc::new(RefCell::new(obj));
                self.objects.push(rc);
                JsValue::Object(crate::types::JsObject {
                    id: self.objects.len() as u64 - 1,
                })
            }
        }
    }

    fn eval_unary(&self, op: UnaryOp, val: &JsValue) -> JsValue {
        match op {
            UnaryOp::Minus => JsValue::Number(number_ops::unary_minus(to_number(val))),
            UnaryOp::Plus => JsValue::Number(to_number(val)),
            UnaryOp::Not => JsValue::Boolean(!to_boolean(val)),
            UnaryOp::BitNot => JsValue::Number(number_ops::bitwise_not(to_number(val))),
        }
    }

    fn call_iterator(&mut self, iterable: &JsValue) -> Option<Vec<JsValue>> {
        if let JsValue::Object(o) = iterable {
            // Look for Symbol.iterator property - symbols are stored as string keys like "Symbol(Symbol.iterator)"
            let iter_fn = if let Some(obj) = self.get_object(o.id) {
                let sym_key = self.global_env.borrow().get("Symbol").and_then(|sv| {
                    if let JsValue::Object(so) = sv {
                        self.get_object(so.id).map(|sobj| {
                            let val = sobj.borrow().get_property("iterator");
                            to_js_string(&val)
                        })
                    } else {
                        None
                    }
                });
                if let Some(key) = sym_key {
                    let val = obj.borrow().get_property(&key);
                    if matches!(val, JsValue::Object(_)) {
                        Some(val)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(iter_fn) = iter_fn {
                let iterator = match self.call_function(&iter_fn, iterable, &[]) {
                    Completion::Normal(v) => v,
                    _ => return None,
                };
                let mut values = Vec::new();
                for _ in 0..100_000 {
                    if let JsValue::Object(io) = &iterator {
                        let next_fn = self.get_object(io.id).and_then(|obj| {
                            obj.borrow()
                                .get_property_descriptor("next")
                                .and_then(|d| d.value)
                        });
                        if let Some(next_fn) = next_fn {
                            match self.call_function(&next_fn, &iterator, &[]) {
                                Completion::Normal(result) => {
                                    if let JsValue::Object(ro) = &result {
                                        if let Some(robj) = self.get_object(ro.id) {
                                            let done = robj.borrow().get_property("done");
                                            if to_boolean(&done) {
                                                break;
                                            }
                                            let value = robj.borrow().get_property("value");
                                            values.push(value);
                                        } else {
                                            break;
                                        }
                                    } else {
                                        break;
                                    }
                                }
                                _ => break,
                            }
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                return Some(values);
            }
        }
        None
    }

    fn require_object_coercible(&mut self, val: &JsValue) -> Completion {
        match val {
            JsValue::Undefined | JsValue::Null => {
                let err = self.create_type_error("Cannot convert undefined or null to object");
                Completion::Throw(err)
            }
            _ => Completion::Normal(val.clone()),
        }
    }

    fn is_regexp(&self, val: &JsValue) -> bool {
        if let JsValue::Object(o) = val {
            if let Some(obj) = self.get_object(o.id) {
                return obj.borrow().class_name == "RegExp";
            }
        }
        false
    }

    fn canonical_numeric_index_string(s: &str) -> Option<f64> {
        if s == "-0" {
            return Some(-0.0_f64);
        }
        let n: f64 = s.parse().ok()?;
        if format!("{n}") == s { Some(n) } else { None }
    }

    fn to_index(&mut self, val: &JsValue) -> Completion {
        if val.is_undefined() {
            return Completion::Normal(JsValue::Number(0.0));
        }
        let integer_index = to_number(val);
        let integer_index = if integer_index.is_nan() {
            0.0
        } else {
            integer_index.trunc()
        };
        if integer_index < 0.0 || integer_index > 9007199254740991.0 {
            let err = self.create_error("RangeError", "Invalid index");
            return Completion::Throw(err);
        }
        Completion::Normal(JsValue::Number(integer_index))
    }

    fn to_length(val: &JsValue) -> f64 {
        let len = to_number(val);
        if len.is_nan() || len <= 0.0 {
            return 0.0;
        }
        len.min(9007199254740991.0).floor() // 2^53 - 1
    }

    fn to_object(&mut self, val: &JsValue) -> Completion {
        match val {
            JsValue::Undefined | JsValue::Null => {
                let err = self.create_type_error("Cannot convert undefined or null to object");
                Completion::Throw(err)
            }
            JsValue::Boolean(_)
            | JsValue::Number(_)
            | JsValue::String(_)
            | JsValue::Symbol(_)
            | JsValue::BigInt(_) => {
                let mut obj_data = JsObjectData::new();
                obj_data.primitive_value = Some(val.clone());
                match val {
                    JsValue::String(_) => {
                        obj_data.class_name = "String".to_string();
                        if let Some(ref sp) = self.string_prototype {
                            obj_data.prototype = Some(sp.clone());
                        }
                    }
                    JsValue::Number(_) => {
                        obj_data.class_name = "Number".to_string();
                        if let Some(ref np) = self.number_prototype {
                            obj_data.prototype = Some(np.clone());
                        }
                    }
                    JsValue::Boolean(_) => {
                        obj_data.class_name = "Boolean".to_string();
                        if let Some(ref bp) = self.boolean_prototype {
                            obj_data.prototype = Some(bp.clone());
                        }
                    }
                    JsValue::Symbol(_) => obj_data.class_name = "Symbol".to_string(),
                    JsValue::BigInt(_) => obj_data.class_name = "BigInt".to_string(),
                    _ => unreachable!(),
                }
                if obj_data.prototype.is_none() {
                    obj_data.prototype = self.object_prototype.clone();
                }
                let obj = Rc::new(RefCell::new(obj_data));
                self.objects.push(obj);
                Completion::Normal(JsValue::Object(crate::types::JsObject {
                    id: self.objects.len() as u64 - 1,
                }))
            }
            JsValue::Object(_) => Completion::Normal(val.clone()),
        }
    }

    fn to_primitive(&mut self, val: &JsValue, preferred_type: &str) -> JsValue {
        match val {
            JsValue::Object(o) => {
                let methods = if preferred_type == "string" {
                    ["toString", "valueOf"]
                } else {
                    ["valueOf", "toString"]
                };
                for method_name in &methods {
                    let method = if let Some(obj) = self.get_object(o.id) {
                        let desc = obj.borrow().get_property_descriptor(method_name);
                        desc.and_then(|d| d.value)
                    } else {
                        None
                    };
                    if let Some(func) = method {
                        if let JsValue::Object(fo) = &func {
                            if self
                                .get_object(fo.id)
                                .map(|o| o.borrow().callable.is_some())
                                .unwrap_or(false)
                            {
                                let result = self.call_function(&func, val, &[]);
                                match result {
                                    Completion::Normal(v) if !matches!(v, JsValue::Object(_)) => {
                                        return v;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                // Fallback: check for primitive_value (wrapper objects)
                if let Some(obj) = self.get_object(o.id) {
                    if let Some(pv) = obj.borrow().primitive_value.clone() {
                        return pv;
                    }
                }
                JsValue::String(JsString::from_str("[object Object]"))
            }
            _ => val.clone(),
        }
    }

    fn to_number_coerce(&mut self, val: &JsValue) -> f64 {
        let prim = self.to_primitive(val, "number");
        to_number(&prim)
    }

    fn abstract_equality(&mut self, left: &JsValue, right: &JsValue) -> bool {
        if std::mem::discriminant(left) == std::mem::discriminant(right) {
            return strict_equality(left, right);
        }
        if (left.is_null() && right.is_undefined()) || (left.is_undefined() && right.is_null()) {
            return true;
        }
        if left.is_number() && right.is_string() {
            return self.abstract_equality(left, &JsValue::Number(to_number(right)));
        }
        if left.is_string() && right.is_number() {
            return self.abstract_equality(&JsValue::Number(to_number(left)), right);
        }
        if left.is_boolean() {
            return self.abstract_equality(&JsValue::Number(to_number(left)), right);
        }
        if right.is_boolean() {
            return self.abstract_equality(left, &JsValue::Number(to_number(right)));
        }
        // Object vs primitive
        if matches!(left, JsValue::Object(_))
            && (right.is_string() || right.is_number() || right.is_symbol())
        {
            let lprim = self.to_primitive(left, "default");
            return self.abstract_equality(&lprim, right);
        }
        if matches!(right, JsValue::Object(_))
            && (left.is_string() || left.is_number() || left.is_symbol())
        {
            let rprim = self.to_primitive(right, "default");
            return self.abstract_equality(left, &rprim);
        }
        false
    }

    fn abstract_relational(&mut self, left: &JsValue, right: &JsValue) -> Option<bool> {
        let lprim = self.to_primitive(left, "number");
        let rprim = self.to_primitive(right, "number");
        if is_string(&lprim) && is_string(&rprim) {
            let ls = to_js_string(&lprim);
            let rs = to_js_string(&rprim);
            return Some(ls < rs);
        }
        let ln = to_number(&lprim);
        let rn = to_number(&rprim);
        number_ops::less_than(ln, rn)
    }

    fn eval_binary(&mut self, op: BinaryOp, left: &JsValue, right: &JsValue) -> JsValue {
        match op {
            BinaryOp::Add => {
                let lprim = self.to_primitive(left, "default");
                let rprim = self.to_primitive(right, "default");
                if is_string(&lprim) || is_string(&rprim) {
                    let ls = to_js_string(&lprim);
                    let rs = to_js_string(&rprim);
                    JsValue::String(JsString::from_str(&format!("{ls}{rs}")))
                } else {
                    JsValue::Number(number_ops::add(to_number(&lprim), to_number(&rprim)))
                }
            }
            BinaryOp::Sub => JsValue::Number(number_ops::subtract(
                self.to_number_coerce(left),
                self.to_number_coerce(right),
            )),
            BinaryOp::Mul => JsValue::Number(number_ops::multiply(
                self.to_number_coerce(left),
                self.to_number_coerce(right),
            )),
            BinaryOp::Div => JsValue::Number(number_ops::divide(
                self.to_number_coerce(left),
                self.to_number_coerce(right),
            )),
            BinaryOp::Mod => JsValue::Number(number_ops::remainder(
                self.to_number_coerce(left),
                self.to_number_coerce(right),
            )),
            BinaryOp::Exp => JsValue::Number(number_ops::exponentiate(
                self.to_number_coerce(left),
                self.to_number_coerce(right),
            )),
            BinaryOp::Eq => JsValue::Boolean(self.abstract_equality(left, right)),
            BinaryOp::NotEq => JsValue::Boolean(!self.abstract_equality(left, right)),
            BinaryOp::StrictEq => JsValue::Boolean(strict_equality(left, right)),
            BinaryOp::StrictNotEq => JsValue::Boolean(!strict_equality(left, right)),
            BinaryOp::Lt => JsValue::Boolean(self.abstract_relational(left, right) == Some(true)),
            BinaryOp::Gt => JsValue::Boolean(self.abstract_relational(right, left) == Some(true)),
            BinaryOp::LtEq => {
                JsValue::Boolean(self.abstract_relational(right, left) == Some(false))
            }
            BinaryOp::GtEq => {
                JsValue::Boolean(self.abstract_relational(left, right) == Some(false))
            }
            BinaryOp::LShift => JsValue::Number(number_ops::left_shift(
                self.to_number_coerce(left),
                self.to_number_coerce(right),
            )),
            BinaryOp::RShift => JsValue::Number(number_ops::signed_right_shift(
                self.to_number_coerce(left),
                self.to_number_coerce(right),
            )),
            BinaryOp::URShift => JsValue::Number(number_ops::unsigned_right_shift(
                self.to_number_coerce(left),
                self.to_number_coerce(right),
            )),
            BinaryOp::BitAnd => JsValue::Number(number_ops::bitwise_and(
                self.to_number_coerce(left),
                self.to_number_coerce(right),
            )),
            BinaryOp::BitOr => JsValue::Number(number_ops::bitwise_or(
                self.to_number_coerce(left),
                self.to_number_coerce(right),
            )),
            BinaryOp::BitXor => JsValue::Number(number_ops::bitwise_xor(
                self.to_number_coerce(left),
                self.to_number_coerce(right),
            )),
            BinaryOp::In => {
                if let JsValue::Object(o) = &right {
                    if let Some(obj) = self.get_object(o.id) {
                        let key = to_js_string(&left);
                        let obj_ref = obj.borrow();
                        JsValue::Boolean(obj_ref.has_property(&key))
                    } else {
                        JsValue::Boolean(false)
                    }
                } else {
                    JsValue::Boolean(false)
                }
            }
            BinaryOp::Instanceof => {
                if let JsValue::Object(rhs) = &right {
                    if let Some(ctor_obj) = self.get_object(rhs.id) {
                        let proto_val = ctor_obj.borrow().get_property("prototype");
                        if let JsValue::Object(proto) = &proto_val {
                            if let Some(proto_data) = self.get_object(proto.id) {
                                if let JsValue::Object(lhs) = &left {
                                    if let Some(inst_obj) = self.get_object(lhs.id) {
                                        let mut current = inst_obj.borrow().prototype.clone();
                                        let mut result = false;
                                        while let Some(p) = current {
                                            if Rc::ptr_eq(&p, &proto_data) {
                                                result = true;
                                                break;
                                            }
                                            current = p.borrow().prototype.clone();
                                        }
                                        JsValue::Boolean(result)
                                    } else {
                                        JsValue::Boolean(false)
                                    }
                                } else {
                                    JsValue::Boolean(false)
                                }
                            } else {
                                JsValue::Boolean(false)
                            }
                        } else {
                            JsValue::Boolean(false)
                        }
                    } else {
                        JsValue::Boolean(false)
                    }
                } else {
                    JsValue::Boolean(false)
                }
            }
        }
    }

    fn eval_logical(
        &mut self,
        op: LogicalOp,
        left: &Expression,
        right: &Expression,
        env: &EnvRef,
    ) -> Completion {
        let lval = match self.eval_expr(left, env) {
            Completion::Normal(v) => v,
            other => return other,
        };
        match op {
            LogicalOp::And => {
                if !to_boolean(&lval) {
                    Completion::Normal(lval)
                } else {
                    self.eval_expr(right, env)
                }
            }
            LogicalOp::Or => {
                if to_boolean(&lval) {
                    Completion::Normal(lval)
                } else {
                    self.eval_expr(right, env)
                }
            }
            LogicalOp::NullishCoalescing => {
                if lval.is_nullish() {
                    self.eval_expr(right, env)
                } else {
                    Completion::Normal(lval)
                }
            }
        }
    }

    fn eval_update(
        &mut self,
        op: UpdateOp,
        prefix: bool,
        arg: &Expression,
        env: &EnvRef,
    ) -> Completion {
        if let Expression::Identifier(name) = arg {
            let old_val = match env.borrow().get(name) {
                Some(v) => to_number(&v),
                None => {
                    let err = self.create_reference_error(&format!("{name} is not defined"));
                    return Completion::Throw(err);
                }
            };
            let new_val = match op {
                UpdateOp::Increment => old_val + 1.0,
                UpdateOp::Decrement => old_val - 1.0,
            };
            if let Err(e) = env.borrow_mut().set(name, JsValue::Number(new_val)) {
                return Completion::Throw(e);
            }
            Completion::Normal(JsValue::Number(if prefix { new_val } else { old_val }))
        } else {
            // TODO: member expression update
            Completion::Normal(JsValue::Number(f64::NAN))
        }
    }

    fn eval_assign(
        &mut self,
        op: AssignOp,
        left: &Expression,
        right: &Expression,
        env: &EnvRef,
    ) -> Completion {
        // Logical assignments are short-circuit
        if matches!(
            op,
            AssignOp::LogicalAndAssign | AssignOp::LogicalOrAssign | AssignOp::NullishAssign
        ) {
            let lval = match self.eval_expr(left, env) {
                Completion::Normal(v) => v,
                other => return other,
            };
            let should_assign = match op {
                AssignOp::LogicalAndAssign => to_boolean(&lval),
                AssignOp::LogicalOrAssign => !to_boolean(&lval),
                AssignOp::NullishAssign => lval.is_null() || lval.is_undefined(),
                _ => unreachable!(),
            };
            if !should_assign {
                return Completion::Normal(lval);
            }
            let rval = match self.eval_expr(right, env) {
                Completion::Normal(v) => v,
                other => return other,
            };
            if let Expression::Identifier(name) = left {
                let _ = env.borrow_mut().set(name, rval.clone());
            }
            return Completion::Normal(rval);
        }

        let rval = match self.eval_expr(right, env) {
            Completion::Normal(v) => v,
            other => return other,
        };

        match left {
            Expression::Identifier(name) => {
                let final_val = if op == AssignOp::Assign {
                    rval
                } else {
                    let lval = env.borrow().get(name).unwrap_or(JsValue::Undefined);
                    self.apply_compound_assign(op, &lval, &rval)
                };
                if !env.borrow().has(name) {
                    env.borrow_mut().declare(name, BindingKind::Var);
                }
                if let Err(e) = env.borrow_mut().set(name, final_val.clone()) {
                    return Completion::Throw(e);
                }
                Completion::Normal(final_val)
            }
            Expression::Member(obj_expr, prop) => {
                let obj_val = match self.eval_expr(obj_expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let key = match prop {
                    MemberProperty::Dot(name) => name.clone(),
                    MemberProperty::Computed(expr) => {
                        let v = match self.eval_expr(expr, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        to_js_string(&v)
                    }
                };
                if let JsValue::Object(ref o) = obj_val
                    && let Some(obj) = self.get_object(o.id)
                {
                    let final_val = if op == AssignOp::Assign {
                        rval
                    } else {
                        let lval = obj.borrow().get_property(&key);
                        self.apply_compound_assign(op, &lval, &rval)
                    };
                    // Check for setter
                    let desc = obj.borrow().get_property_descriptor(&key);
                    if let Some(ref d) = desc {
                        if let Some(ref setter) = d.set {
                            let setter = setter.clone();
                            let this = obj_val.clone();
                            return match self.call_function(&setter, &this, &[final_val.clone()]) {
                                Completion::Normal(_) => Completion::Normal(final_val),
                                other => other,
                            };
                        }
                    }
                    obj.borrow_mut().set_property_value(&key, final_val.clone());
                    return Completion::Normal(final_val);
                }
                Completion::Normal(rval)
            }
            Expression::Array(elements) if op == AssignOp::Assign => {
                // Destructuring array assignment
                for (i, elem) in elements.iter().enumerate() {
                    if let Some(expr) = elem {
                        if let Expression::Spread(inner) = expr {
                            let rest: Vec<JsValue> = if let JsValue::Object(o) = &rval {
                                if let Some(obj) = self.get_object(o.id) {
                                    obj.borrow()
                                        .array_elements
                                        .as_ref()
                                        .map(|e| e.get(i..).unwrap_or(&[]).to_vec())
                                        .unwrap_or_default()
                                } else {
                                    vec![]
                                }
                            } else {
                                vec![]
                            };
                            let arr = self.create_array(rest);
                            let result = self.eval_assign(
                                AssignOp::Assign,
                                inner,
                                &Expression::Literal(Literal::Null),
                                env,
                            );
                            // Assign directly
                            if let Expression::Identifier(name) = inner.as_ref() {
                                if !env.borrow().has(name) {
                                    env.borrow_mut().declare(name, BindingKind::Var);
                                }
                                let _ = env.borrow_mut().set(name, arr);
                            }
                            break;
                        }
                        let item = if let JsValue::Object(o) = &rval {
                            if let Some(obj) = self.get_object(o.id) {
                                obj.borrow()
                                    .array_elements
                                    .as_ref()
                                    .and_then(|e| e.get(i).cloned())
                                    .unwrap_or(JsValue::Undefined)
                            } else {
                                JsValue::Undefined
                            }
                        } else {
                            JsValue::Undefined
                        };
                        // Check for default value: `[a = defaultVal] = arr`
                        let (target, val) =
                            if let Expression::Assign(AssignOp::Assign, target, default) = expr {
                                let v = if item.is_undefined() {
                                    match self.eval_expr(default, env) {
                                        Completion::Normal(v) => v,
                                        other => return other,
                                    }
                                } else {
                                    item
                                };
                                (target.as_ref(), v)
                            } else {
                                (expr, item)
                            };
                        match target {
                            Expression::Identifier(name) => {
                                if !env.borrow().has(name) {
                                    env.borrow_mut().declare(name, BindingKind::Var);
                                }
                                let _ = env.borrow_mut().set(name, val);
                            }
                            Expression::Member(..) => {
                                // Create a temp to hold the val, assign to member
                                let temp_lit = Expression::Literal(Literal::Null);
                                // We'd need to manually do the member assign here
                                // For now, skip complex member destructuring
                            }
                            _ => {}
                        }
                    }
                }
                Completion::Normal(rval)
            }
            Expression::Object(props) if op == AssignOp::Assign => {
                // Destructuring object assignment
                for prop in props {
                    let (key, target, default_val) = match &prop.kind {
                        PropertyKind::Init => {
                            let key = match &prop.key {
                                PropertyKey::Identifier(s) | PropertyKey::String(s) => s.clone(),
                                PropertyKey::Number(n) => to_js_string(&JsValue::Number(*n)),
                                PropertyKey::Computed(expr) => match self.eval_expr(expr, env) {
                                    Completion::Normal(v) => to_js_string(&v),
                                    other => return other,
                                },
                            };
                            // Check if shorthand ({a} = obj) or key-value ({a: b} = obj)
                            if let Expression::Identifier(name) = &prop.value {
                                if name == &key {
                                    (key, prop.value.clone(), None)
                                } else {
                                    (key, prop.value.clone(), None)
                                }
                            } else if let Expression::Assign(AssignOp::Assign, target, default) =
                                &prop.value
                            {
                                (key, *target.clone(), Some(*default.clone()))
                            } else {
                                (key, prop.value.clone(), None)
                            }
                        }
                        _ => continue,
                    };
                    let val = if let JsValue::Object(o) = &rval {
                        if let Some(obj) = self.get_object(o.id) {
                            obj.borrow().get_property(&key)
                        } else {
                            JsValue::Undefined
                        }
                    } else {
                        JsValue::Undefined
                    };
                    let val = if val.is_undefined() {
                        if let Some(default) = default_val {
                            match self.eval_expr(&default, env) {
                                Completion::Normal(v) => v,
                                other => return other,
                            }
                        } else {
                            val
                        }
                    } else {
                        val
                    };
                    match &target {
                        Expression::Identifier(name) => {
                            if !env.borrow().has(name) {
                                env.borrow_mut().declare(name, BindingKind::Var);
                            }
                            let _ = env.borrow_mut().set(name, val);
                        }
                        _ => {}
                    }
                }
                Completion::Normal(rval)
            }
            _ => Completion::Normal(rval),
        }
    }

    fn apply_compound_assign(&mut self, op: AssignOp, lval: &JsValue, rval: &JsValue) -> JsValue {
        match op {
            AssignOp::AddAssign => self.eval_binary(BinaryOp::Add, lval, rval),
            AssignOp::SubAssign => self.eval_binary(BinaryOp::Sub, lval, rval),
            AssignOp::MulAssign => self.eval_binary(BinaryOp::Mul, lval, rval),
            AssignOp::DivAssign => self.eval_binary(BinaryOp::Div, lval, rval),
            AssignOp::ModAssign => self.eval_binary(BinaryOp::Mod, lval, rval),
            AssignOp::ExpAssign => self.eval_binary(BinaryOp::Exp, lval, rval),
            AssignOp::LShiftAssign => self.eval_binary(BinaryOp::LShift, lval, rval),
            AssignOp::RShiftAssign => self.eval_binary(BinaryOp::RShift, lval, rval),
            AssignOp::URShiftAssign => self.eval_binary(BinaryOp::URShift, lval, rval),
            AssignOp::BitAndAssign => self.eval_binary(BinaryOp::BitAnd, lval, rval),
            AssignOp::BitOrAssign => self.eval_binary(BinaryOp::BitOr, lval, rval),
            AssignOp::BitXorAssign => self.eval_binary(BinaryOp::BitXor, lval, rval),
            _ => rval.clone(),
        }
    }

    fn eval_call(&mut self, callee: &Expression, args: &[Expression], env: &EnvRef) -> Completion {
        // Handle super() calls - call parent constructor with current this
        if matches!(callee, Expression::Super) {
            let super_ctor = env.borrow().get("__super__").unwrap_or(JsValue::Undefined);
            let this_val = env.borrow().get("this").unwrap_or(JsValue::Undefined);
            let mut arg_vals = Vec::new();
            for arg in args {
                match self.eval_expr(arg, env) {
                    Completion::Normal(v) => arg_vals.push(v),
                    other => return other,
                }
            }
            return self.call_function(&super_ctor, &this_val, &arg_vals);
        }

        // Handle member calls: obj.method()
        let (func_val, this_val) = match callee {
            Expression::Member(obj_expr, prop) => {
                let is_super_call = matches!(obj_expr.as_ref(), Expression::Super);
                let obj_val = match self.eval_expr(obj_expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let key = match prop {
                    MemberProperty::Dot(name) => name.clone(),
                    MemberProperty::Computed(expr) => {
                        let v = match self.eval_expr(expr, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        to_js_string(&v)
                    }
                };
                // super.method() - look up on super constructor's prototype, bind this
                if is_super_call {
                    if let JsValue::Object(ref o) = obj_val {
                        if let Some(obj) = self.get_object(o.id) {
                            let proto_val = obj.borrow().get_property("prototype");
                            if let JsValue::Object(ref p) = proto_val {
                                if let Some(proto) = self.get_object(p.id) {
                                    let method = proto.borrow().get_property(&key);
                                    let this_val =
                                        env.borrow().get("this").unwrap_or(JsValue::Undefined);
                                    (method, this_val)
                                } else {
                                    (JsValue::Undefined, JsValue::Undefined)
                                }
                            } else {
                                (JsValue::Undefined, JsValue::Undefined)
                            }
                        } else {
                            (JsValue::Undefined, JsValue::Undefined)
                        }
                    } else {
                        (JsValue::Undefined, JsValue::Undefined)
                    }
                } else if let JsValue::Object(ref o) = obj_val {
                    let oid = o.id;
                    let ov = obj_val.clone();
                    match self.get_object_property(oid, &key, &ov) {
                        Completion::Normal(method) => (method, obj_val),
                        other => return other,
                    }
                } else if let JsValue::String(_) = &obj_val {
                    if let Some(ref sp) = self.string_prototype {
                        let method = sp.borrow().get_property(&key);
                        (method, obj_val)
                    } else {
                        (JsValue::Undefined, obj_val)
                    }
                } else if matches!(&obj_val, JsValue::Number(_)) {
                    let proto = self
                        .number_prototype
                        .clone()
                        .or(self.object_prototype.clone());
                    if let Some(ref p) = proto {
                        let method = p.borrow().get_property(&key);
                        (method, obj_val)
                    } else {
                        (JsValue::Undefined, obj_val)
                    }
                } else if matches!(&obj_val, JsValue::Boolean(_)) {
                    let proto = self
                        .boolean_prototype
                        .clone()
                        .or(self.object_prototype.clone());
                    if let Some(ref p) = proto {
                        let method = p.borrow().get_property(&key);
                        (method, obj_val)
                    } else {
                        (JsValue::Undefined, obj_val)
                    }
                } else if matches!(&obj_val, JsValue::Undefined | JsValue::Null) {
                    let err = self.create_type_error(&format!(
                        "Cannot read properties of {obj_val} (reading '{key}')"
                    ));
                    return Completion::Throw(err);
                } else {
                    (JsValue::Undefined, obj_val)
                }
            }
            _ => {
                let val = match self.eval_expr(callee, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                (val, JsValue::Undefined)
            }
        };

        let mut evaluated_args = Vec::new();
        for arg in args {
            if let Expression::Spread(inner) = arg {
                let val = match self.eval_expr(inner, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if let JsValue::Object(o) = &val {
                    if let Some(obj) = self.get_object(o.id) {
                        if let Some(elems) = obj.borrow().array_elements.clone() {
                            evaluated_args.extend(elems);
                            continue;
                        }
                    }
                }
                evaluated_args.push(val);
            } else {
                let val = match self.eval_expr(arg, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                evaluated_args.push(val);
            }
        }

        self.call_function(&func_val, &this_val, &evaluated_args)
    }

    fn call_function(
        &mut self,
        func_val: &JsValue,
        _this_val: &JsValue,
        args: &[JsValue],
    ) -> Completion {
        if let JsValue::Object(o) = func_val
            && let Some(obj) = self.get_object(o.id)
        {
            let callable = obj.borrow().callable.clone();
            if let Some(func) = callable {
                return match func {
                    JsFunction::Native(_, f) => f(self, _this_val, args),
                    JsFunction::User {
                        params,
                        body,
                        closure,
                        is_arrow,
                        ..
                    } => {
                        let func_env = Environment::new(Some(closure));
                        // Bind parameters
                        for (i, param) in params.iter().enumerate() {
                            if let Pattern::Rest(inner) = param {
                                let rest: Vec<JsValue> = args.get(i..).unwrap_or(&[]).to_vec();
                                let rest_arr = self.create_array(rest);
                                let _ =
                                    self.bind_pattern(inner, rest_arr, BindingKind::Var, &func_env);
                                break;
                            }
                            let val = args.get(i).cloned().unwrap_or(JsValue::Undefined);
                            let _ = self.bind_pattern(param, val, BindingKind::Var, &func_env);
                        }
                        // Bind this (arrow functions inherit from closure)
                        if !is_arrow {
                            func_env.borrow_mut().bindings.insert(
                                "this".to_string(),
                                Binding {
                                    value: _this_val.clone(),
                                    kind: BindingKind::Const,
                                    initialized: true,
                                },
                            );
                        }
                        // Create arguments object
                        let arguments_obj = self.create_arguments_object(args);
                        func_env.borrow_mut().declare("arguments", BindingKind::Var);
                        let _ = func_env.borrow_mut().set("arguments", arguments_obj);
                        let result = self.exec_statements(&body, &func_env);
                        match result {
                            Completion::Return(v) | Completion::Normal(v) => Completion::Normal(v),
                            other => other,
                        }
                    }
                };
            }
        }
        let err = self.create_type_error("is not a function");
        Completion::Throw(err)
    }

    fn eval_new(&mut self, callee: &Expression, args: &[Expression], env: &EnvRef) -> Completion {
        let callee_val = match self.eval_expr(callee, env) {
            Completion::Normal(v) => v,
            other => return other,
        };
        let mut evaluated_args = Vec::new();
        for arg in args {
            if let Expression::Spread(inner) = arg {
                let val = match self.eval_expr(inner, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if let JsValue::Object(o) = &val {
                    if let Some(obj) = self.get_object(o.id) {
                        if let Some(elems) = obj.borrow().array_elements.clone() {
                            evaluated_args.extend(elems);
                            continue;
                        }
                    }
                }
                evaluated_args.push(val);
            } else {
                let val = match self.eval_expr(arg, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                evaluated_args.push(val);
            }
        }
        // Create new object for 'this'
        let new_obj = self.create_object();
        // Set prototype from constructor.prototype if available
        if let JsValue::Object(o) = &callee_val
            && let Some(func_obj) = self.get_object(o.id)
        {
            let proto = func_obj.borrow().get_property_value("prototype");
            if let Some(JsValue::Object(proto_obj)) = proto
                && let Some(proto_rc) = self.get_object(proto_obj.id)
            {
                new_obj.borrow_mut().prototype = Some(proto_rc);
            }
            // Store constructor reference
            new_obj
                .borrow_mut()
                .insert_builtin("constructor".to_string(), callee_val.clone());
        }
        let this_val = JsValue::Object(crate::types::JsObject {
            id: self.objects.len() as u64 - 1,
        });
        let result = self.call_function(&callee_val, &this_val, &evaluated_args);
        match result {
            Completion::Normal(v) => {
                // If constructor returns an object, use it; otherwise return this
                if matches!(v, JsValue::Object(_)) {
                    Completion::Normal(v)
                } else {
                    Completion::Normal(this_val)
                }
            }
            other => other,
        }
    }

    fn get_object_property(&mut self, obj_id: u64, key: &str, this_val: &JsValue) -> Completion {
        let desc = if let Some(obj) = self.get_object(obj_id) {
            obj.borrow().get_property_descriptor(key)
        } else {
            None
        };
        match desc {
            Some(ref d) if d.get.is_some() => {
                let getter = d.get.clone().unwrap();
                self.call_function(&getter, this_val, &[])
            }
            Some(ref d) => Completion::Normal(d.value.clone().unwrap_or(JsValue::Undefined)),
            None => Completion::Normal(JsValue::Undefined),
        }
    }

    fn eval_member(&mut self, obj: &Expression, prop: &MemberProperty, env: &EnvRef) -> Completion {
        let obj_val = match self.eval_expr(obj, env) {
            Completion::Normal(v) => v,
            other => return other,
        };
        let key = match prop {
            MemberProperty::Dot(name) => name.clone(),
            MemberProperty::Computed(expr) => {
                let v = match self.eval_expr(expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                to_js_string(&v)
            }
        };
        match &obj_val {
            JsValue::Object(o) => self.get_object_property(o.id, &key, &obj_val.clone()),
            JsValue::String(s) => {
                if key == "length" {
                    Completion::Normal(JsValue::Number(s.len() as f64))
                } else if let Ok(idx) = key.parse::<usize>() {
                    let ch = s.to_rust_string().chars().nth(idx);
                    match ch {
                        Some(c) => {
                            Completion::Normal(JsValue::String(JsString::from_str(&c.to_string())))
                        }
                        None => Completion::Normal(JsValue::Undefined),
                    }
                } else if let Some(ref sp) = self.string_prototype {
                    Completion::Normal(sp.borrow().get_property(&key))
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            JsValue::Undefined | JsValue::Null => {
                let err = self.create_type_error(&format!(
                    "Cannot read properties of {obj_val} (reading '{key}')"
                ));
                Completion::Throw(err)
            }
            _ => Completion::Normal(JsValue::Undefined),
        }
    }

    fn eval_array_literal(&mut self, elements: &[Option<Expression>], env: &EnvRef) -> Completion {
        let mut values = Vec::new();
        for elem in elements {
            match elem {
                Some(Expression::Spread(inner)) => {
                    let val = match self.eval_expr(inner, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    if let JsValue::Object(o) = &val {
                        if let Some(obj) = self.get_object(o.id) {
                            if let Some(elems) = obj.borrow().array_elements.clone() {
                                values.extend(elems);
                                continue;
                            }
                        }
                    }
                    if let JsValue::String(s) = &val {
                        for ch in s.to_rust_string().chars() {
                            values.push(JsValue::String(JsString::from_str(&ch.to_string())));
                        }
                        continue;
                    }
                    values.push(val);
                }
                Some(expr) => {
                    let val = match self.eval_expr(expr, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    values.push(val);
                }
                None => values.push(JsValue::Undefined),
            }
        }
        Completion::Normal(self.create_array(values))
    }

    fn eval_class(
        &mut self,
        name: &str,
        super_class: &Option<Box<Expression>>,
        body: &[ClassElement],
        env: &EnvRef,
    ) -> Completion {
        // Find constructor method
        let ctor_method = body.iter().find_map(|elem| {
            if let ClassElement::Method(m) = elem {
                if m.kind == ClassMethodKind::Constructor {
                    return Some(m);
                }
            }
            None
        });

        // Evaluate super class if present
        let super_val = if let Some(sc) = super_class {
            match self.eval_expr(sc, env) {
                Completion::Normal(v) => Some(v),
                other => return other,
            }
        } else {
            None
        };

        // Create class environment with __super__ binding
        let class_env = Environment::new(Some(env.clone()));
        if let Some(ref sv) = super_val {
            class_env
                .borrow_mut()
                .declare("__super__", BindingKind::Const);
            let _ = class_env.borrow_mut().set("__super__", sv.clone());
        }

        // Create constructor function
        let ctor_func = if let Some(cm) = ctor_method {
            JsFunction::User {
                name: Some(name.to_string()),
                params: cm.value.params.clone(),
                body: cm.value.body.clone(),
                closure: class_env.clone(),
                is_arrow: false,
            }
        } else if super_val.is_some() {
            JsFunction::User {
                name: Some(name.to_string()),
                params: vec![],
                body: vec![],
                closure: class_env.clone(),
                is_arrow: false,
            }
        } else {
            JsFunction::User {
                name: Some(name.to_string()),
                params: vec![],
                body: vec![],
                closure: class_env.clone(),
                is_arrow: false,
            }
        };

        let ctor_val = self.create_function(ctor_func);

        // Get the prototype object that was auto-created by create_function
        let proto_obj = if let JsValue::Object(ref o) = ctor_val {
            if let Some(func_obj) = self.get_object(o.id) {
                let proto_val = func_obj.borrow().get_property("prototype");
                if let JsValue::Object(ref p) = proto_val {
                    self.get_object(p.id)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        // Set up inheritance
        if let Some(ref sv) = super_val {
            if let JsValue::Object(super_o) = sv {
                if let Some(super_obj) = self.get_object(super_o.id) {
                    let super_proto_val = super_obj.borrow().get_property("prototype");
                    if let JsValue::Object(ref sp) = super_proto_val {
                        if let Some(super_proto) = self.get_object(sp.id) {
                            if let Some(ref proto) = proto_obj {
                                proto.borrow_mut().prototype = Some(super_proto);
                            }
                        }
                    }
                }
            }
        }

        // Add methods and properties to prototype/constructor
        for elem in body {
            match elem {
                ClassElement::Method(m) => {
                    if m.kind == ClassMethodKind::Constructor {
                        continue;
                    }
                    let key = match &m.key {
                        PropertyKey::Identifier(s) | PropertyKey::String(s) => s.clone(),
                        PropertyKey::Number(n) => to_js_string(&JsValue::Number(*n)),
                        PropertyKey::Computed(expr) => match self.eval_expr(expr, env) {
                            Completion::Normal(v) => to_js_string(&v),
                            other => return other,
                        },
                    };
                    let method_func = JsFunction::User {
                        name: Some(key.clone()),
                        params: m.value.params.clone(),
                        body: m.value.body.clone(),
                        closure: class_env.clone(),
                        is_arrow: false,
                    };
                    let method_val = self.create_function(method_func);

                    let target = if m.is_static {
                        if let JsValue::Object(ref o) = ctor_val {
                            self.get_object(o.id)
                        } else {
                            None
                        }
                    } else {
                        proto_obj.clone()
                    };
                    if let Some(ref t) = target {
                        match m.kind {
                            ClassMethodKind::Get => {
                                let mut desc = t.borrow().properties.get(&key).cloned().unwrap_or(
                                    PropertyDescriptor {
                                        value: None,
                                        writable: None,
                                        get: None,
                                        set: None,
                                        enumerable: Some(false),
                                        configurable: Some(true),
                                    },
                                );
                                desc.get = Some(method_val);
                                desc.value = None;
                                desc.writable = None;
                                t.borrow_mut().insert_property(key, desc);
                            }
                            ClassMethodKind::Set => {
                                let mut desc = t.borrow().properties.get(&key).cloned().unwrap_or(
                                    PropertyDescriptor {
                                        value: None,
                                        writable: None,
                                        get: None,
                                        set: None,
                                        enumerable: Some(false),
                                        configurable: Some(true),
                                    },
                                );
                                desc.set = Some(method_val);
                                desc.value = None;
                                desc.writable = None;
                                t.borrow_mut().insert_property(key, desc);
                            }
                            _ => {
                                t.borrow_mut().insert_value(key, method_val);
                            }
                        }
                    }
                }
                ClassElement::Property(p) => {
                    // Instance properties are handled in the constructor
                    // Static properties are set on the constructor
                    if p.is_static {
                        let key = match &p.key {
                            PropertyKey::Identifier(s) | PropertyKey::String(s) => s.clone(),
                            PropertyKey::Number(n) => to_js_string(&JsValue::Number(*n)),
                            PropertyKey::Computed(expr) => match self.eval_expr(expr, env) {
                                Completion::Normal(v) => to_js_string(&v),
                                other => return other,
                            },
                        };
                        let val = if let Some(ref expr) = p.value {
                            match self.eval_expr(expr, env) {
                                Completion::Normal(v) => v,
                                other => return other,
                            }
                        } else {
                            JsValue::Undefined
                        };
                        if let JsValue::Object(ref o) = ctor_val {
                            if let Some(func_obj) = self.get_object(o.id) {
                                func_obj.borrow_mut().insert_value(key, val);
                            }
                        }
                    }
                }
                ClassElement::StaticBlock(_) => {} // TODO
            }
        }

        Completion::Normal(ctor_val)
    }

    fn eval_object_literal(&mut self, props: &[Property], env: &EnvRef) -> Completion {
        let mut obj_data = JsObjectData::new();
        obj_data.prototype = self.object_prototype.clone();
        for prop in props {
            let key = match &prop.key {
                PropertyKey::Identifier(n) => n.clone(),
                PropertyKey::String(s) => s.clone(),
                PropertyKey::Number(n) => number_ops::to_string(*n),
                PropertyKey::Computed(expr) => {
                    let v = match self.eval_expr(expr, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    to_js_string(&v)
                }
            };
            let value = match self.eval_expr(&prop.value, env) {
                Completion::Normal(v) => v,
                other => return other,
            };
            // Handle spread
            if let Expression::Spread(inner) = &prop.value {
                let spread_val = match self.eval_expr(inner, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if let JsValue::Object(ref o) = spread_val
                    && let Some(src) = self.get_object(o.id)
                {
                    let src_ref = src.borrow();
                    for k in &src_ref.property_order {
                        if let Some(v) = src_ref.properties.get(k) {
                            obj_data.insert_property(k.clone(), v.clone());
                        }
                    }
                }
                continue;
            }
            match prop.kind {
                PropertyKind::Get => {
                    let mut desc =
                        obj_data
                            .properties
                            .get(&key)
                            .cloned()
                            .unwrap_or(PropertyDescriptor {
                                value: None,
                                writable: None,
                                get: None,
                                set: None,
                                enumerable: Some(true),
                                configurable: Some(true),
                            });
                    desc.get = Some(value);
                    desc.value = None;
                    desc.writable = None;
                    obj_data.insert_property(key, desc);
                }
                PropertyKind::Set => {
                    let mut desc =
                        obj_data
                            .properties
                            .get(&key)
                            .cloned()
                            .unwrap_or(PropertyDescriptor {
                                value: None,
                                writable: None,
                                get: None,
                                set: None,
                                enumerable: Some(true),
                                configurable: Some(true),
                            });
                    desc.set = Some(value);
                    desc.value = None;
                    desc.writable = None;
                    obj_data.insert_property(key, desc);
                }
                _ => {
                    obj_data.insert_value(key, value);
                }
            }
        }
        let obj = Rc::new(RefCell::new(obj_data));
        self.objects.push(obj);
        Completion::Normal(JsValue::Object(crate::types::JsObject {
            id: self.objects.len() as u64 - 1,
        }))
    }
}

// Type conversion helpers

fn format_radix(mut n: i64, radix: u32) -> String {
    if radix < 2 || radix > 36 {
        return n.to_string();
    }
    if n == 0 {
        return "0".to_string();
    }
    let negative = n < 0;
    if negative {
        n = -n;
    }
    let mut digits = Vec::new();
    while n > 0 {
        let d = (n % radix as i64) as u32;
        digits.push(char::from_digit(d, radix).unwrap_or('?'));
        n /= radix as i64;
    }
    if negative {
        digits.push('-');
    }
    digits.iter().rev().collect()
}

pub fn to_boolean(val: &JsValue) -> bool {
    match val {
        JsValue::Undefined | JsValue::Null => false,
        JsValue::Boolean(b) => *b,
        JsValue::Number(n) => *n != 0.0 && !n.is_nan(),
        JsValue::String(s) => !s.is_empty(),
        JsValue::BigInt(_) => true, // BigInt(0n) is falsy, but simplified
        JsValue::Symbol(_) | JsValue::Object(_) => true,
    }
}

pub fn to_number(val: &JsValue) -> f64 {
    match val {
        JsValue::Undefined => f64::NAN,
        JsValue::Null => 0.0,
        JsValue::Boolean(b) => *b as u8 as f64,
        JsValue::Number(n) => *n,
        JsValue::String(s) => {
            let rust_str = s.to_rust_string();
            let trimmed = rust_str.trim();
            if trimmed.is_empty() {
                return 0.0;
            }
            if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
                return i64::from_str_radix(&trimmed[2..], 16)
                    .map(|n| n as f64)
                    .unwrap_or(f64::NAN);
            }
            if trimmed.starts_with("0o") || trimmed.starts_with("0O") {
                return i64::from_str_radix(&trimmed[2..], 8)
                    .map(|n| n as f64)
                    .unwrap_or(f64::NAN);
            }
            if trimmed.starts_with("0b") || trimmed.starts_with("0B") {
                return i64::from_str_radix(&trimmed[2..], 2)
                    .map(|n| n as f64)
                    .unwrap_or(f64::NAN);
            }
            trimmed.parse::<f64>().unwrap_or(f64::NAN)
        }
        _ => f64::NAN,
    }
}

pub fn to_js_string(val: &JsValue) -> String {
    format!("{val}")
}

fn is_string(val: &JsValue) -> bool {
    matches!(val, JsValue::String(_))
}

fn strict_equality(left: &JsValue, right: &JsValue) -> bool {
    match (left, right) {
        (JsValue::Undefined, JsValue::Undefined) => true,
        (JsValue::Null, JsValue::Null) => true,
        (JsValue::Boolean(a), JsValue::Boolean(b)) => a == b,
        (JsValue::Number(a), JsValue::Number(b)) => number_ops::equal(*a, *b),
        (JsValue::String(a), JsValue::String(b)) => a == b,
        (JsValue::Object(a), JsValue::Object(b)) => a.id == b.id,
        _ => false,
    }
}

fn abstract_equality(left: &JsValue, right: &JsValue) -> bool {
    // Same type
    if std::mem::discriminant(left) == std::mem::discriminant(right) {
        return strict_equality(left, right);
    }
    // null == undefined
    if (left.is_null() && right.is_undefined()) || (left.is_undefined() && right.is_null()) {
        return true;
    }
    // Number vs String
    if left.is_number() && right.is_string() {
        return abstract_equality(left, &JsValue::Number(to_number(right)));
    }
    if left.is_string() && right.is_number() {
        return abstract_equality(&JsValue::Number(to_number(left)), right);
    }
    // Boolean coercion
    if left.is_boolean() {
        return abstract_equality(&JsValue::Number(to_number(left)), right);
    }
    if right.is_boolean() {
        return abstract_equality(left, &JsValue::Number(to_number(right)));
    }
    false
}

fn abstract_relational(left: &JsValue, right: &JsValue) -> Option<bool> {
    if is_string(left) && is_string(right) {
        let ls = to_js_string(left);
        let rs = to_js_string(right);
        return Some(ls < rs);
    }
    let ln = to_number(left);
    let rn = to_number(right);
    number_ops::less_than(ln, rn)
}

fn typeof_val<'a>(val: &JsValue, objects: &[Rc<RefCell<JsObjectData>>]) -> &'a str {
    match val {
        JsValue::Undefined => "undefined",
        JsValue::Null => "object",
        JsValue::Boolean(_) => "boolean",
        JsValue::Number(_) => "number",
        JsValue::String(_) => "string",
        JsValue::Symbol(_) => "symbol",
        JsValue::BigInt(_) => "bigint",
        JsValue::Object(o) => {
            if let Some(obj) = objects.get(o.id as usize)
                && obj.borrow().callable.is_some()
            {
                return "function";
            }
            "object"
        }
    }
}

fn json_stringify_value(interp: &mut Interpreter, val: &JsValue) -> Option<String> {
    match val {
        JsValue::Null => Some("null".to_string()),
        JsValue::Boolean(b) => Some(b.to_string()),
        JsValue::Number(n) => {
            if n.is_nan() || n.is_infinite() {
                Some("null".to_string())
            } else {
                Some(number_ops::to_string(*n))
            }
        }
        JsValue::String(s) => {
            let escaped = s
                .to_rust_string()
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\n', "\\n")
                .replace('\r', "\\r")
                .replace('\t', "\\t");
            Some(format!("\"{}\"", escaped))
        }
        JsValue::Object(o) => {
            if let Some(obj) = interp.get_object(o.id) {
                let is_array = obj.borrow().class_name == "Array";
                if is_array {
                    let len = if let Some(JsValue::Number(n)) =
                        obj.borrow().get_property_value("length")
                    {
                        n as usize
                    } else {
                        0
                    };
                    let mut items = Vec::new();
                    for i in 0..len {
                        let v = obj.borrow().get_property(&i.to_string());
                        match json_stringify_value(interp, &v) {
                            Some(s) => items.push(s),
                            None => items.push("null".to_string()),
                        }
                    }
                    Some(format!("[{}]", items.join(",")))
                } else {
                    if obj.borrow().callable.is_some() {
                        return None;
                    }
                    let keys: Vec<String> = obj.borrow().properties.keys().cloned().collect();
                    let mut entries = Vec::new();
                    for k in &keys {
                        let desc = obj.borrow().properties.get(k).cloned();
                        if let Some(d) = desc {
                            if d.enumerable != Some(true) {
                                continue;
                            }
                            let v = d.value.clone().unwrap_or(JsValue::Undefined);
                            if let Some(sv) = json_stringify_value(interp, &v) {
                                let key_escaped = k.replace('\\', "\\\\").replace('"', "\\\"");
                                entries.push(format!("\"{}\":{}", key_escaped, sv));
                            }
                        }
                    }
                    Some(format!("{{{}}}", entries.join(",")))
                }
            } else {
                Some("null".to_string())
            }
        }
        JsValue::Undefined => None,
        _ => None,
    }
}

fn json_parse_value(interp: &mut Interpreter, s: &str) -> Completion {
    let s = s.trim();
    if s == "null" {
        return Completion::Normal(JsValue::Null);
    }
    if s == "true" {
        return Completion::Normal(JsValue::Boolean(true));
    }
    if s == "false" {
        return Completion::Normal(JsValue::Boolean(false));
    }
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        let inner = &s[1..s.len() - 1];
        let unescaped = inner
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
            .replace("\\n", "\n")
            .replace("\\r", "\r")
            .replace("\\t", "\t");
        return Completion::Normal(JsValue::String(JsString::from_str(&unescaped)));
    }
    if let Ok(n) = s.parse::<f64>() {
        return Completion::Normal(JsValue::Number(n));
    }
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1];
        let items = json_split_items(inner);
        let mut vals = Vec::new();
        for item in &items {
            match json_parse_value(interp, item) {
                Completion::Normal(v) => vals.push(v),
                other => return other,
            }
        }
        return Completion::Normal(interp.create_array(vals));
    }
    if s.starts_with('{') && s.ends_with('}') {
        let inner = &s[1..s.len() - 1];
        let pairs = json_split_items(inner);
        let obj = interp.create_object();
        for pair in &pairs {
            let pair = pair.trim();
            if pair.is_empty() {
                continue;
            }
            if let Some(colon_pos) = find_json_colon(pair) {
                let key_str = pair[..colon_pos].trim();
                let val_str = pair[colon_pos + 1..].trim();
                let key =
                    if key_str.starts_with('"') && key_str.ends_with('"') && key_str.len() >= 2 {
                        key_str[1..key_str.len() - 1].to_string()
                    } else {
                        key_str.to_string()
                    };
                match json_parse_value(interp, val_str) {
                    Completion::Normal(v) => {
                        obj.borrow_mut().insert_value(key, v);
                    }
                    other => return other,
                }
            }
        }
        let id = interp
            .objects
            .iter()
            .position(|o| Rc::ptr_eq(o, &obj))
            .unwrap() as u64;
        return Completion::Normal(JsValue::Object(crate::types::JsObject { id }));
    }
    let err = interp.create_error("SyntaxError", "Unexpected token in JSON");
    Completion::Throw(err)
}

fn json_split_items(s: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    let mut current = String::new();
    for ch in s.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }
        if ch == '\\' && in_string {
            current.push(ch);
            escape = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            current.push(ch);
            continue;
        }
        if in_string {
            current.push(ch);
            continue;
        }
        match ch {
            '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ']' | '}' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    items.push(trimmed);
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        items.push(trimmed);
    }
    items
}

fn find_json_colon(s: &str) -> Option<usize> {
    let mut in_string = false;
    let mut escape = false;
    for (i, ch) in s.chars().enumerate() {
        if escape {
            escape = false;
            continue;
        }
        if ch == '\\' && in_string {
            escape = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if !in_string && ch == ':' {
            return Some(i);
        }
    }
    None
}
