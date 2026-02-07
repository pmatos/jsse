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
mod eval;
mod exec;
mod gc;
pub(crate) mod generator_analysis;
pub(crate) mod generator_transform;

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
    function_prototype: Option<Rc<RefCell<JsObjectData>>>,
    generator_function_prototype: Option<Rc<RefCell<JsObjectData>>>,
    async_iterator_prototype: Option<Rc<RefCell<JsObjectData>>>,
    async_generator_prototype: Option<Rc<RefCell<JsObjectData>>>,
    async_generator_function_prototype: Option<Rc<RefCell<JsObjectData>>>,
    async_function_prototype: Option<Rc<RefCell<JsObjectData>>>,
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
    module_registry: HashMap<PathBuf, Rc<RefCell<LoadedModule>>>,
    current_module_path: Option<PathBuf>,
    throw_type_error: Option<JsValue>,
    last_call_had_explicit_return: bool,
    last_call_this_value: Option<JsValue>,
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
            function_prototype: None,
            generator_function_prototype: None,
            async_iterator_prototype: None,
            async_generator_prototype: None,
            async_generator_function_prototype: None,
            async_function_prototype: None,
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
            module_registry: HashMap::new(),
            current_module_path: None,
            throw_type_error: None,
            last_call_had_explicit_return: false,
            last_call_this_value: None,
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
        let is_accessor = desc.get.is_some() || desc.set.is_some();
        {
            let mut r = result.borrow_mut();
            if is_accessor {
                r.insert_value("get".to_string(), desc.get.clone().unwrap_or(JsValue::Undefined));
                r.insert_value("set".to_string(), desc.set.clone().unwrap_or(JsValue::Undefined));
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
            self.async_generator_function_prototype
                .clone()
                .or(self.object_prototype.clone())
        } else if is_gen {
            self.generator_function_prototype
                .clone()
                .or(self.object_prototype.clone())
        } else if is_async_non_gen {
            self.async_function_prototype
                .clone()
                .or(self.object_prototype.clone())
        } else {
            // Look up Function.prototype dynamically to ensure identity matches
            let fp = self.global_env.borrow().get("Function").and_then(|fv| {
                if let JsValue::Object(fo) = fv {
                    self.get_object(fo.id).and_then(|fd| {
                        let pv = fd.borrow().get_property("prototype");
                        if let JsValue::Object(pr) = pv {
                            self.get_object(pr.id)
                        } else {
                            None
                        }
                    })
                } else {
                    None
                }
            });
            fp.or(self.object_prototype.clone())
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
        if let Some(JsFunction::User { is_strict, is_arrow, is_generator, is_async, .. }) = &obj_data.callable {
            if !*is_strict && !*is_arrow && !*is_generator && !*is_async {
                obj_data.insert_property(
                    "caller".to_string(),
                    PropertyDescriptor::data(JsValue::Null, false, false, true),
                );
                obj_data.insert_property(
                    "arguments".to_string(),
                    PropertyDescriptor::data(JsValue::Null, false, false, true),
                );
            }
        }
        let is_constructable = match &obj_data.callable {
            Some(JsFunction::User {
                is_arrow, is_async, is_generator, ..
            }) => !is_arrow && !(*is_async && !*is_generator),
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
            obj_data.insert_property("prototype".to_string(), PropertyDescriptor::data(proto_val.clone(), true, false, false));
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
            let thrower = self
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

    pub fn run(&mut self, program: &Program) -> Completion {
        self.maybe_gc();
        let result = match program.source_type {
            SourceType::Script => self.exec_statements(&program.body, &self.global_env.clone()),
            SourceType::Module => self.run_module(program, None),
        };
        self.drain_microtasks();
        result
    }

    pub fn run_with_path(&mut self, program: &Program, path: &Path) -> Completion {
        self.maybe_gc();
        let result = match program.source_type {
            SourceType::Script => self.exec_statements(&program.body, &self.global_env.clone()),
            SourceType::Module => self.run_module(program, Some(path.to_path_buf())),
        };
        self.drain_microtasks();
        result
    }

    pub fn get_current_module_path(&self) -> Option<&Path> {
        self.current_module_path.as_deref()
    }

    fn run_module(&mut self, program: &Program, module_path: Option<PathBuf>) -> Completion {
        let prev_module_path = self.current_module_path.take();
        self.current_module_path = module_path.clone();

        let module_env = Environment::new_function_scope(Some(self.global_env.clone()));
        module_env.borrow_mut().strict = true;

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
            if let ModuleItem::ImportDeclaration(import) = item {
                if let Err(e) = self.process_import(import, &module_env) {
                    self.current_module_path = prev_module_path;
                    return Completion::Throw(e);
                }
            }
        }

        // Third pass: process re-exports (export * from)
        for item in &program.module_items {
            if let ModuleItem::ExportDeclaration(ExportDeclaration::All { source, exported }) = item
            {
                if let Err(e) =
                    self.process_star_reexport(source, exported.as_ref(), &loaded_module)
                {
                    self.current_module_path = prev_module_path;
                    return Completion::Throw(e);
                }
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
                {
                    if let Err(e) = self.validate_named_reexports(canon_path, source, specifiers) {
                        self.current_module_path = prev_module_path;
                        return Completion::Throw(e);
                    }
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
        let module_env = Environment::new_function_scope(Some(self.global_env.clone()));
        module_env.borrow_mut().strict = true;

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
                    {
                        if let Ok(source_mod) = self.load_module(&resolved) {
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
                if !f.name.is_empty() {
                    if let Some(val) = env.borrow().get(&f.name) {
                        module
                            .borrow_mut()
                            .exports
                            .insert("default".to_string(), val);
                    }
                }
            }
            ExportDeclaration::DefaultClass(c) => {
                if let Some(val) = env.borrow().get("*default*") {
                    module
                        .borrow_mut()
                        .exports
                        .insert("default".to_string(), val);
                }
                if !c.name.is_empty() {
                    if let Some(val) = env.borrow().get(&c.name) {
                        module
                            .borrow_mut()
                            .exports
                            .insert("default".to_string(), val);
                    }
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
                        source_text: f.source_text.clone(),
                    };
                    let val = self.create_function(func);
                    let _ = env.borrow_mut().set(&f.name, val);
                }
            }
            _ => {}
        }
    }

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
                for elem in elems {
                    if let Some(e) = elem {
                        match e {
                            ArrayPatternElement::Pattern(p) | ArrayPatternElement::Rest(p) => {
                                self.get_pattern_names(p, names);
                            }
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
                let js_func = JsFunction::User {
                    name: Some(name),
                    params: func.params.clone(),
                    body: func.body.clone(),
                    closure: env.clone(),
                    is_arrow: false,
                    is_strict: Self::is_strict_mode_body(&func.body) || env.borrow().strict,
                    is_generator: func.is_generator,
                    is_async: func.is_async,
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
