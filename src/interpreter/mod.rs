use crate::ast::*;
use crate::parser;
use crate::types::{JsBigInt, JsString, JsValue, bigint_ops, number_ops};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

mod types;
pub use types::*;

mod helpers;
pub(crate) use helpers::*;
mod builtins;
pub(crate) use builtins::regexp::validate_js_pattern;
mod eval;
mod exec;
mod gc;
pub(crate) mod generator_analysis;
pub(crate) mod generator_transform;

#[allow(clippy::type_complexity)]
pub struct Interpreter {
    pub(crate) realms: Vec<Realm>,
    pub(crate) current_realm_id: usize,
    objects: Vec<Option<Rc<RefCell<JsObjectData>>>>,
    global_symbol_registry: HashMap<String, crate::types::JsSymbol>,
    pub(crate) well_known_symbols: HashMap<String, crate::types::JsSymbol>,
    next_symbol_id: u64,
    new_target: Option<JsValue>,
    free_list: Vec<usize>,
    gc_alloc_count: usize,
    generator_context: Option<GeneratorContext>,
    pub(crate) destructuring_yield: bool,
    pub(crate) pending_iter_close: Vec<JsValue>,
    pub(crate) generator_inline_iters: HashMap<u64, Vec<JsValue>>,
    microtask_queue: Vec<Box<dyn FnOnce(&mut Interpreter) -> Completion>>,
    cached_has_instance_key: Option<String>,
    module_registry: HashMap<PathBuf, Rc<RefCell<LoadedModule>>>,
    current_module_path: Option<PathBuf>,
    last_call_had_explicit_return: bool,
    last_call_this_value: Option<JsValue>,
    constructing_derived: bool,
    pub(crate) call_stack_envs: Vec<EnvRef>,
    pub(crate) gc_temp_roots: Vec<u64>,
    pub(crate) microtask_roots: Vec<JsValue>,
    pub(crate) class_private_names: Vec<std::collections::HashMap<String, String>>,
    next_class_brand_id: u64,
    pub(crate) regexp_legacy_input: String,
    pub(crate) regexp_legacy_last_match: String,
    pub(crate) regexp_legacy_last_paren: String,
    pub(crate) regexp_legacy_left_context: String,
    pub(crate) regexp_legacy_right_context: String,
    pub(crate) regexp_legacy_parens: [String; 9],
    pub(crate) regexp_constructor_id: Option<u64>,
    pub(crate) function_realm_map: HashMap<u64, usize>,
}

pub struct LoadedModule {
    pub path: PathBuf,
    pub env: EnvRef,
    pub exports: HashMap<String, JsValue>,
    pub export_bindings: HashMap<String, String>, // export_name -> binding_name
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
                        deletable: false,
                    },
                );
            }
        }

        let realm = Realm::new(0, global);

        let mut interp = Self {
            realms: vec![realm],
            current_realm_id: 0,
            objects: Vec::new(),
            global_symbol_registry: HashMap::new(),
            well_known_symbols: HashMap::new(),
            next_symbol_id: 1,
            new_target: None,
            free_list: Vec::new(),
            gc_alloc_count: 0,
            generator_context: None,
            destructuring_yield: false,
            pending_iter_close: Vec::new(),
            generator_inline_iters: HashMap::new(),
            microtask_queue: Vec::new(),
            cached_has_instance_key: None,
            module_registry: HashMap::new(),
            current_module_path: None,
            last_call_had_explicit_return: false,
            last_call_this_value: None,
            constructing_derived: false,
            call_stack_envs: Vec::new(),
            gc_temp_roots: Vec::new(),
            microtask_roots: Vec::new(),
            class_private_names: Vec::new(),
            next_class_brand_id: 0,
            regexp_legacy_input: String::new(),
            regexp_legacy_last_match: String::new(),
            regexp_legacy_last_paren: String::new(),
            regexp_legacy_left_context: String::new(),
            regexp_legacy_right_context: String::new(),
            regexp_legacy_parens: Default::default(),
            regexp_constructor_id: None,
            function_realm_map: HashMap::new(),
        };
        interp.setup_globals();
        interp
    }

    #[inline(always)]
    pub(crate) fn realm(&self) -> &Realm {
        &self.realms[self.current_realm_id]
    }

    #[inline(always)]
    pub(crate) fn realm_mut(&mut self) -> &mut Realm {
        &mut self.realms[self.current_realm_id]
    }

    pub(crate) fn create_new_realm(&mut self) -> usize {
        let new_id = self.realms.len();
        let new_global_env = Environment::new(None);
        {
            let mut env = new_global_env.borrow_mut();
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
                        deletable: false,
                    },
                );
            }
        }
        let realm = Realm::new(new_id, new_global_env);
        self.realms.push(realm);

        let old_realm = self.current_realm_id;
        self.current_realm_id = new_id;
        self.setup_globals();
        self.current_realm_id = old_realm;
        new_id
    }

    pub(crate) fn create_dollar_262(&mut self, realm_id: usize) -> JsValue {
        let dollar_262 = self.create_object();
        let dollar_262_id = dollar_262.borrow().id.unwrap();

        // $262.detachArrayBuffer
        let detach_fn = self.create_function(JsFunction::native(
            "detachArrayBuffer".to_string(),
            1,
            |interp, _this, args| {
                let buf = args.first().cloned().unwrap_or(JsValue::Undefined);
                interp.detach_arraybuffer(&buf)
            },
        ));
        dollar_262
            .borrow_mut()
            .insert_builtin("detachArrayBuffer".to_string(), detach_fn);

        // $262.gc
        let gc_fn = self.create_function(JsFunction::native(
            "gc".to_string(),
            0,
            |interp, _this, _args| {
                interp.maybe_gc();
                Completion::Normal(JsValue::Undefined)
            },
        ));
        dollar_262
            .borrow_mut()
            .insert_builtin("gc".to_string(), gc_fn);

        // $262.global — reference to the realm's global object
        let global_env = self.realms[realm_id].global_env.clone();
        let global_obj = global_env.borrow().global_object.clone();
        if let Some(ref go) = global_obj {
            let go_id = go.borrow().id.unwrap();
            dollar_262.borrow_mut().insert_builtin(
                "global".to_string(),
                JsValue::Object(crate::types::JsObject { id: go_id }),
            );
        }

        // $262.createRealm
        let create_realm_fn = self.create_function(JsFunction::native(
            "createRealm".to_string(),
            0,
            |interp, _this, _args| {
                let new_realm_id = interp.create_new_realm();
                let new_dollar_262 = interp.create_dollar_262(new_realm_id);
                Completion::Normal(new_dollar_262)
            },
        ));
        dollar_262
            .borrow_mut()
            .insert_builtin("createRealm".to_string(), create_realm_fn);

        // $262.evalScript — parse and execute code in this $262's realm
        let eval_realm_id = realm_id;
        let eval_script_fn = self.create_function(JsFunction::native(
            "evalScript".to_string(),
            1,
            move |interp, _this, args| {
                let code_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let code = if let JsValue::String(ref s) = code_val {
                    crate::interpreter::builtins::regexp::js_string_to_regex_input(&s.code_units)
                } else {
                    match interp.to_string_value(&code_val) {
                        Ok(s) => s,
                        Err(e) => return Completion::Throw(e),
                    }
                };
                let mut p = match crate::parser::Parser::new(&code) {
                    Ok(p) => p,
                    Err(_) => {
                        return Completion::Throw(
                            interp.create_error("SyntaxError", "Invalid eval source"),
                        );
                    }
                };
                let program = match p.parse_program() {
                    Ok(prog) => prog,
                    Err(e) => {
                        return Completion::Throw(
                            interp.create_error("SyntaxError", &format!("{}", e)),
                        );
                    }
                };
                let old_realm = interp.current_realm_id;
                interp.current_realm_id = eval_realm_id;
                let result = interp.run(&program);
                interp.current_realm_id = old_realm;
                match result {
                    Completion::Normal(v) => Completion::Normal(v),
                    Completion::Empty => Completion::Normal(JsValue::Undefined),
                    other => other,
                }
            },
        ));
        dollar_262
            .borrow_mut()
            .insert_builtin("evalScript".to_string(), eval_script_fn);

        // $262.IsHTMLDDA — B.3.6 [[IsHTMLDDA]] internal slot
        let htmldda_obj = self.create_object();
        htmldda_obj.borrow_mut().callable = Some(JsFunction::native(
            "".to_string(),
            0,
            |_interp, _this, _args| Completion::Normal(JsValue::Null),
        ));
        htmldda_obj.borrow_mut().is_htmldda = true;
        let htmldda_val = JsValue::Object(crate::types::JsObject {
            id: htmldda_obj.borrow().id.unwrap(),
        });
        dollar_262
            .borrow_mut()
            .insert_builtin("IsHTMLDDA".to_string(), htmldda_val);

        JsValue::Object(crate::types::JsObject { id: dollar_262_id })
    }

    pub(crate) fn gc_root_value(&mut self, val: &JsValue) {
        if let JsValue::Object(o) = val {
            self.gc_temp_roots.push(o.id);
        }
    }

    pub(crate) fn gc_unroot_value(&mut self, val: &JsValue) {
        if let JsValue::Object(o) = val
            && let Some(pos) = self.gc_temp_roots.iter().rposition(|&id| id == o.id)
        {
            self.gc_temp_roots.remove(pos);
        }
    }

    pub(crate) fn can_be_held_weakly(&self, val: &JsValue) -> bool {
        match val {
            JsValue::Object(_) => true,
            JsValue::Symbol(sym) => !self
                .global_symbol_registry
                .values()
                .any(|reg| reg.id == sym.id),
            _ => false,
        }
    }

    // GetFunctionRealm — §10.2.4
    pub(crate) fn get_function_realm(&self, func_val: &JsValue) -> usize {
        if let JsValue::Object(o) = func_val
            && let Some(obj) = self.get_object(o.id)
        {
            let obj_ref = obj.borrow();
            // Bound function: recurse on [[BoundTargetFunction]]
            if let Some(ref target) = obj_ref.bound_target_function {
                let target_clone = target.clone();
                drop(obj_ref);
                return self.get_function_realm(&target_clone);
            }
            // Proxy: recurse on [[ProxyTarget]]
            if let Some(ref target) = obj_ref.proxy_target
                && !obj_ref.proxy_revoked
            {
                let target_id = target.borrow().id.unwrap();
                drop(obj_ref);
                return self.get_function_realm(&JsValue::Object(crate::types::JsObject { id: target_id }));
            }
            drop(obj_ref);
            // Check function_realm_map
            if let Some(&realm_id) = self.function_realm_map.get(&o.id) {
                return realm_id;
            }
        }
        self.current_realm_id
    }

    // GetPrototypeFromConstructor — §10.2.4
    pub(crate) fn get_prototype_from_new_target(
        &mut self,
        default_proto: &Option<Rc<RefCell<JsObjectData>>>,
    ) -> Result<Option<Rc<RefCell<JsObjectData>>>, JsValue> {
        let nt = match self.new_target.clone() {
            Some(v) => v,
            None => return Ok(default_proto.clone()),
        };
        if let JsValue::Object(nt_o) = &nt {
            let proto_val = match self.get_object_property(nt_o.id, "prototype", &nt) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::Undefined,
            };
            if let JsValue::Object(po) = proto_val
                && let Some(proto_rc) = self.get_object(po.id)
            {
                return Ok(Some(proto_rc));
            }
        }
        Ok(default_proto.clone())
    }

    // GetPrototypeFromConstructor with realm-aware fallback — §10.2.4
    pub(crate) fn get_prototype_from_new_target_realm<F>(
        &mut self,
        get_realm_proto: F,
    ) -> Result<Option<Rc<RefCell<JsObjectData>>>, JsValue>
    where
        F: Fn(&Realm) -> Option<Rc<RefCell<JsObjectData>>>,
    {
        let nt = match self.new_target.clone() {
            Some(v) => v,
            None => {
                let proto = get_realm_proto(&self.realms[self.current_realm_id]);
                return Ok(proto);
            }
        };
        if let JsValue::Object(nt_o) = &nt {
            let proto_val = match self.get_object_property(nt_o.id, "prototype", &nt) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::Undefined,
            };
            if let JsValue::Object(po) = proto_val
                && let Some(proto_rc) = self.get_object(po.id)
            {
                return Ok(Some(proto_rc));
            }
            // proto is not an object: use realm of newTarget
            let nt_realm_id = self.get_function_realm(&JsValue::Object(nt_o.clone()));
            let proto = get_realm_proto(&self.realms[nt_realm_id]);
            return Ok(proto);
        }
        let proto = get_realm_proto(&self.realms[self.current_realm_id]);
        Ok(proto)
    }

    fn register_global_fn(&mut self, name: &str, kind: BindingKind, func: JsFunction) {
        let val = self.create_function(func);
        let global_env = self.realm().global_env.clone();
        global_env.borrow_mut().declare(name, kind);
        let _ = global_env.borrow_mut().set(name, val);
    }

    #[allow(clippy::wrong_self_convention)]
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
                Some(self.to_boolean_val(&v)));
            check_field!(desc, "enumerable", |v: JsValue| desc.enumerable =
                Some(self.to_boolean_val(&v)));
            check_field!(desc, "configurable", |v: JsValue| desc.configurable =
                Some(self.to_boolean_val(&v)));
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

    #[allow(clippy::wrong_self_convention)]
    fn from_property_descriptor(&mut self, desc: &PropertyDescriptor) -> JsValue {
        let result = self.create_object();
        let is_accessor = desc.get.is_some() || desc.set.is_some();
        {
            let mut r = result.borrow_mut();
            if is_accessor {
                r.insert_value(
                    "get".to_string(),
                    desc.get.clone().unwrap_or(JsValue::Undefined),
                );
                r.insert_value(
                    "set".to_string(),
                    desc.set.clone().unwrap_or(JsValue::Undefined),
                );
            } else {
                if let Some(ref val) = desc.value {
                    r.insert_value("value".to_string(), val.clone());
                }
                if let Some(w) = desc.writable {
                    r.insert_value("writable".to_string(), JsValue::Boolean(w));
                }
            }
            if let Some(e) = desc.enumerable {
                r.insert_value("enumerable".to_string(), JsValue::Boolean(e));
            }
            if let Some(c) = desc.configurable {
                r.insert_value("configurable".to_string(), JsValue::Boolean(c));
            }
        }
        let id = result.borrow().id.unwrap();
        JsValue::Object(crate::types::JsObject { id })
    }

    pub(crate) fn to_boolean_val(&self, val: &JsValue) -> bool {
        if let JsValue::Object(o) = val {
            if let Some(Some(obj)) = self.objects.get(o.id as usize) {
                if obj.borrow().is_htmldda {
                    return false;
                }
            }
        }
        to_boolean(val)
    }

    fn create_object(&mut self) -> Rc<RefCell<JsObjectData>> {
        let mut data = JsObjectData::new();
        data.prototype = self.realm().object_prototype.clone();
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
                if utf16_eq(s, "use strict") {
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
        let is_async_non_gen = matches!(
            &func,
            JsFunction::User {
                is_generator: false,
                is_async: true,
                ..
            }
        );
        let (fn_name, fn_length) = match &func {
            JsFunction::User { name, params, .. } => {
                let n = name.clone().unwrap_or_default();
                let len = params
                    .iter()
                    .take_while(|p| !matches!(p, Pattern::Assign(..) | Pattern::Rest(_)))
                    .count();
                (n, len)
            }
            JsFunction::Native(name, arity, _, _) => (name.clone(), *arity),
        };
        let mut obj_data = JsObjectData::new();
        obj_data.prototype = if is_async_gen {
            self.realm().async_generator_function_prototype
                .clone()
                .or(self.realm().object_prototype.clone())
        } else if is_gen {
            self.realm().generator_function_prototype
                .clone()
                .or(self.realm().object_prototype.clone())
        } else if is_async_non_gen {
            self.realm().async_function_prototype
                .clone()
                .or(self.realm().object_prototype.clone())
        } else {
            self.realm().function_prototype
                .clone()
                .or(self.realm().object_prototype.clone())
        };
        obj_data.callable = Some(func);
        obj_data.class_name = if is_async_gen {
            "AsyncGeneratorFunction".to_string()
        } else if is_gen {
            "GeneratorFunction".to_string()
        } else if is_async_non_gen {
            "AsyncFunction".to_string()
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
        // Annex B: sloppy non-arrow, non-generator, non-async functions get
        // own caller/arguments = null to shadow the ThrowTypeError accessor
        if let Some(JsFunction::User {
            is_strict,
            is_arrow,
            is_generator,
            is_async,
            ..
        }) = &obj_data.callable
            && !*is_strict
            && !*is_arrow
            && !*is_generator
            && !*is_async
        {
            obj_data.insert_property(
                "caller".to_string(),
                PropertyDescriptor::data(JsValue::Null, false, false, true),
            );
            obj_data.insert_property(
                "arguments".to_string(),
                PropertyDescriptor::data(JsValue::Null, false, false, true),
            );
        }
        let is_constructable = match &obj_data.callable {
            Some(JsFunction::User {
                is_arrow,
                is_async,
                is_generator,
                is_method,
                ..
            }) => !is_arrow && !is_method && !*is_generator && !*is_async,
            Some(JsFunction::Native(_, _, _, is_ctor)) => *is_ctor,
            None => false,
        };
        // Generators/async-generators always need .prototype (for generator prototype chain)
        // even when they are class methods (non-constructable)
        let needs_prototype = is_constructable || is_gen || is_async_gen;
        if needs_prototype {
            let proto = self.create_object();
            if is_async_gen {
                proto.borrow_mut().prototype = self.realm().async_generator_prototype.clone();
            } else if is_gen {
                proto.borrow_mut().prototype = self.realm().generator_prototype.clone();
            }
            let proto_id = proto.borrow().id.unwrap();
            let proto_val = JsValue::Object(crate::types::JsObject { id: proto_id });
            obj_data.insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(proto_val.clone(), true, false, false),
            );
        }
        let obj = Rc::new(RefCell::new(obj_data));
        let func_id = self.allocate_object_slot(obj.clone());
        self.function_realm_map.insert(func_id, self.current_realm_id);
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
        if let JsValue::Object(o) = val
            && let Some(obj) = self.get_object(o.id)
        {
            let obj_ref = obj.borrow();
            if obj_ref.callable.is_none() {
                return;
            }
            if let Some(prop) = obj_ref.properties.get("name")
                && let Some(ref v) = prop.value
            {
                if let JsValue::String(s) = v {
                    if !s.to_string().is_empty() {
                        return;
                    }
                } else {
                    return;
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

    fn create_arguments_object(
        &mut self,
        args: &[JsValue],
        callee: JsValue,
        _is_strict: bool,
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

            if let Some(env) = func_env {
                // Mapped (sloppy + simple params): callee is a data property
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

                let mut map = HashMap::new();
                for (i, name) in param_names.iter().enumerate() {
                    map.insert(i.to_string(), (env.clone(), name.clone()));
                }
                if !map.is_empty() {
                    o.parameter_map = Some(map);
                }
            }
        }

        let result_id = obj.borrow().id.unwrap();
        let result = JsValue::Object(crate::types::JsObject { id: result_id });

        // Unmapped (strict OR non-simple params): callee is a throw accessor
        if func_env.is_none() {
            let thrower = self.realm()
                .throw_type_error
                .clone()
                .unwrap_or_else(|| self.create_thrower_function());
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

    /// Convert a symbol property key string (e.g. "Symbol(Symbol.toStringTag)" or "Symbol(desc)#42")
    /// back to a JsValue::Symbol.
    pub(crate) fn symbol_key_to_jsvalue(&self, key: &str) -> JsValue {
        // Well-known symbols: "Symbol(Symbol.xyz)" — look up from Symbol constructor
        if key.starts_with("Symbol(Symbol.") && key.ends_with(')') && !key.contains('#') {
            // Extract the well-known name (e.g. "toStringTag" from "Symbol(Symbol.toStringTag)")
            let inner = &key[7..key.len() - 1]; // "Symbol.toStringTag"
            let name = &inner[7..]; // "toStringTag"
            if let Some(sym_val) = self.realm().global_env.borrow().get("Symbol")
                && let JsValue::Object(so) = sym_val
                && let Some(sobj) = self.get_object(so.id)
            {
                let val = sobj.borrow().get_property(name);
                if let JsValue::Symbol(s) = val {
                    return JsValue::Symbol(s);
                }
            }
        }
        // User symbols with id: "Symbol(desc)#id" or "Symbol()#id"
        if let Some(hash_pos) = key.rfind('#')
            && let Ok(id) = key[hash_pos + 1..].parse::<u64>()
        {
            let desc_part = &key[7..hash_pos]; // content between "Symbol(" and ")#id"
            // desc_part should end with ')'
            let desc = if let Some(inner) = desc_part.strip_suffix(')') {
                if inner.is_empty() {
                    None
                } else {
                    Some(JsString::from_str(inner))
                }
            } else {
                None
            };
            // Check global_symbol_registry for Symbol.for() symbols
            for sym in self.global_symbol_registry.values() {
                if sym.id == id {
                    return JsValue::Symbol(sym.clone());
                }
            }
            return JsValue::Symbol(crate::types::JsSymbol {
                id,
                description: desc,
            });
        }
        // Fallback: return as string
        JsValue::String(JsString::from_str(key))
    }

    pub fn run(&mut self, program: &Program) -> Completion {
        self.maybe_gc();
        let result = match program.source_type {
            SourceType::Script => {
                let global = self.realm().global_env.clone();
                if program.body_is_strict {
                    global.borrow_mut().strict = true;
                }
                self.exec_statements(&program.body, &global)
            }
            SourceType::Module => self.run_module(program, None),
        };
        self.drain_microtasks();
        result
    }

    pub fn run_with_path(&mut self, program: &Program, path: &Path) -> Completion {
        self.maybe_gc();
        let result = match program.source_type {
            SourceType::Script => {
                let prev = self.current_module_path.take();
                self.current_module_path = Some(path.to_path_buf());
                let global = self.realm().global_env.clone();
                if program.body_is_strict {
                    global.borrow_mut().strict = true;
                }
                let r = self.exec_statements(&program.body, &global);
                self.current_module_path = prev;
                r
            }
            SourceType::Module => self.run_module(program, Some(path.to_path_buf())),
        };
        self.drain_microtasks();
        result
    }

    #[allow(dead_code)]
    pub fn get_current_module_path(&self) -> Option<&Path> {
        self.current_module_path.as_deref()
    }

    fn run_module(&mut self, program: &Program, module_path: Option<PathBuf>) -> Completion {
        let prev_module_path = self.current_module_path.take();
        self.current_module_path = module_path.clone();

        let module_env = Environment::new_function_scope(Some(self.realm().global_env.clone()));
        module_env.borrow_mut().strict = true;
        {
            let mut env = module_env.borrow_mut();
            env.declare("this", BindingKind::Var);
        }

        // Register entry-point module in registry to handle self-imports
        let canon_path_entry = module_path
            .as_ref()
            .map(|p| p.canonicalize().unwrap_or_else(|_| p.clone()))
            .unwrap_or_default();
        let loaded_module = Rc::new(RefCell::new(LoadedModule {
            path: canon_path_entry.clone(),
            env: module_env.clone(),
            exports: HashMap::new(),
            export_bindings: HashMap::new(),
        }));
        if let Some(ref path) = module_path {
            let canon_path = path.canonicalize().unwrap_or_else(|_| path.clone());
            self.module_registry
                .insert(canon_path, loaded_module.clone());
        }

        // Collect export names and bindings first (before processing imports) for namespace objects
        for item in &program.module_items {
            if let ModuleItem::ExportDeclaration(export) = item {
                let bindings = self.get_export_bindings(export);
                for (export_name, binding_name) in bindings {
                    loaded_module
                        .borrow_mut()
                        .exports
                        .insert(export_name.clone(), JsValue::Undefined);
                    loaded_module
                        .borrow_mut()
                        .export_bindings
                        .insert(export_name, binding_name);
                }
            }
        }

        // First pass: hoist declarations (before processing imports)
        for item in &program.module_items {
            match item {
                ModuleItem::Statement(stmt) => {
                    self.hoist_module_statement(stmt, &module_env);
                }
                ModuleItem::ExportDeclaration(export) => {
                    self.hoist_export_declaration(export, &module_env);
                }
                _ => {}
            }
        }

        // Second pass: process imports (after hoisting)
        for item in &program.module_items {
            if let ModuleItem::ImportDeclaration(import) = item
                && let Err(e) = self.process_import(import, &module_env)
            {
                self.current_module_path = prev_module_path;
                return Completion::Throw(e);
            }
        }

        // Third pass: process re-exports (export * from)
        for item in &program.module_items {
            if let ModuleItem::ExportDeclaration(ExportDeclaration::All { source, exported }) = item
                && let Err(e) =
                    self.process_star_reexport(source, exported.as_ref(), &loaded_module)
            {
                self.current_module_path = prev_module_path;
                return Completion::Throw(e);
            }
        }

        // Third-and-half pass: validate named re-exports (export { x } from './mod')
        if let Some(ref canon_path) = module_path {
            for item in &program.module_items {
                if let ModuleItem::ExportDeclaration(ExportDeclaration::Named {
                    specifiers,
                    source: Some(source),
                    ..
                }) = item
                    && let Err(e) = self.validate_named_reexports(canon_path, source, specifiers)
                {
                    self.current_module_path = prev_module_path;
                    return Completion::Throw(e);
                }
            }
        }

        // Fourth pass: execute statements
        for item in &program.module_items {
            match item {
                ModuleItem::Statement(stmt) => {
                    let result = self.exec_statement(stmt, &module_env);
                    if result.is_abrupt() {
                        self.current_module_path = prev_module_path;
                        return result;
                    }
                }
                ModuleItem::ImportDeclaration(_) => {
                    // Already processed
                }
                ModuleItem::ExportDeclaration(export) => {
                    let result = self.exec_export_declaration(export, &module_env);
                    if result.is_abrupt() {
                        self.current_module_path = prev_module_path;
                        return result;
                    }
                    // Collect exports for entry-point module
                    self.collect_exports(export, &module_env, &loaded_module);
                }
            }
        }

        self.current_module_path = prev_module_path;
        Completion::Normal(JsValue::Undefined)
    }

    fn process_import(&mut self, import: &ImportDeclaration, env: &EnvRef) -> Result<(), JsValue> {
        let module_path = self.current_module_path.clone();
        let resolved = self.resolve_module_specifier(&import.source, module_path.as_deref())?;

        // Load the module if not already loaded
        let loaded = self.load_module(&resolved)?;

        // Create bindings for each import specifier
        for spec in &import.specifiers {
            match spec {
                ImportSpecifier::Default(local) => {
                    let exports = loaded.borrow().exports.clone();
                    if let Some(val) = exports.get("default") {
                        env.borrow_mut().declare(local, BindingKind::Const);
                        let _ = env.borrow_mut().set(local, val.clone());
                    } else {
                        return Err(JsValue::String(JsString::from_str(&format!(
                            "Module '{}' has no default export",
                            import.source
                        ))));
                    }
                }
                ImportSpecifier::Named { imported, local } => {
                    let exports = loaded.borrow().exports.clone();
                    if let Some(val) = exports.get(imported) {
                        env.borrow_mut().declare(local, BindingKind::Const);
                        let _ = env.borrow_mut().set(local, val.clone());
                    } else {
                        return Err(JsValue::String(JsString::from_str(&format!(
                            "Module '{}' has no export named '{}'",
                            import.source, imported
                        ))));
                    }
                }
                ImportSpecifier::Namespace(local) => {
                    let ns = self.create_module_namespace(&loaded);
                    env.borrow_mut().declare(local, BindingKind::Const);
                    let _ = env.borrow_mut().set(local, ns);
                }
                ImportSpecifier::DeferredNamespace(local) => {
                    // For now, treat deferred namespace as eager namespace
                    let ns = self.create_module_namespace(&loaded);
                    env.borrow_mut().declare(local, BindingKind::Const);
                    let _ = env.borrow_mut().set(local, ns);
                }
            }
        }

        Ok(())
    }

    fn process_star_reexport(
        &mut self,
        source: &str,
        exported_as: Option<&String>,
        module: &Rc<RefCell<LoadedModule>>,
    ) -> Result<(), JsValue> {
        let module_path = self.current_module_path.clone();
        let resolved = self.resolve_module_specifier(source, module_path.as_deref())?;
        let source_module = self.load_module(&resolved)?;

        if let Some(name) = exported_as {
            // export * as ns from './mod' - create namespace object
            let ns = self.create_module_namespace(&source_module);
            module.borrow_mut().exports.insert(name.clone(), ns);
            module
                .borrow_mut()
                .export_bindings
                .insert(name.clone(), format!("*ns:{}", source));
        } else {
            // export * from './mod' - re-export all non-default exports
            let source_exports = source_module.borrow().exports.clone();
            let source_bindings = source_module.borrow().export_bindings.clone();
            for (export_name, val) in source_exports {
                if export_name != "default" {
                    module.borrow_mut().exports.insert(export_name.clone(), val);
                    // For bindings, use a special marker for re-exports
                    if let Some(binding) = source_bindings.get(&export_name) {
                        module
                            .borrow_mut()
                            .export_bindings
                            .insert(export_name, binding.clone());
                    }
                }
            }
        }

        Ok(())
    }

    fn resolve_module_specifier(
        &self,
        specifier: &str,
        referrer: Option<&Path>,
    ) -> Result<PathBuf, JsValue> {
        // Relative paths: ./ or ../
        if specifier.starts_with("./") || specifier.starts_with("../") {
            if let Some(referrer) = referrer {
                let base = referrer.parent().unwrap_or(Path::new("."));
                let resolved = base.join(specifier);
                if resolved.exists() {
                    return Ok(resolved.canonicalize().unwrap_or(resolved));
                }
                return Err(JsValue::String(JsString::from_str(&format!(
                    "Cannot find module '{}'",
                    specifier
                ))));
            } else {
                return Err(JsValue::String(JsString::from_str(
                    "Relative imports require a referrer path",
                )));
            }
        }

        // Absolute paths
        let path = Path::new(specifier);
        if path.is_absolute() && path.exists() {
            return Ok(path.to_path_buf());
        }

        // Bare specifiers not supported
        Err(JsValue::String(JsString::from_str(&format!(
            "Cannot resolve bare module specifier '{}'",
            specifier
        ))))
    }

    fn load_module(&mut self, path: &Path) -> Result<Rc<RefCell<LoadedModule>>, JsValue> {
        let canon_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        // Check if module is already loaded
        if let Some(existing) = self.module_registry.get(&canon_path) {
            return Ok(existing.clone());
        }

        // Read and parse the module
        let source = std::fs::read_to_string(path).map_err(|e| {
            JsValue::String(JsString::from_str(&format!(
                "Cannot read module '{}': {}",
                path.display(),
                e
            )))
        })?;

        let mut parser = parser::Parser::new(&source).map_err(|e| {
            JsValue::String(JsString::from_str(&format!(
                "Parse error in '{}': {}",
                path.display(),
                e
            )))
        })?;

        let program = parser.parse_program_as_module().map_err(|e| {
            JsValue::String(JsString::from_str(&format!(
                "Parse error in '{}': {}",
                path.display(),
                e
            )))
        })?;

        // Create module environment
        let module_env = Environment::new_function_scope(Some(self.realm().global_env.clone()));
        module_env.borrow_mut().strict = true;
        {
            let mut env = module_env.borrow_mut();
            env.declare("this", BindingKind::Var);
        }

        // Register module early to handle circular imports
        let loaded_module = Rc::new(RefCell::new(LoadedModule {
            path: canon_path.clone(),
            env: module_env.clone(),
            exports: HashMap::new(),
            export_bindings: HashMap::new(),
        }));
        self.module_registry
            .insert(canon_path.clone(), loaded_module.clone());

        // Collect export names and bindings first (before processing imports) for namespace objects
        for item in &program.module_items {
            if let ModuleItem::ExportDeclaration(export) = item {
                let bindings = self.get_export_bindings(export);
                for (export_name, binding_name) in bindings {
                    loaded_module
                        .borrow_mut()
                        .exports
                        .insert(export_name.clone(), JsValue::Undefined);
                    loaded_module
                        .borrow_mut()
                        .export_bindings
                        .insert(export_name, binding_name);
                }
            }
        }

        // Execute module with its path set
        let prev_path = self.current_module_path.take();
        self.current_module_path = Some(canon_path.clone());

        // First pass: hoist declarations
        for item in &program.module_items {
            match item {
                ModuleItem::Statement(stmt) => {
                    self.hoist_module_statement(stmt, &module_env);
                }
                ModuleItem::ExportDeclaration(export) => {
                    self.hoist_export_declaration(export, &module_env);
                }
                _ => {}
            }
        }

        // Second pass: process imports (after hoisting)
        for item in &program.module_items {
            if let ModuleItem::ImportDeclaration(import) = item {
                self.process_import(import, &module_env)?;
            }
        }

        // Third pass: process re-exports (export * from)
        for item in &program.module_items {
            if let ModuleItem::ExportDeclaration(ExportDeclaration::All { source, exported }) = item
            {
                self.process_star_reexport(source, exported.as_ref(), &loaded_module)?;
            }
        }

        // Third-and-half pass: validate named re-exports (export { x } from './mod')
        for item in &program.module_items {
            if let ModuleItem::ExportDeclaration(ExportDeclaration::Named {
                specifiers,
                source: Some(source),
                ..
            }) = item
            {
                self.validate_named_reexports(&canon_path, source, specifiers)?;
            }
        }

        // Fourth pass: execute statements
        for item in &program.module_items {
            match item {
                ModuleItem::Statement(stmt) => {
                    let result = self.exec_statement(stmt, &module_env);
                    if let Completion::Throw(e) = result {
                        self.current_module_path = prev_path;
                        return Err(e);
                    }
                }
                ModuleItem::ImportDeclaration(_) => {}
                ModuleItem::ExportDeclaration(export) => {
                    let result = self.exec_export_declaration(export, &module_env);
                    if let Completion::Throw(e) = result {
                        self.current_module_path = prev_path;
                        return Err(e);
                    }
                    // Collect exports
                    self.collect_exports(export, &module_env, &loaded_module);
                }
            }
        }

        self.current_module_path = prev_path;
        Ok(loaded_module)
    }

    fn validate_named_reexports(
        &mut self,
        current_module: &Path,
        source: &str,
        specifiers: &[ExportSpecifier],
    ) -> Result<(), JsValue> {
        let resolved = self.resolve_module_specifier(source, Some(current_module))?;

        for spec in specifiers {
            let mut visited = std::collections::HashSet::new();
            self.resolve_export(&resolved, &spec.local, &mut visited)?;
        }
        Ok(())
    }

    fn resolve_export(
        &mut self,
        module_path: &Path,
        export_name: &str,
        visited: &mut std::collections::HashSet<(PathBuf, String)>,
    ) -> Result<(), JsValue> {
        let canon_path = module_path
            .canonicalize()
            .unwrap_or_else(|_| module_path.to_path_buf());
        let key = (canon_path.clone(), export_name.to_string());

        // Check for circular reference
        if visited.contains(&key) {
            return Err(self.create_error(
                "SyntaxError",
                &format!("Circular re-export of '{}'", export_name),
            ));
        }
        visited.insert(key);

        // Load the module if not already loaded
        let module = self.load_module(&canon_path)?;

        // Check if this export is a local binding or a re-export
        let reexport_info = {
            let module_ref = module.borrow();
            if let Some(binding) = module_ref.export_bindings.get(export_name) {
                if let Some(info) = binding.strip_prefix("*reexport:") {
                    // Format: "source_module:export_name"
                    let parts: Vec<&str> = info.splitn(2, ':').collect();
                    if parts.len() == 2 {
                        Some((parts[0].to_string(), parts[1].to_string()))
                    } else {
                        None
                    }
                } else {
                    // Local binding - exists, no cycle
                    return Ok(());
                }
            } else {
                None
            }
        };

        if let Some((source_specifier, source_export)) = reexport_info {
            let resolved = self.resolve_module_specifier(&source_specifier, Some(&canon_path))?;
            return self.resolve_export(&resolved, &source_export, visited);
        }

        // Export not found
        Err(self.create_error(
            "SyntaxError",
            &format!(
                "Module '{}' has no export named '{}'",
                canon_path.display(),
                export_name
            ),
        ))
    }

    fn collect_exports(
        &mut self,
        export: &ExportDeclaration,
        env: &EnvRef,
        module: &Rc<RefCell<LoadedModule>>,
    ) {
        match export {
            ExportDeclaration::Named {
                specifiers,
                source,
                declaration,
            } => {
                if let Some(src) = source {
                    // Re-export: get values from source module
                    let module_path = self.current_module_path.clone();
                    if let Ok(resolved) = self.resolve_module_specifier(src, module_path.as_deref())
                        && let Ok(source_mod) = self.load_module(&resolved)
                    {
                        let source_exports = source_mod.borrow().exports.clone();
                        for spec in specifiers {
                            if let Some(val) = source_exports.get(&spec.local) {
                                module
                                    .borrow_mut()
                                    .exports
                                    .insert(spec.exported.clone(), val.clone());
                            }
                        }
                    }
                } else {
                    // Local export
                    for spec in specifiers {
                        if let Some(val) = env.borrow().get(&spec.local) {
                            module
                                .borrow_mut()
                                .exports
                                .insert(spec.exported.clone(), val);
                        }
                    }
                    if let Some(decl) = declaration {
                        // Extract names from declaration
                        self.collect_declaration_exports(decl, env, module);
                    }
                }
            }
            ExportDeclaration::Default(expr) => {
                if let Some(val) = env.borrow().get("*default*") {
                    module
                        .borrow_mut()
                        .exports
                        .insert("default".to_string(), val);
                } else {
                    // For expression defaults, evaluate directly
                    let _ = expr; // Already evaluated and stored
                }
            }
            ExportDeclaration::DefaultFunction(f) => {
                if let Some(val) = env.borrow().get("*default*") {
                    module
                        .borrow_mut()
                        .exports
                        .insert("default".to_string(), val);
                }
                if !f.name.is_empty()
                    && let Some(val) = env.borrow().get(&f.name)
                {
                    module
                        .borrow_mut()
                        .exports
                        .insert("default".to_string(), val);
                }
            }
            ExportDeclaration::DefaultClass(c) => {
                if let Some(val) = env.borrow().get("*default*") {
                    module
                        .borrow_mut()
                        .exports
                        .insert("default".to_string(), val);
                }
                if !c.name.is_empty()
                    && let Some(val) = env.borrow().get(&c.name)
                {
                    module
                        .borrow_mut()
                        .exports
                        .insert("default".to_string(), val);
                }
            }
            ExportDeclaration::All { .. } => {
                // Re-exports handled separately
            }
        }
    }

    fn collect_declaration_exports(
        &self,
        decl: &Statement,
        env: &EnvRef,
        module: &Rc<RefCell<LoadedModule>>,
    ) {
        match decl {
            Statement::Variable(var) => {
                for d in &var.declarations {
                    self.collect_pattern_exports(&d.pattern, env, module);
                }
            }
            Statement::FunctionDeclaration(f) => {
                if let Some(val) = env.borrow().get(&f.name) {
                    module.borrow_mut().exports.insert(f.name.clone(), val);
                }
            }
            Statement::ClassDeclaration(c) => {
                if let Some(val) = env.borrow().get(&c.name) {
                    module.borrow_mut().exports.insert(c.name.clone(), val);
                }
            }
            _ => {}
        }
    }

    fn collect_pattern_exports(
        &self,
        pattern: &Pattern,
        env: &EnvRef,
        module: &Rc<RefCell<LoadedModule>>,
    ) {
        match pattern {
            Pattern::Identifier(name) => {
                if let Some(val) = env.borrow().get(name) {
                    module.borrow_mut().exports.insert(name.clone(), val);
                }
            }
            Pattern::Array(elems) => {
                for elem in elems.iter().flatten() {
                    match elem {
                        ArrayPatternElement::Pattern(p) | ArrayPatternElement::Rest(p) => {
                            self.collect_pattern_exports(p, env, module);
                        }
                    }
                }
            }
            Pattern::Object(props) => {
                for prop in props {
                    match prop {
                        ObjectPatternProperty::KeyValue(_, p) | ObjectPatternProperty::Rest(p) => {
                            self.collect_pattern_exports(p, env, module);
                        }
                        ObjectPatternProperty::Shorthand(name) => {
                            if let Some(val) = env.borrow().get(name) {
                                module.borrow_mut().exports.insert(name.clone(), val);
                            }
                        }
                    }
                }
            }
            Pattern::Assign(inner, _) | Pattern::Rest(inner) => {
                self.collect_pattern_exports(inner, env, module);
            }
            Pattern::MemberExpression(_) => {}
        }
    }

    fn create_module_namespace(&mut self, module: &Rc<RefCell<LoadedModule>>) -> JsValue {
        use crate::interpreter::types::ModuleNamespaceData;

        let obj = self.create_object();
        let module_ref = module.borrow();
        let env = module_ref.env.clone();
        let export_bindings = module_ref.export_bindings.clone();

        // Get module path for looking up re-exports dynamically
        let module_path = if module_ref.path.as_os_str().is_empty() {
            None
        } else {
            Some(module_ref.path.clone())
        };

        // Collect export names - these will be looked up dynamically
        let mut export_names: Vec<String> = module_ref.exports.keys().cloned().collect();
        export_names.sort(); // Module namespace exports are sorted alphabetically

        // Set module namespace data for live bindings
        obj.borrow_mut().module_namespace = Some(ModuleNamespaceData {
            env: env.clone(),
            export_names: export_names.clone(),
            export_to_binding: export_bindings,
            module_path,
        });
        obj.borrow_mut().class_name = "Module".to_string();
        obj.borrow_mut().extensible = false; // Module namespaces are non-extensible
        obj.borrow_mut().prototype = None; // Module namespaces have null prototype

        // Add property descriptors for each export (values will be looked up dynamically)
        for name in &export_names {
            // Exports: writable=true, enumerable=true, configurable=false
            // Value is left as undefined - will be looked up dynamically
            obj.borrow_mut().insert_property(
                name.clone(),
                PropertyDescriptor::data(JsValue::Undefined, true, true, false),
            );
        }

        // Set Symbol.toStringTag to "Module"
        // writable=false, enumerable=false, configurable=false
        let sym_key = self
            .get_symbol_key("toStringTag")
            .unwrap_or_else(|| "Symbol(Symbol.toStringTag)".to_string());
        obj.borrow_mut().insert_property(
            sym_key,
            PropertyDescriptor::data(
                JsValue::String(JsString::from_str("Module")),
                false,
                false,
                false,
            ),
        );

        let id = obj.borrow().id.unwrap();
        JsValue::Object(crate::types::JsObject { id })
    }

    fn hoist_module_statement(&mut self, stmt: &Statement, env: &EnvRef) {
        match stmt {
            Statement::Variable(decl) if decl.kind == VarKind::Var => {
                for d in &decl.declarations {
                    self.hoist_pattern(&d.pattern, env, false);
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
                    is_strict: true, // Module code is always strict
                    is_generator: f.is_generator,
                    is_async: f.is_async,
                    is_method: false,
                    source_text: f.source_text.clone(),
                };
                let val = self.create_function(func);
                let _ = env.borrow_mut().set(&f.name, val);
            }
            _ => {}
        }
    }

    fn hoist_export_declaration(&mut self, export: &ExportDeclaration, env: &EnvRef) {
        match export {
            ExportDeclaration::Named {
                declaration: Some(decl),
                ..
            } => {
                self.hoist_module_statement(decl, env);
            }
            ExportDeclaration::DefaultFunction(f) => {
                if !f.name.is_empty() {
                    env.borrow_mut().declare(&f.name, BindingKind::Const);
                    let func = JsFunction::User {
                        name: Some(f.name.clone()),
                        params: f.params.clone(),
                        body: f.body.clone(),
                        closure: env.clone(),
                        is_arrow: false,
                        is_strict: true,
                        is_generator: f.is_generator,
                        is_async: f.is_async,
                        is_method: false,
                        source_text: f.source_text.clone(),
                    };
                    let val = self.create_function(func);
                    let _ = env.borrow_mut().set(&f.name, val);
                }
            }
            _ => {}
        }
    }

    #[allow(dead_code)]
    fn get_export_names(&self, export: &ExportDeclaration) -> Vec<String> {
        let mut names = Vec::new();
        match export {
            ExportDeclaration::Named {
                specifiers,
                declaration,
                ..
            } => {
                for spec in specifiers {
                    names.push(spec.exported.clone());
                }
                if let Some(decl) = declaration {
                    self.get_declaration_export_names(decl, &mut names);
                }
            }
            ExportDeclaration::Default(_)
            | ExportDeclaration::DefaultFunction(_)
            | ExportDeclaration::DefaultClass(_) => {
                names.push("default".to_string());
            }
            ExportDeclaration::All { .. } => {
                // Re-exports: will need to load the source module to get names
                // For now, skip these
            }
        }
        names
    }

    // Returns (export_name, binding_name) pairs
    // For re-exports, binding_name is "*reexport:source:export_name"
    fn get_export_bindings(&self, export: &ExportDeclaration) -> Vec<(String, String)> {
        let mut bindings = Vec::new();
        match export {
            ExportDeclaration::Named {
                specifiers,
                source,
                declaration,
            } => {
                if let Some(src) = source {
                    // Re-export: export { x } from './mod'
                    for spec in specifiers {
                        let binding = format!("*reexport:{}:{}", src, spec.local);
                        bindings.push((spec.exported.clone(), binding));
                    }
                } else {
                    // Local export: export { local as exported }
                    for spec in specifiers {
                        bindings.push((spec.exported.clone(), spec.local.clone()));
                    }
                    // export var x = ... - x is both binding and export
                    if let Some(decl) = declaration {
                        let mut names = Vec::new();
                        self.get_declaration_export_names(decl, &mut names);
                        for name in names {
                            bindings.push((name.clone(), name));
                        }
                    }
                }
            }
            ExportDeclaration::Default(_) => {
                // export default expr - stored in special "*default*" binding
                bindings.push(("default".to_string(), "*default*".to_string()));
            }
            ExportDeclaration::DefaultFunction(f) => {
                if f.name.is_empty() {
                    bindings.push(("default".to_string(), "*default*".to_string()));
                } else {
                    bindings.push(("default".to_string(), f.name.clone()));
                }
            }
            ExportDeclaration::DefaultClass(c) => {
                if c.name.is_empty() {
                    bindings.push(("default".to_string(), "*default*".to_string()));
                } else {
                    bindings.push(("default".to_string(), c.name.clone()));
                }
            }
            ExportDeclaration::All { .. } => {
                // Re-exports: handled separately
            }
        }
        bindings
    }

    fn get_declaration_export_names(&self, stmt: &Statement, names: &mut Vec<String>) {
        match stmt {
            Statement::Variable(decl) => {
                for d in &decl.declarations {
                    self.get_pattern_names(&d.pattern, names);
                }
            }
            Statement::FunctionDeclaration(f) => {
                names.push(f.name.clone());
            }
            Statement::ClassDeclaration(c) => {
                names.push(c.name.clone());
            }
            _ => {}
        }
    }

    fn get_pattern_names(&self, pattern: &Pattern, names: &mut Vec<String>) {
        use crate::ast::{ArrayPatternElement, ObjectPatternProperty};
        match pattern {
            Pattern::Identifier(name) => names.push(name.clone()),
            Pattern::Object(props) => {
                for prop in props {
                    match prop {
                        ObjectPatternProperty::Shorthand(name) => names.push(name.clone()),
                        ObjectPatternProperty::KeyValue(_, p) | ObjectPatternProperty::Rest(p) => {
                            self.get_pattern_names(p, names);
                        }
                    }
                }
            }
            Pattern::Array(elems) => {
                for e in elems.iter().flatten() {
                    match e {
                        ArrayPatternElement::Pattern(p) | ArrayPatternElement::Rest(p) => {
                            self.get_pattern_names(p, names);
                        }
                    }
                }
            }
            Pattern::Assign(inner, _) | Pattern::Rest(inner) => {
                self.get_pattern_names(inner, names);
            }
            Pattern::MemberExpression(_) => {}
        }
    }

    fn exec_export_declaration(&mut self, export: &ExportDeclaration, env: &EnvRef) -> Completion {
        match export {
            ExportDeclaration::Named {
                declaration: Some(decl),
                ..
            } => self.exec_statement(decl, env),
            ExportDeclaration::Named {
                declaration: None, ..
            } => Completion::Normal(JsValue::Undefined),
            ExportDeclaration::Default(expr) => {
                let val = match self.eval_expr(expr, env) {
                    Completion::Normal(v) => v,
                    c => return c,
                };
                env.borrow_mut().declare("*default*", BindingKind::Const);
                let _ = env.borrow_mut().set("*default*", val);
                Completion::Normal(JsValue::Undefined)
            }
            ExportDeclaration::DefaultFunction(func) => {
                let name = if func.name.is_empty() {
                    "default".to_string()
                } else {
                    func.name.clone()
                };
                let enclosing_strict = env.borrow().strict;
                let js_func = JsFunction::User {
                    name: Some(name),
                    params: func.params.clone(),
                    body: func.body.clone(),
                    closure: env.clone(),
                    is_arrow: false,
                    is_strict: func.body_is_strict || enclosing_strict,
                    is_generator: func.is_generator,
                    is_async: func.is_async,
                    is_method: false,
                    source_text: func.source_text.clone(),
                };
                let fn_obj = self.create_function(js_func);
                if !func.name.is_empty() {
                    env.borrow_mut().declare(&func.name, BindingKind::Const);
                    let _ = env.borrow_mut().set(&func.name, fn_obj.clone());
                }
                env.borrow_mut().declare("*default*", BindingKind::Const);
                let _ = env.borrow_mut().set("*default*", fn_obj);
                Completion::Normal(JsValue::Undefined)
            }
            ExportDeclaration::DefaultClass(class) => {
                let name = if class.name.is_empty() {
                    "default".to_string()
                } else {
                    class.name.clone()
                };
                let class_val = match self.eval_class(
                    &name,
                    &class.super_class,
                    &class.body,
                    env,
                    class.source_text.clone(),
                ) {
                    Completion::Normal(v) => v,
                    c => return c,
                };
                if !class.name.is_empty() {
                    env.borrow_mut().declare(&class.name, BindingKind::Const);
                    let _ = env.borrow_mut().set(&class.name, class_val.clone());
                }
                env.borrow_mut().declare("*default*", BindingKind::Const);
                let _ = env.borrow_mut().set("*default*", class_val);
                Completion::Normal(JsValue::Undefined)
            }
            ExportDeclaration::All { .. } => {
                // Re-exports handled in Phase 3
                Completion::Normal(JsValue::Undefined)
            }
        }
    }

    pub(crate) fn drain_microtasks(&mut self) {
        while !self.microtask_queue.is_empty() {
            let job = self.microtask_queue.remove(0);
            let _ = job(self);
        }
        self.microtask_roots.clear();
    }

    /// §10.4.2.4 ArraySetLength(A, Desc)
    pub(crate) fn array_set_length(
        &mut self,
        obj_id: usize,
        desc: PropertyDescriptor,
    ) -> Result<bool, JsValue> {
        // 1. If Desc does not have [[Value]], just do OrdinaryDefineOwnProperty(A, "length", Desc)
        if desc.value.is_none() {
            let obj_rc = self.get_object(obj_id as u64).unwrap();
            return Ok(obj_rc
                .borrow_mut()
                .define_own_property("length".to_string(), desc));
        }

        // 2. Let newLenDesc be a copy of Desc.
        let desc_value = desc.value.clone().unwrap();
        let mut new_len_desc = desc;

        // 3. Let newLen be ? ToUint32(Desc.[[Value]])
        let number_len = self.to_number_value(&desc_value)?;
        let new_len = to_uint32_f64(number_len);

        // 4. If SameValueZero(newLen, numberLen) is false, throw RangeError.
        //    The spec actually says: If newLen != numberLen (as Number values), throw RangeError.
        if (new_len as f64) != number_len {
            return Err(self.create_error("RangeError", "Invalid array length"));
        }

        // 5. Set newLenDesc.[[Value]] to newLen (as a Number).
        new_len_desc.value = Some(JsValue::Number(new_len as f64));

        // 6. Let oldLenDesc be OrdinaryGetOwnProperty(A, "length").
        let (old_len, old_len_writable) = {
            let obj_rc = self.get_object(obj_id as u64).unwrap();
            let obj = obj_rc.borrow();
            let old_len_desc = obj.properties.get("length").cloned();
            match old_len_desc {
                Some(ref d) => {
                    let ol = d
                        .value
                        .as_ref()
                        .and_then(|v| {
                            if let JsValue::Number(n) = v {
                                Some(*n as u32)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0);
                    let w = d.writable.unwrap_or(true);
                    (ol, w)
                }
                None => (0, true),
            }
        };

        // 7. If newLen >= oldLen, return OrdinaryDefineOwnProperty(A, "length", newLenDesc).
        if new_len >= old_len {
            let obj_rc = self.get_object(obj_id as u64).unwrap();
            let result = obj_rc
                .borrow_mut()
                .define_own_property("length".to_string(), new_len_desc);
            return Ok(result);
        }

        // 8. If oldLenDesc.[[Writable]] is false, return false.
        if !old_len_writable {
            return Ok(false);
        }

        // 9. If newLenDesc.[[Writable]] is absent or true, let newWritable be true.
        //    Else, need to defer setting writable to false until after deletions.
        let new_writable = !matches!(new_len_desc.writable, Some(false));

        // 10. If newWritable is false, set newLenDesc.[[Writable]] to true (we'll set it false later).
        if !new_writable {
            new_len_desc.writable = Some(true);
        }

        // 11. Let succeeded be OrdinaryDefineOwnProperty(A, "length", newLenDesc).
        {
            let obj_rc = self.get_object(obj_id as u64).unwrap();
            let succeeded = obj_rc
                .borrow_mut()
                .define_own_property("length".to_string(), new_len_desc);
            // 12. If succeeded is false, return false.
            if !succeeded {
                return Ok(false);
            }
        }

        // 13. For each property key P that is an array index, delete from oldLen-1 down to newLen.
        //     Stop if a deletion fails (non-configurable).
        let mut actual_new_len = new_len;
        {
            let obj_rc = self.get_object(obj_id as u64).unwrap();
            let mut obj = obj_rc.borrow_mut();

            // Collect array index keys >= newLen and < oldLen, sorted descending.
            let mut idx_keys: Vec<(u32, String)> = obj
                .properties
                .keys()
                .filter_map(|k| {
                    k.parse::<u32>()
                        .ok()
                        .filter(|&idx| idx.to_string() == *k && idx >= new_len && idx < old_len)
                        .map(|idx| (idx, k.clone()))
                })
                .collect();
            idx_keys.sort_by(|a, b| b.0.cmp(&a.0));

            for (idx, k) in &idx_keys {
                let is_non_configurable = obj
                    .properties
                    .get(k.as_str())
                    .is_some_and(|d| d.configurable == Some(false));
                if is_non_configurable {
                    // 13.d.iii. Set newLenDesc.[[Value]] to index + 1.
                    actual_new_len = idx + 1;
                    break;
                } else {
                    obj.properties.remove(k.as_str());
                    obj.property_order.retain(|p| p != k);
                }
            }

            // Also truncate array_elements.
            if let Some(ref mut elements) = obj.array_elements {
                elements.truncate(actual_new_len as usize);
            }
        }

        // If we were blocked by a non-configurable element, update length and handle writable.
        if actual_new_len != new_len {
            let obj_rc = self.get_object(obj_id as u64).unwrap();
            let mut obj = obj_rc.borrow_mut();
            if let Some(len_desc) = obj.properties.get_mut("length") {
                len_desc.value = Some(JsValue::Number(actual_new_len as f64));
            }
            // 13.d.iii.2. If newWritable is false, set length writable to false.
            if !new_writable && let Some(len_desc) = obj.properties.get_mut("length") {
                len_desc.writable = Some(false);
            }
            return Ok(false);
        }

        // 14. If newWritable is false, set [[Writable]] on length to false.
        if !new_writable {
            let obj_rc = self.get_object(obj_id as u64).unwrap();
            let mut obj = obj_rc.borrow_mut();
            if let Some(len_desc) = obj.properties.get_mut("length") {
                len_desc.writable = Some(false);
            }
        }

        Ok(true)
    }

    /// §10.4.2.1 [[DefineOwnProperty]](P, Desc) for Array exotic objects
    pub(crate) fn array_define_own_property(
        &mut self,
        obj_id: usize,
        key: &str,
        desc: PropertyDescriptor,
    ) -> Result<bool, JsValue> {
        // 1. If P is "length", return ArraySetLength(A, Desc).
        if key == "length" {
            return self.array_set_length(obj_id, desc);
        }

        // 2. If P is an array index (canonical numeric string with value <= 0xFFFFFFFE)...
        if let Ok(index) = key.parse::<u64>()
            && index <= 0xFFFF_FFFE
            && index.to_string() == key
        {
            let index_u32 = index as u32;

            // 2.a. Let oldLen be the current length value.
            let (old_len, length_writable) = {
                let obj_rc = self.get_object(obj_id as u64).unwrap();
                let obj = obj_rc.borrow();
                let len_desc = obj.properties.get("length");
                let ol = len_desc
                    .and_then(|d| d.value.as_ref())
                    .and_then(|v| {
                        if let JsValue::Number(n) = v {
                            Some(*n as u32)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0);
                let w = len_desc.and_then(|d| d.writable).unwrap_or(true);
                (ol, w)
            };

            // 2.b. If index >= oldLen and length is non-writable, return false.
            if index_u32 >= old_len && !length_writable {
                return Ok(false);
            }

            // 2.c. Let succeeded be OrdinaryDefineOwnProperty(A, P, Desc).
            let succeeded = {
                let obj_rc = self.get_object(obj_id as u64).unwrap();
                obj_rc
                    .borrow_mut()
                    .define_own_property(key.to_string(), desc.clone())
            };

            // 2.d. If succeeded is false, return false.
            if !succeeded {
                return Ok(false);
            }

            // 2.e. If index >= oldLen, set length to index + 1.
            if index_u32 >= old_len {
                let new_len = index_u32 + 1;
                let obj_rc = self.get_object(obj_id as u64).unwrap();
                let mut obj = obj_rc.borrow_mut();
                if let Some(len_desc) = obj.properties.get_mut("length") {
                    len_desc.value = Some(JsValue::Number(new_len as f64));
                }
                if let Some(ref mut elems) = obj.array_elements {
                    let val = desc.value.unwrap_or(JsValue::Undefined);
                    let idx = index_u32 as usize;
                    if idx < elems.len() {
                        elems[idx] = val;
                    } else if idx <= elems.len() + 1024 {
                        while elems.len() < idx {
                            elems.push(JsValue::Undefined);
                        }
                        elems.push(val);
                    }
                }
            } else {
                let obj_rc = self.get_object(obj_id as u64).unwrap();
                let mut obj = obj_rc.borrow_mut();
                if let Some(ref mut elems) = obj.array_elements {
                    let idx = index_u32 as usize;
                    if idx < elems.len()
                        && let Some(ref val) = desc.value
                    {
                        elems[idx] = val.clone();
                    }
                }
            }

            return Ok(true);
        }

        // 3. Return OrdinaryDefineOwnProperty(A, P, Desc).
        let obj_rc = self.get_object(obj_id as u64).unwrap();
        Ok(obj_rc
            .borrow_mut()
            .define_own_property(key.to_string(), desc))
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

/// §7.1.6 ToUint32 — convert an f64 to u32 per spec.
fn to_uint32_f64(n: f64) -> u32 {
    if n.is_nan() || n.is_infinite() || n == 0.0 {
        return 0;
    }
    let int_val = n.signum() * n.abs().floor();
    let modulo = int_val % 4294967296.0;
    let modulo = if modulo < 0.0 {
        modulo + 4294967296.0
    } else {
        modulo
    };
    modulo as u32
}
