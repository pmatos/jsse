use crate::ast::*;
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
    pub prototype: Option<Rc<RefCell<JsObjectData>>>,
    pub callable: Option<JsFunction>,
    pub array_elements: Option<Vec<JsValue>>,
    pub class_name: String,
    pub extensible: bool,
}

impl JsObjectData {
    fn new() -> Self {
        Self {
            properties: HashMap::new(),
            prototype: None,
            callable: None,
            array_elements: None,
            class_name: "Object".to_string(),
            extensible: true,
        }
    }

    pub fn get_property(&self, key: &str) -> JsValue {
        if let Some(desc) = self.properties.get(key) {
            if let Some(ref val) = desc.value {
                return val.clone();
            }
            return JsValue::Undefined;
        }
        if let Some(proto) = &self.prototype {
            return proto.borrow().get_property(key);
        }
        JsValue::Undefined
    }

    pub fn get_own_property(&self, key: &str) -> Option<&PropertyDescriptor> {
        self.properties.get(key)
    }

    pub fn has_own_property(&self, key: &str) -> bool {
        self.properties.contains_key(key)
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
        self.properties
            .insert(key, PropertyDescriptor::data_default(value));
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
                .insert_value("toString".to_string(), tostring_fn);
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
                Rc::new(|_interp, _this, args| {
                    let val = args
                        .first()
                        .cloned()
                        .unwrap_or(JsValue::String(JsString::from_str("")));
                    Completion::Normal(JsValue::String(JsString::from_str(&to_js_string(&val))))
                }),
            ),
        );

        // Number constructor/converter
        self.register_global_fn(
            "Number",
            BindingKind::Var,
            JsFunction::Native(
                "Number".to_string(),
                Rc::new(|_interp, _this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::Number(0.0));
                    Completion::Normal(JsValue::Number(to_number(&val)))
                }),
            ),
        );

        // Boolean constructor/converter
        self.register_global_fn(
            "Boolean",
            BindingKind::Var,
            JsFunction::Native(
                "Boolean".to_string(),
                Rc::new(|_interp, _this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    Completion::Normal(JsValue::Boolean(to_boolean(&val)))
                }),
            ),
        );

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
            ("sign", f64::signum),
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
            .insert_value("max".to_string(), max_fn);
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
            .insert_value("min".to_string(), min_fn);
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
            .insert_value("pow".to_string(), pow_fn);
        let random_fn = self.create_function(JsFunction::Native(
            "random".to_string(),
            Rc::new(|_interp, _this, _args| {
                Completion::Normal(JsValue::Number(0.5)) // deterministic for testing
            }),
        ));
        math_obj
            .borrow_mut()
            .insert_value("random".to_string(), random_fn);

        let math_val = JsValue::Object(crate::types::JsObject { id: math_id });
        self.global_env
            .borrow_mut()
            .declare("Math", BindingKind::Const);
        let _ = self.global_env.borrow_mut().set("Math", math_val);

        // eval (stub that throws)
        self.register_global_fn(
            "eval",
            BindingKind::Var,
            JsFunction::Native(
                "eval".to_string(),
                Rc::new(|_interp, _this, _args| {
                    Completion::Throw(JsValue::String(JsString::from_str("eval is not supported")))
                }),
            ),
        );

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
                            .insert_value("hasOwnProperty".to_string(), has_own_fn);

                        // Object.prototype.toString
                        let obj_tostring_fn = self.create_function(JsFunction::Native(
                            "toString".to_string(),
                            Rc::new(|interp, this_val, _args| {
                                let tag = if let JsValue::Object(o) = this_val {
                                    if let Some(obj) = interp.get_object(o.id) {
                                        obj.borrow().class_name.clone()
                                    } else {
                                        "Object".to_string()
                                    }
                                } else if this_val.is_undefined() {
                                    "Undefined".to_string()
                                } else if this_val.is_null() {
                                    "Null".to_string()
                                } else {
                                    "Object".to_string()
                                };
                                Completion::Normal(JsValue::String(JsString::from_str(&format!(
                                    "[object {tag}]"
                                ))))
                            }),
                        ));
                        proto_obj
                            .borrow_mut()
                            .insert_value("toString".to_string(), obj_tostring_fn);

                        // Object.prototype.valueOf
                        let obj_valueof_fn = self.create_function(JsFunction::Native(
                            "valueOf".to_string(),
                            Rc::new(|_interp, this_val, _args| {
                                Completion::Normal(this_val.clone())
                            }),
                        ));
                        proto_obj
                            .borrow_mut()
                            .insert_value("valueOf".to_string(), obj_valueof_fn);

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
                            .insert_value("propertyIsEnumerable".to_string(), pie_fn);

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
                            .insert_value("isPrototypeOf".to_string(), ipof_fn);

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
                                let keys: Vec<JsValue> = obj
                                    .borrow()
                                    .properties
                                    .iter()
                                    .filter(|(_, desc)| desc.enumerable != Some(false))
                                    .map(|(k, _)| JsValue::String(JsString::from_str(k)))
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
                                let entries: Vec<JsValue> = obj
                                    .borrow()
                                    .properties
                                    .iter()
                                    .filter(|(_, desc)| desc.enumerable != Some(false))
                                    .map(|(k, desc)| {
                                        let key = JsValue::String(JsString::from_str(k));
                                        let val = desc.value.clone().unwrap_or(JsValue::Undefined);
                                        interp.create_array(vec![key, val])
                                    })
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
                                let values: Vec<JsValue> = obj
                                    .borrow()
                                    .properties
                                    .iter()
                                    .filter(|(_, desc)| desc.enumerable != Some(false))
                                    .map(|(_, desc)| {
                                        desc.value.clone().unwrap_or(JsValue::Undefined)
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
                                        let props: Vec<(String, JsValue)> = src_obj
                                            .borrow()
                                            .properties
                                            .iter()
                                            .filter(|(_, desc)| desc.enumerable != Some(false))
                                            .map(|(k, desc)| {
                                                (
                                                    k.clone(),
                                                    desc.value
                                                        .clone()
                                                        .unwrap_or(JsValue::Undefined),
                                                )
                                            })
                                            .collect();
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
                                    .properties
                                    .keys()
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
        proto.borrow_mut().insert_value("push".to_string(), push_fn);

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
        proto.borrow_mut().insert_value("pop".to_string(), pop_fn);

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
            .insert_value("shift".to_string(), shift_fn);

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
            .insert_value("unshift".to_string(), unshift_fn);

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
            .insert_value("indexOf".to_string(), indexof_fn);

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
            .insert_value("lastIndexOf".to_string(), lastindexof_fn);

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
            .insert_value("includes".to_string(), includes_fn);

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
        proto.borrow_mut().insert_value("join".to_string(), join_fn);

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
            .insert_value("toString".to_string(), tostring_fn);

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
            .insert_value("concat".to_string(), concat_fn);

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
            .insert_value("slice".to_string(), slice_fn);

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
            .insert_value("reverse".to_string(), reverse_fn);

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
            .insert_value("forEach".to_string(), foreach_fn);

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
        proto.borrow_mut().insert_value("map".to_string(), map_fn);

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
            .insert_value("filter".to_string(), filter_fn);

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
            .insert_value("reduce".to_string(), reduce_fn);

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
        proto.borrow_mut().insert_value("some".to_string(), some_fn);

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
            .insert_value("every".to_string(), every_fn);

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
        proto.borrow_mut().insert_value("find".to_string(), find_fn);

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
            .insert_value("findIndex".to_string(), findindex_fn);

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
            .insert_value("splice".to_string(), splice_fn);

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
        proto.borrow_mut().insert_value("fill".to_string(), fill_fn);

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

        // Set Array.isArray on the Array constructor
        if let Some(array_val) = self.global_env.borrow().get("Array") {
            if let JsValue::Object(o) = &array_val {
                if let Some(obj) = self.get_object(o.id) {
                    obj.borrow_mut()
                        .insert_value("isArray".to_string(), is_array_fn);
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
                    let cp = s
                        .chars()
                        .nth(idx)
                        .map(|c| c as u32 as f64)
                        .unwrap_or(f64::NAN);
                    Completion::Normal(JsValue::Number(cp))
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
        ];

        for (name, func) in methods {
            let fn_val = self.create_function(JsFunction::Native(name.to_string(), func));
            proto.borrow_mut().insert_value(name.to_string(), fn_val);
        }

        self.string_prototype = Some(proto);
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
            .insert_value("call".to_string(), call_fn);

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
            .insert_value("apply".to_string(), apply_fn);
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
        let mut obj_data = JsObjectData::new();
        obj_data.prototype = self.object_prototype.clone();
        obj_data.callable = Some(func);
        obj_data.class_name = "Function".to_string();
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
                .insert_value("constructor".to_string(), func_val.clone());
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
            Statement::ClassDeclaration(_) => Completion::Normal(JsValue::Undefined),    // TODO
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
            _ => {
                // TODO: array/object destructuring
                Ok(())
            }
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
                    let comp = self.exec_variable_declaration(decl, &for_env);
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
            let keys: Vec<String> = obj.borrow().properties.keys().cloned().collect();
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
                        if let Some(d) = decl.declarations.first()
                            && let Err(e) = self.bind_pattern(&d.pattern, key_val, kind, &for_env)
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
        // Get iterable values
        let values: Vec<JsValue> = if let JsValue::Object(ref o) = iterable {
            if let Some(obj) = self.get_object(o.id) {
                let obj_ref = obj.borrow();
                if let Some(ref elems) = obj_ref.array_elements {
                    elems.clone()
                } else {
                    // Iterate object values
                    obj_ref
                        .properties
                        .values()
                        .filter_map(|d| d.value.clone())
                        .collect()
                }
            } else {
                return Completion::Normal(JsValue::Undefined);
            }
        } else if let JsValue::String(ref s) = iterable {
            // String iteration: each character
            s.to_rust_string()
                .chars()
                .map(|c| JsValue::String(JsString::from_str(&c.to_string())))
                .collect()
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
                    if let Some(d) = decl.declarations.first()
                        && let Err(e) = self.bind_pattern(&d.pattern, val, kind, &for_env)
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
            Expression::Delete(_) => Completion::Normal(JsValue::Boolean(true)), // TODO
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

    fn eval_literal(&self, lit: &Literal) -> JsValue {
        match lit {
            Literal::Null => JsValue::Null,
            Literal::Boolean(b) => JsValue::Boolean(*b),
            Literal::Number(n) => JsValue::Number(*n),
            Literal::String(s) => JsValue::String(JsString::from_str(s)),
            Literal::BigInt(_) => JsValue::Undefined, // TODO
            Literal::RegExp(_, _) => JsValue::Undefined, // TODO
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

    fn eval_binary(&self, op: BinaryOp, left: &JsValue, right: &JsValue) -> JsValue {
        match op {
            BinaryOp::Add => {
                // String concatenation or numeric addition
                if is_string(left) || is_string(right) {
                    let ls = to_js_string(left);
                    let rs = to_js_string(right);
                    JsValue::String(JsString::from_str(&format!("{ls}{rs}")))
                } else {
                    JsValue::Number(number_ops::add(to_number(left), to_number(right)))
                }
            }
            BinaryOp::Sub => {
                JsValue::Number(number_ops::subtract(to_number(left), to_number(right)))
            }
            BinaryOp::Mul => {
                JsValue::Number(number_ops::multiply(to_number(left), to_number(right)))
            }
            BinaryOp::Div => JsValue::Number(number_ops::divide(to_number(left), to_number(right))),
            BinaryOp::Mod => {
                JsValue::Number(number_ops::remainder(to_number(left), to_number(right)))
            }
            BinaryOp::Exp => {
                JsValue::Number(number_ops::exponentiate(to_number(left), to_number(right)))
            }
            BinaryOp::Eq => JsValue::Boolean(abstract_equality(left, right)),
            BinaryOp::NotEq => JsValue::Boolean(!abstract_equality(left, right)),
            BinaryOp::StrictEq => JsValue::Boolean(strict_equality(left, right)),
            BinaryOp::StrictNotEq => JsValue::Boolean(!strict_equality(left, right)),
            BinaryOp::Lt => JsValue::Boolean(abstract_relational(left, right) == Some(true)),
            BinaryOp::Gt => JsValue::Boolean(abstract_relational(right, left) == Some(true)),
            BinaryOp::LtEq => JsValue::Boolean(abstract_relational(right, left) == Some(false)),
            BinaryOp::GtEq => JsValue::Boolean(abstract_relational(left, right) == Some(false)),
            BinaryOp::LShift => {
                JsValue::Number(number_ops::left_shift(to_number(left), to_number(right)))
            }
            BinaryOp::RShift => JsValue::Number(number_ops::signed_right_shift(
                to_number(left),
                to_number(right),
            )),
            BinaryOp::URShift => JsValue::Number(number_ops::unsigned_right_shift(
                to_number(left),
                to_number(right),
            )),
            BinaryOp::BitAnd => {
                JsValue::Number(number_ops::bitwise_and(to_number(left), to_number(right)))
            }
            BinaryOp::BitOr => {
                JsValue::Number(number_ops::bitwise_or(to_number(left), to_number(right)))
            }
            BinaryOp::BitXor => {
                JsValue::Number(number_ops::bitwise_xor(to_number(left), to_number(right)))
            }
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
                    obj.borrow_mut().insert_value(key, final_val.clone());
                    return Completion::Normal(final_val);
                }
                Completion::Normal(rval)
            }
            _ => Completion::Normal(rval),
        }
    }

    fn apply_compound_assign(&self, op: AssignOp, lval: &JsValue, rval: &JsValue) -> JsValue {
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
        // Handle member calls: obj.method()
        let (func_val, this_val) = match callee {
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
                if let JsValue::Object(ref o) = obj_val {
                    if let Some(obj) = self.get_object(o.id) {
                        let method = obj.borrow().get_property(&key);
                        (method, obj_val)
                    } else {
                        let err = self.create_type_error("Cannot read property of undefined");
                        return Completion::Throw(err);
                    }
                } else if let JsValue::String(_) = &obj_val {
                    if let Some(ref sp) = self.string_prototype {
                        let method = sp.borrow().get_property(&key);
                        (method, obj_val)
                    } else {
                        (JsValue::Undefined, obj_val)
                    }
                } else if matches!(&obj_val, JsValue::Number(_) | JsValue::Boolean(_)) {
                    // Handle toString/valueOf inline for Number/Boolean primitives
                    if key == "toString"
                        || key == "valueOf"
                        || key == "toFixed"
                        || key == "toLocaleString"
                    {
                        // We'll handle these built-in methods directly in call_function
                        // by creating inline native functions
                        let captured_val = obj_val.clone();
                        let method_name = key.clone();
                        let inline_fn = self.create_function(JsFunction::Native(
                            key.clone(),
                            Rc::new(move |_interp, _this, args| match method_name.as_str() {
                                "toString" => {
                                    if let JsValue::Number(n) = &captured_val {
                                        let radix =
                                            args.first().map(|v| to_number(v) as u32).unwrap_or(10);
                                        if radix == 10 {
                                            Completion::Normal(JsValue::String(JsString::from_str(
                                                &to_js_string(&captured_val),
                                            )))
                                        } else {
                                            let i = *n as i64;
                                            let s = format_radix(i, radix);
                                            Completion::Normal(JsValue::String(JsString::from_str(
                                                &s,
                                            )))
                                        }
                                    } else {
                                        Completion::Normal(JsValue::String(JsString::from_str(
                                            &to_js_string(&captured_val),
                                        )))
                                    }
                                }
                                "valueOf" | "toLocaleString" => {
                                    Completion::Normal(captured_val.clone())
                                }
                                "toFixed" => {
                                    if let JsValue::Number(n) = &captured_val {
                                        let digits = args
                                            .first()
                                            .map(|v| to_number(v) as usize)
                                            .unwrap_or(0);
                                        Completion::Normal(JsValue::String(JsString::from_str(
                                            &format!("{n:.digits$}"),
                                        )))
                                    } else {
                                        Completion::Normal(captured_val.clone())
                                    }
                                }
                                _ => Completion::Normal(JsValue::Undefined),
                            }),
                        ));
                        (inline_fn, obj_val)
                    } else if let Some(ref op) = self.object_prototype {
                        let method = op.borrow().get_property(&key);
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
            let val = match self.eval_expr(arg, env) {
                Completion::Normal(v) => v,
                other => return other,
            };
            evaluated_args.push(val);
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
            let val = match self.eval_expr(arg, env) {
                Completion::Normal(v) => v,
                other => return other,
            };
            evaluated_args.push(val);
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
                .insert_value("constructor".to_string(), callee_val.clone());
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
            JsValue::Object(o) => {
                if let Some(obj) = self.get_object(o.id) {
                    Completion::Normal(obj.borrow().get_property(&key))
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
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
                    for (k, v) in &src.borrow().properties {
                        obj_data.properties.insert(k.clone(), v.clone());
                    }
                }
                continue;
            }
            obj_data.insert_value(key, value);
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
