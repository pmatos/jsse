use crate::ast::*;
use crate::parser;
use crate::types::{JsBigInt, JsString, JsValue, bigint_ops, number_ops};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

mod types;
pub use types::*;

mod helpers;
pub(crate) use helpers::*;
mod builtins;
mod eval;
mod exec;
mod gc;
pub(crate) mod generator_analysis;

pub struct Interpreter {
    global_env: EnvRef,
    objects: Vec<Option<Rc<RefCell<JsObjectData>>>>,
    object_prototype: Option<Rc<RefCell<JsObjectData>>>,
    array_prototype: Option<Rc<RefCell<JsObjectData>>>,
    string_prototype: Option<Rc<RefCell<JsObjectData>>>,
    number_prototype: Option<Rc<RefCell<JsObjectData>>>,
    boolean_prototype: Option<Rc<RefCell<JsObjectData>>>,
    regexp_prototype: Option<Rc<RefCell<JsObjectData>>>,
    iterator_prototype: Option<Rc<RefCell<JsObjectData>>>,
    array_iterator_prototype: Option<Rc<RefCell<JsObjectData>>>,
    string_iterator_prototype: Option<Rc<RefCell<JsObjectData>>>,
    map_prototype: Option<Rc<RefCell<JsObjectData>>>,
    map_iterator_prototype: Option<Rc<RefCell<JsObjectData>>>,
    set_prototype: Option<Rc<RefCell<JsObjectData>>>,
    set_iterator_prototype: Option<Rc<RefCell<JsObjectData>>>,
    weakmap_prototype: Option<Rc<RefCell<JsObjectData>>>,
    weakset_prototype: Option<Rc<RefCell<JsObjectData>>>,
    weakref_prototype: Option<Rc<RefCell<JsObjectData>>>,
    finalization_registry_prototype: Option<Rc<RefCell<JsObjectData>>>,
    date_prototype: Option<Rc<RefCell<JsObjectData>>>,
    generator_prototype: Option<Rc<RefCell<JsObjectData>>>,
    async_iterator_prototype: Option<Rc<RefCell<JsObjectData>>>,
    async_generator_prototype: Option<Rc<RefCell<JsObjectData>>>,
    async_generator_function_prototype: Option<Rc<RefCell<JsObjectData>>>,
    bigint_prototype: Option<Rc<RefCell<JsObjectData>>>,
    symbol_prototype: Option<Rc<RefCell<JsObjectData>>>,
    arraybuffer_prototype: Option<Rc<RefCell<JsObjectData>>>,
    typed_array_prototype: Option<Rc<RefCell<JsObjectData>>>,
    typed_array_constructor: Option<JsValue>,
    int8array_prototype: Option<Rc<RefCell<JsObjectData>>>,
    uint8array_prototype: Option<Rc<RefCell<JsObjectData>>>,
    uint8clampedarray_prototype: Option<Rc<RefCell<JsObjectData>>>,
    int16array_prototype: Option<Rc<RefCell<JsObjectData>>>,
    uint16array_prototype: Option<Rc<RefCell<JsObjectData>>>,
    int32array_prototype: Option<Rc<RefCell<JsObjectData>>>,
    uint32array_prototype: Option<Rc<RefCell<JsObjectData>>>,
    float32array_prototype: Option<Rc<RefCell<JsObjectData>>>,
    float64array_prototype: Option<Rc<RefCell<JsObjectData>>>,
    bigint64array_prototype: Option<Rc<RefCell<JsObjectData>>>,
    biguint64array_prototype: Option<Rc<RefCell<JsObjectData>>>,
    dataview_prototype: Option<Rc<RefCell<JsObjectData>>>,
    promise_prototype: Option<Rc<RefCell<JsObjectData>>>,
    pub(crate) aggregate_error_prototype: Option<Rc<RefCell<JsObjectData>>>,
    global_symbol_registry: HashMap<String, crate::types::JsSymbol>,
    next_symbol_id: u64,
    new_target: Option<JsValue>,
    free_list: Vec<usize>,
    gc_alloc_count: usize,
    generator_context: Option<GeneratorContext>,
    microtask_queue: Vec<Box<dyn FnOnce(&mut Interpreter) -> Completion>>,
    cached_has_instance_key: Option<String>,
    template_cache: HashMap<usize, u64>,
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
            iterator_prototype: None,
            array_iterator_prototype: None,
            string_iterator_prototype: None,
            map_prototype: None,
            map_iterator_prototype: None,
            set_prototype: None,
            set_iterator_prototype: None,
            weakmap_prototype: None,
            weakset_prototype: None,
            weakref_prototype: None,
            finalization_registry_prototype: None,
            date_prototype: None,
            generator_prototype: None,
            async_iterator_prototype: None,
            async_generator_prototype: None,
            async_generator_function_prototype: None,
            bigint_prototype: None,
            symbol_prototype: None,
            arraybuffer_prototype: None,
            typed_array_prototype: None,
            typed_array_constructor: None,
            int8array_prototype: None,
            uint8array_prototype: None,
            uint8clampedarray_prototype: None,
            int16array_prototype: None,
            uint16array_prototype: None,
            int32array_prototype: None,
            uint32array_prototype: None,
            float32array_prototype: None,
            float64array_prototype: None,
            bigint64array_prototype: None,
            biguint64array_prototype: None,
            dataview_prototype: None,
            promise_prototype: None,
            aggregate_error_prototype: None,
            global_symbol_registry: HashMap::new(),
            next_symbol_id: 1,
            new_target: None,
            free_list: Vec::new(),
            gc_alloc_count: 0,
            generator_context: None,
            microtask_queue: Vec::new(),
            cached_has_instance_key: None,
            template_cache: HashMap::new(),
        };
        interp.setup_globals();
        interp
    }

    fn register_global_fn(&mut self, name: &str, kind: BindingKind, func: JsFunction) {
        let val = self.create_function(func);
        self.global_env.borrow_mut().declare(name, kind);
        let _ = self.global_env.borrow_mut().set(name, val);
    }

    fn to_property_descriptor(
        &mut self,
        val: &JsValue,
    ) -> Result<PropertyDescriptor, Option<JsValue>> {
        if let JsValue::Object(d) = val {
            let obj_id = d.id;
            let mut desc = PropertyDescriptor {
                value: None,
                writable: None,
                get: None,
                set: None,
                enumerable: None,
                configurable: None,
            };

            macro_rules! check_field {
                ($field:expr, $key:expr, $assign:expr) => {
                    let has = self
                        .get_object(obj_id)
                        .map(|o| o.borrow().has_property($key))
                        .unwrap_or(false);
                    if has {
                        match self.get_object_property(obj_id, $key, val) {
                            Completion::Normal(v) => {
                                $assign(v);
                            }
                            Completion::Throw(e) => return Err(Some(e)),
                            _ => {}
                        }
                    }
                };
            }

            check_field!(desc, "value", |v: JsValue| desc.value = Some(v));
            check_field!(desc, "writable", |v: JsValue| desc.writable =
                Some(to_boolean(&v)));
            check_field!(desc, "enumerable", |v: JsValue| desc.enumerable =
                Some(to_boolean(&v)));
            check_field!(desc, "configurable", |v: JsValue| desc.configurable =
                Some(to_boolean(&v)));
            check_field!(desc, "get", |v: JsValue| desc.get = Some(v));
            check_field!(desc, "set", |v: JsValue| desc.set = Some(v));

            // Validate: get must be callable or undefined
            if let Some(ref getter) = desc.get
                && !matches!(getter, JsValue::Undefined)
            {
                let is_callable = if let JsValue::Object(o) = getter
                    && let Some(obj) = self.get_object(o.id)
                {
                    obj.borrow().callable.is_some()
                } else {
                    false
                };
                if !is_callable {
                    return Err(Some(self.create_type_error("Getter must be a function")));
                }
            }
            if let Some(ref setter) = desc.set
                && !matches!(setter, JsValue::Undefined)
            {
                let is_callable = if let JsValue::Object(o) = setter
                    && let Some(obj) = self.get_object(o.id)
                {
                    obj.borrow().callable.is_some()
                } else {
                    false
                };
                if !is_callable {
                    return Err(Some(self.create_type_error("Setter must be a function")));
                }
            }

            // Cannot have both accessor and data descriptor fields
            if desc.is_accessor_descriptor() && desc.is_data_descriptor() {
                return Err(Some(self.create_type_error(
                    "Invalid property descriptor. Cannot both specify accessors and a value or writable attribute",
                )));
            }

            Ok(desc)
        } else {
            Err(Some(self.create_type_error(
                "Property description must be an object",
            )))
        }
    }

    fn from_property_descriptor(&mut self, desc: &PropertyDescriptor) -> JsValue {
        let result = self.create_object();
        {
            let mut r = result.borrow_mut();
            if let Some(ref val) = desc.value {
                r.insert_value("value".to_string(), val.clone());
            }
            if let Some(w) = desc.writable {
                r.insert_value("writable".to_string(), JsValue::Boolean(w));
            }
            if let Some(e) = desc.enumerable {
                r.insert_value("enumerable".to_string(), JsValue::Boolean(e));
            }
            if let Some(c) = desc.configurable {
                r.insert_value("configurable".to_string(), JsValue::Boolean(c));
            }
            if let Some(ref g) = desc.get {
                r.insert_value("get".to_string(), g.clone());
            }
            if let Some(ref s) = desc.set {
                r.insert_value("set".to_string(), s.clone());
            }
        }
        let id = result.borrow().id.unwrap();
        JsValue::Object(crate::types::JsObject { id })
    }

    fn create_object(&mut self) -> Rc<RefCell<JsObjectData>> {
        let mut data = JsObjectData::new();
        data.prototype = self.object_prototype.clone();
        let obj = Rc::new(RefCell::new(data));
        self.allocate_object_slot(obj.clone());
        obj
    }

    fn create_thrower_function(&mut self) -> JsValue {
        let func = JsFunction::native(
            "%ThrowTypeError%".to_string(),
            0,
            |interp: &mut Interpreter, _this: &JsValue, _args: &[JsValue]| {
                let err = interp.create_type_error(
                    "'caller', 'callee', and 'arguments' properties may not be accessed on strict mode functions or the arguments objects for calls to them",
                );
                Completion::Throw(err)
            },
        );
        self.create_function(func)
    }

    fn is_strict_mode_body(body: &[Statement]) -> bool {
        for stmt in body {
            if let Statement::Expression(Expression::Literal(Literal::String(s))) = stmt {
                if s == "use strict" {
                    return true;
                }
            } else {
                break;
            }
        }
        false
    }

    fn create_function(&mut self, func: JsFunction) -> JsValue {
        let is_gen = matches!(
            &func,
            JsFunction::User {
                is_generator: true,
                ..
            }
        );
        let is_async_gen = matches!(
            &func,
            JsFunction::User {
                is_generator: true,
                is_async: true,
                ..
            }
        );
        let (fn_name, fn_length) = match &func {
            JsFunction::User { name, params, .. } => {
                let n = name.clone().unwrap_or_default();
                let len = params
                    .iter()
                    .filter(|p| !matches!(p, Pattern::Rest(_)))
                    .count();
                (n, len)
            }
            JsFunction::Native(name, arity, _, _) => (name.clone(), *arity),
        };
        let mut obj_data = JsObjectData::new();
        obj_data.prototype = if is_async_gen {
            self.async_generator_function_prototype
                .clone()
                .or(self.object_prototype.clone())
        } else {
            self.object_prototype.clone()
        };
        obj_data.callable = Some(func);
        obj_data.class_name = if is_async_gen {
            "AsyncGeneratorFunction".to_string()
        } else if is_gen {
            "GeneratorFunction".to_string()
        } else {
            "Function".to_string()
        };
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
        let is_constructable = match &obj_data.callable {
            Some(JsFunction::User { is_arrow, .. }) => !is_arrow,
            Some(JsFunction::Native(_, _, _, is_ctor)) => *is_ctor,
            None => false,
        };
        if is_constructable {
            let proto = self.create_object();
            if is_async_gen {
                proto.borrow_mut().prototype = self.async_generator_prototype.clone();
            } else if is_gen {
                proto.borrow_mut().prototype = self.generator_prototype.clone();
            }
            let proto_id = proto.borrow().id.unwrap();
            let proto_val = JsValue::Object(crate::types::JsObject { id: proto_id });
            obj_data.insert_value("prototype".to_string(), proto_val.clone());
        }
        let obj = Rc::new(RefCell::new(obj_data));
        let func_id = self.allocate_object_slot(obj.clone());
        let func_val = JsValue::Object(crate::types::JsObject { id: func_id });
        // Set prototype.constructor = func (not for generators)
        if is_constructable
            && !is_gen
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
        self.objects.get(id as usize).and_then(|slot| slot.clone())
    }

    pub(crate) fn set_function_name(&self, val: &JsValue, name: &str) {
        if let JsValue::Object(o) = val {
            if let Some(obj) = self.get_object(o.id) {
                let obj_ref = obj.borrow();
                if obj_ref.callable.is_none() {
                    return;
                }
                if let Some(prop) = obj_ref.properties.get("name") {
                    if let Some(ref v) = prop.value {
                        if let JsValue::String(s) = v {
                            if !s.to_string().is_empty() {
                                return;
                            }
                        } else {
                            return;
                        }
                    }
                }
                drop(obj_ref);
                obj.borrow_mut().insert_property(
                    "name".to_string(),
                    PropertyDescriptor::data(
                        JsValue::String(JsString::from_str(name)),
                        false,
                        false,
                        true,
                    ),
                );
            }
        }
    }

    fn create_arguments_object(
        &mut self,
        args: &[JsValue],
        callee: JsValue,
        is_strict: bool,
        func_env: Option<&EnvRef>,
        param_names: &[String],
    ) -> JsValue {
        let obj = self.create_object();
        {
            let mut o = obj.borrow_mut();
            o.class_name = "Arguments".to_string();

            // length property: writable, not enumerable, configurable
            o.define_own_property(
                "length".to_string(),
                PropertyDescriptor {
                    value: Some(JsValue::Number(args.len() as f64)),
                    writable: Some(true),
                    enumerable: Some(false),
                    configurable: Some(true),
                    get: None,
                    set: None,
                },
            );

            // Index properties: writable, enumerable, configurable
            for (i, val) in args.iter().enumerate() {
                o.define_own_property(
                    i.to_string(),
                    PropertyDescriptor {
                        value: Some(val.clone()),
                        writable: Some(true),
                        enumerable: Some(true),
                        configurable: Some(true),
                        get: None,
                        set: None,
                    },
                );
            }

            if !is_strict {
                // Non-strict: callee is a writable, non-enumerable, configurable data property
                o.define_own_property(
                    "callee".to_string(),
                    PropertyDescriptor {
                        value: Some(callee),
                        writable: Some(true),
                        enumerable: Some(false),
                        configurable: Some(true),
                        get: None,
                        set: None,
                    },
                );

                if let Some(env) = func_env {
                    let mut map = HashMap::new();
                    for (i, name) in param_names.iter().enumerate() {
                        map.insert(i.to_string(), (env.clone(), name.clone()));
                    }
                    if !map.is_empty() {
                        o.parameter_map = Some(map);
                    }
                }
            }
        }

        let result_id = obj.borrow().id.unwrap();
        let result = JsValue::Object(crate::types::JsObject { id: result_id });

        if is_strict {
            // Strict: callee is an accessor that throws TypeError on get/set
            let thrower = self.create_thrower_function();
            if let JsValue::Object(ref o) = result
                && let Some(obj_rc) = self.get_object(o.id)
            {
                obj_rc.borrow_mut().define_own_property(
                    "callee".to_string(),
                    PropertyDescriptor {
                        value: None,
                        writable: None,
                        get: Some(thrower.clone()),
                        set: Some(thrower),
                        enumerable: Some(false),
                        configurable: Some(false),
                    },
                );
            }
        }

        // Add Symbol.iterator (Array.prototype[@@iterator]) to both strict and non-strict
        if let Some(key) = self.get_symbol_iterator_key() {
            let iter_fn = self.create_function(JsFunction::native(
                "[Symbol.iterator]".to_string(),
                0,
                |interp, this_val, _args| {
                    if let JsValue::Object(o) = this_val {
                        return Completion::Normal(
                            interp.create_array_iterator(o.id, IteratorKind::Value),
                        );
                    }
                    let err = interp.create_type_error("Symbol.iterator called on non-object");
                    Completion::Throw(err)
                },
            ));
            if let JsValue::Object(ref o) = result
                && let Some(obj_rc) = self.get_object(o.id)
            {
                obj_rc
                    .borrow_mut()
                    .insert_property(key, PropertyDescriptor::data(iter_fn, true, false, true));
            }
        }

        result
    }

    pub fn run(&mut self, program: &Program) -> Completion {
        self.maybe_gc();
        let result = self.exec_statements(&program.body, &self.global_env.clone());
        self.drain_microtasks();
        result
    }

    pub(crate) fn drain_microtasks(&mut self) {
        while !self.microtask_queue.is_empty() {
            let job = self.microtask_queue.remove(0);
            let _ = job(self);
        }
    }

    pub fn format_value(&self, val: &JsValue) -> String {
        match val {
            JsValue::Object(o) => {
                if let Some(obj) = self.get_object(o.id) {
                    let obj = obj.borrow();
                    let name = obj.get_property("name");
                    let message = obj.get_property("message");
                    if let JsValue::String(ref msg) = message {
                        let msg_str = msg.to_rust_string();
                        if let JsValue::String(ref n) = name {
                            let n_str = n.to_rust_string();
                            if n_str.is_empty() {
                                return msg_str;
                            }
                            return format!("{n_str}: {msg_str}");
                        }
                        return msg_str;
                    }
                }
                format!("{val}")
            }
            _ => format!("{val}"),
        }
    }
}
