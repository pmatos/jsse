use crate::ast::*;
use crate::parser;
use crate::types::{JsBigInt, JsString, JsValue, bigint_ops, number_ops};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

pub struct AgentBroadcastMsg {
    pub sab_shared: Arc<types::SharedBufferInner>,
}

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
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ImportModuleType {
    Text,
    Bytes,
}

fn import_module_type(attrs: &[(String, String)]) -> Option<ImportModuleType> {
    for (key, value) in attrs {
        if key == "type" {
            return match value.as_str() {
                "text" => Some(ImportModuleType::Text),
                "bytes" => Some(ImportModuleType::Bytes),
                _ => None,
            };
        }
    }
    None
}

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
    gc_requested: bool,
    gc_bytes_since_gc: usize,
    gc_external_bytes: usize,
    gc_threshold_bytes: usize,
    generator_context: Option<GeneratorContext>,
    pub(crate) destructuring_yield: bool,
    pub(crate) pending_iter_close: Vec<JsValue>,
    pub(crate) generator_inline_iters: HashMap<u64, Vec<JsValue>>,
    microtask_queue: Vec<(
        Vec<JsValue>,
        Box<dyn FnOnce(&mut Interpreter) -> Completion>,
    )>,
    cached_has_instance_key: Option<String>,
    module_registry: HashMap<PathBuf, Rc<RefCell<LoadedModule>>>,
    synthetic_module_registry: HashMap<(PathBuf, ImportModuleType), Rc<RefCell<LoadedModule>>>,
    current_module_path: Option<PathBuf>,
    loading_deferred: bool,
    last_call_had_explicit_return: bool,
    last_call_this_value: Option<JsValue>,
    constructing_derived: bool,
    calling_as_construct: bool,
    pub(crate) call_stack_envs: Vec<EnvRef>,
    pub(crate) call_stack_frames: Vec<CallFrame>,
    pub(crate) gc_temp_roots: Vec<u64>,
    // microtask roots are now stored inline in the microtask_queue tuples
    pub(crate) class_private_names: Vec<std::collections::HashMap<String, String>>,
    next_class_brand_id: u64,
    next_auto_accessor_id: u64,
    pub(crate) regexp_legacy_input: String,
    pub(crate) regexp_legacy_last_match: String,
    pub(crate) regexp_legacy_last_paren: String,
    pub(crate) regexp_legacy_left_context: String,
    pub(crate) regexp_legacy_right_context: String,
    pub(crate) regexp_legacy_parens: [String; 9],
    pub(crate) regexp_constructor_id: Option<u64>,
    pub(crate) function_realm_map: HashMap<u64, usize>,
    pub(crate) in_tail_position: bool,
    pub(crate) in_state_machine: bool,
    pub(crate) next_function_is_method: bool,
    pub(crate) is_agent_thread: bool,
    pub(crate) can_block: bool,
    pub(crate) agent_reports: std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<String>>>,
    pub(crate) agent_broadcast_txs: Vec<std::sync::mpsc::Sender<AgentBroadcastMsg>>,
    pub(crate) agent_handles: Vec<std::thread::JoinHandle<()>>,
    pub(crate) agent_broadcast_rx: Option<std::sync::mpsc::Receiver<AgentBroadcastMsg>>,
    pub(crate) agent_async_completions: std::sync::Arc<(
        std::sync::Mutex<Vec<Box<dyn FnOnce(&mut Interpreter) + Send>>>,
        std::sync::Condvar,
    )>,
    pub(crate) iterator_next_cache: HashMap<u64, JsValue>,
    last_identifier_with_base: Option<u64>,
    pub(crate) async_gen_queues: HashMap<u64, std::collections::VecDeque<AsyncGenRequest>>,
    pub(crate) async_gen_yield_pending: bool,
    pub(crate) async_function_states: HashMap<u64, AsyncFunctionState>,
    next_async_function_id: u64,
    pub(crate) pending_async_dispose_await: bool,
    pub(crate) static_module_load_depth: u32,
    module_async_evaluation_count: u64,
    module_async_info: HashMap<u64, PathBuf>,
}

pub(crate) struct CallFrame {
    pub func_obj_id: u64,
    pub arguments_obj: JsValue,
    pub is_eval: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct AsyncGenRequest {
    pub kind: AsyncGenRequestKind,
    pub value: JsValue,
    pub promise: JsValue,
    pub resolve_fn: JsValue,
    pub reject_fn: JsValue,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AsyncGenRequestKind {
    Next,
    Return,
    Throw,
}

pub struct LoadedModule {
    pub path: PathBuf,
    pub env: EnvRef,
    pub exports: HashMap<String, JsValue>,
    pub export_bindings: HashMap<String, String>, // export_name -> binding_name
    pub cached_namespace: Option<JsValue>, // cached namespace object (same identity on re-import)
    pub cached_deferred_namespace: Option<JsValue>, // cached deferred namespace (separate from eager)
    pub cached_import_meta: Option<JsValue>,        // cached import.meta object per §16.2.1.5.2
    pub error: Option<JsValue>,                     // if module evaluation threw, the error
    pub namespace_imports: HashMap<String, PathBuf>, // local_name -> source module path (for `import * as ns`)
    pub star_export_sources: Vec<String>,            // source specifiers from `export * from '...'`
    pub evaluated: bool,
    pub is_evaluating: bool,
    pub has_tla: bool,
    pub deferred_only: bool, // loaded via load_module_no_eval, not yet fully loaded
    pub program_ast: Option<crate::ast::Program>,
    pub async_evaluation_order: Option<u64>,
    pub pending_async_dependencies: u32,
    pub async_parent_modules: Vec<PathBuf>,
    pub cycle_root: Option<PathBuf>,
    pub top_level_capability: Option<(JsValue, JsValue, JsValue)>,
    pub dfs_index: Option<u32>,
    pub dfs_ancestor_index: Option<u32>,
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
                        kind: BindingKind::ImmutableValue,
                        initialized: true,
                        deletable: false,
                    },
                );
            }
        }

        let realm = Realm::new(global);

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
            gc_requested: false,
            gc_bytes_since_gc: 0,
            gc_external_bytes: 0,
            gc_threshold_bytes: GC_INITIAL_THRESHOLD_BYTES,
            generator_context: None,
            destructuring_yield: false,
            pending_iter_close: Vec::new(),
            generator_inline_iters: HashMap::new(),
            microtask_queue: Vec::new(),
            cached_has_instance_key: None,
            module_registry: HashMap::new(),
            synthetic_module_registry: HashMap::new(),
            current_module_path: None,
            loading_deferred: false,
            last_call_had_explicit_return: false,
            last_call_this_value: None,
            constructing_derived: false,
            calling_as_construct: false,
            call_stack_envs: Vec::new(),
            call_stack_frames: Vec::new(),
            gc_temp_roots: Vec::new(),
            class_private_names: Vec::new(),
            next_class_brand_id: 0,
            next_auto_accessor_id: 0,
            regexp_legacy_input: String::new(),
            regexp_legacy_last_match: String::new(),
            regexp_legacy_last_paren: String::new(),
            regexp_legacy_left_context: String::new(),
            regexp_legacy_right_context: String::new(),
            regexp_legacy_parens: Default::default(),
            regexp_constructor_id: None,
            function_realm_map: HashMap::new(),
            in_tail_position: false,
            in_state_machine: false,
            next_function_is_method: false,
            is_agent_thread: false,
            can_block: false,
            agent_reports: Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new())),
            agent_broadcast_txs: Vec::new(),
            agent_handles: Vec::new(),
            agent_broadcast_rx: None,
            agent_async_completions: Arc::new((
                std::sync::Mutex::new(Vec::new()),
                std::sync::Condvar::new(),
            )),
            iterator_next_cache: HashMap::new(),
            last_identifier_with_base: None,
            async_gen_queues: HashMap::new(),
            async_gen_yield_pending: false,
            async_function_states: HashMap::new(),
            next_async_function_id: 0,
            pending_async_dispose_await: false,
            static_module_load_depth: 0,
            module_async_evaluation_count: 0,
            module_async_info: HashMap::new(),
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
                        kind: BindingKind::ImmutableValue,
                        initialized: true,
                        deletable: false,
                    },
                );
            }
        }
        let realm = Realm::new(new_global_env);
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
                interp.gc_requested = true;
                interp.gc_safepoint();
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

        // $262.agent
        let agent_obj = self.create_object();
        let agent_obj_id = agent_obj.borrow().id.unwrap();

        // $262.agent.start(script)
        let _reports_clone = self.agent_reports.clone();
        let start_fn = self.create_function(JsFunction::native(
            "start".to_string(),
            1,
            move |interp, _this, args| {
                let script_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let script = match interp.to_string_value(&script_val) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };

                let (tx, rx) = std::sync::mpsc::channel::<AgentBroadcastMsg>();
                interp.agent_broadcast_txs.push(tx);

                let reports = interp.agent_reports.clone();
                let handle = std::thread::spawn(move || {
                    let mut agent_interp = Interpreter::new();
                    agent_interp.is_agent_thread = true;
                    agent_interp.can_block = true;
                    agent_interp.agent_reports = reports;
                    agent_interp.agent_broadcast_rx = Some(rx);

                    // Set up agent-side $262.agent on the global
                    setup_agent_side_262(&mut agent_interp);

                    let mut p = match crate::parser::Parser::new(&script) {
                        Ok(p) => p,
                        Err(_) => return,
                    };
                    let program = match p.parse_program() {
                        Ok(prog) => prog,
                        Err(_) => return,
                    };
                    let _ = agent_interp.run(&program);
                    agent_interp.drain_microtasks_blocking();
                });
                interp.agent_handles.push(handle);

                Completion::Normal(JsValue::Undefined)
            },
        ));
        agent_obj
            .borrow_mut()
            .insert_builtin("start".to_string(), start_fn);

        // $262.agent.broadcast(sab)
        let broadcast_fn = self.create_function(JsFunction::native(
            "broadcast".to_string(),
            1,
            |interp, _this, args| {
                let sab_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                if let JsValue::Object(o) = &sab_val
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let sab_shared = obj.borrow().sab_shared.clone();
                    if let Some(inner) = sab_shared {
                        for tx in &interp.agent_broadcast_txs {
                            let _ = tx.send(AgentBroadcastMsg {
                                sab_shared: inner.clone(),
                            });
                        }
                    }
                }
                Completion::Normal(JsValue::Undefined)
            },
        ));
        agent_obj
            .borrow_mut()
            .insert_builtin("broadcast".to_string(), broadcast_fn);

        // $262.agent.getReport()
        let get_report_fn = self.create_function(JsFunction::native(
            "getReport".to_string(),
            0,
            |interp, _this, _args| {
                let mut reports = interp.agent_reports.lock().unwrap();
                if let Some(report) = reports.pop_front() {
                    Completion::Normal(JsValue::String(JsString::from_str(&report)))
                } else {
                    Completion::Normal(JsValue::Null)
                }
            },
        ));
        agent_obj
            .borrow_mut()
            .insert_builtin("getReport".to_string(), get_report_fn);

        // $262.agent.sleep(ms)
        let sleep_fn = self.create_function(JsFunction::native(
            "sleep".to_string(),
            1,
            |interp, _this, args| {
                let ms_val = args.first().cloned().unwrap_or(JsValue::Undefined);
                let ms = match interp.to_number_value(&ms_val) {
                    Ok(n) => n.max(0.0) as u64,
                    Err(e) => return Completion::Throw(e),
                };
                std::thread::sleep(std::time::Duration::from_millis(ms));
                Completion::Normal(JsValue::Undefined)
            },
        ));
        agent_obj
            .borrow_mut()
            .insert_builtin("sleep".to_string(), sleep_fn);

        // $262.agent.monotonicNow()
        let start_time = std::time::Instant::now();
        let monotonic_fn = self.create_function(JsFunction::native(
            "monotonicNow".to_string(),
            0,
            move |_interp, _this, _args| {
                let elapsed = start_time.elapsed().as_millis() as f64;
                Completion::Normal(JsValue::Number(elapsed))
            },
        ));
        agent_obj
            .borrow_mut()
            .insert_builtin("monotonicNow".to_string(), monotonic_fn);

        // $262.agent.leaving()
        let leaving_fn = self.create_function(JsFunction::native(
            "leaving".to_string(),
            0,
            |_interp, _this, _args| Completion::Normal(JsValue::Undefined),
        ));
        agent_obj
            .borrow_mut()
            .insert_builtin("leaving".to_string(), leaving_fn);

        let agent_val = JsValue::Object(crate::types::JsObject { id: agent_obj_id });
        dollar_262
            .borrow_mut()
            .insert_builtin("agent".to_string(), agent_val);

        // $262.AbstractModuleSource — §28.1.1.1
        let ams_fn = self.create_function(JsFunction::native(
            "AbstractModuleSource".to_string(),
            0,
            |interp, _this, _args| {
                Completion::Throw(
                    interp.create_error("TypeError", "AbstractModuleSource is not a constructor"),
                )
            },
        ));
        let ams_fn_id = if let JsValue::Object(o) = &ams_fn {
            o.id
        } else {
            unreachable!()
        };

        // AbstractModuleSource.prototype
        let ams_proto = self.create_object();
        // constructor property
        ams_proto
            .borrow_mut()
            .insert_builtin("constructor".to_string(), ams_fn.clone());
        // @@toStringTag getter — returns undefined (no [[ModuleSourceClassName]] slot)
        let tag_getter = self.create_function(JsFunction::native(
            "get [Symbol.toStringTag]".to_string(),
            0,
            |_interp, this_val, _args| {
                if let JsValue::Object(_) = this_val {
                    return Completion::Normal(JsValue::Undefined);
                }
                Completion::Normal(JsValue::Undefined)
            },
        ));
        ams_proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(tag_getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );
        let ams_proto_val = JsValue::Object(crate::types::JsObject {
            id: ams_proto.borrow().id.unwrap(),
        });

        // Wire prototype on the constructor: {writable: false, enumerable: false, configurable: false}
        if let Some(obj) = self.get_object(ams_fn_id) {
            obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(ams_proto_val, false, false, false),
            );
        }

        dollar_262
            .borrow_mut()
            .insert_builtin("AbstractModuleSource".to_string(), ams_fn);

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

    pub(crate) fn gc_unroot_args(&mut self, args: &[JsValue]) {
        for v in args {
            self.gc_unroot_value(v);
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
    pub(crate) fn get_function_realm(&mut self, func_val: &JsValue) -> Result<usize, JsValue> {
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
            // Proxy: §7.3.22 step 4
            if obj_ref.proxy_revoked {
                drop(obj_ref);
                return Err(self.create_type_error("Cannot perform operation on a revoked proxy"));
            }
            if let Some(ref target) = obj_ref.proxy_target {
                let target_id = target.borrow().id.unwrap();
                drop(obj_ref);
                return self.get_function_realm(&JsValue::Object(crate::types::JsObject {
                    id: target_id,
                }));
            }
            drop(obj_ref);
            // Check function_realm_map
            if let Some(&realm_id) = self.function_realm_map.get(&o.id) {
                return Ok(realm_id);
            }
        }
        Ok(self.current_realm_id)
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
            let nt_realm_id = self.get_function_realm(&JsValue::Object(nt_o.clone()))?;
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
                    let has = match self.proxy_has_property(obj_id, $key) {
                        Ok(b) => b,
                        Err(e) => return Err(Some(e)),
                    };
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

            // §6.2.6.5 ToPropertyDescriptor — spec-mandated order
            check_field!(desc, "enumerable", |v: JsValue| desc.enumerable =
                Some(self.to_boolean_val(&v)));
            check_field!(desc, "configurable", |v: JsValue| desc.configurable =
                Some(self.to_boolean_val(&v)));
            check_field!(desc, "value", |v: JsValue| desc.value = Some(v));
            check_field!(desc, "writable", |v: JsValue| desc.writable =
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

    fn same_value_option(a: Option<&JsValue>, b: Option<&JsValue>) -> bool {
        match (a, b) {
            (Some(a), Some(b)) => crate::interpreter::helpers::same_value(a, b),
            (None, None) => true,
            _ => false,
        }
    }

    #[allow(clippy::wrong_self_convention)]
    pub(crate) fn from_property_descriptor(&mut self, desc: &PropertyDescriptor) -> JsValue {
        let result = self.create_object();
        {
            let mut r = result.borrow_mut();
            // §6.2.6.4 FromPropertyDescriptor — only include fields that are present
            if let Some(ref val) = desc.value {
                r.insert_value("value".to_string(), val.clone());
            }
            if let Some(w) = desc.writable {
                r.insert_value("writable".to_string(), JsValue::Boolean(w));
            }
            if let Some(ref getter) = desc.get {
                r.insert_value("get".to_string(), getter.clone());
            }
            if let Some(ref setter) = desc.set {
                r.insert_value("set".to_string(), setter.clone());
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
        if let JsValue::Object(o) = val
            && let Some(Some(obj)) = self.objects.get(o.id as usize)
            && obj.borrow().is_htmldda
        {
            return false;
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
        let realm_id = self.current_realm_id;
        let func = JsFunction::native(
            String::new(),
            0,
            move |interp: &mut Interpreter, _this: &JsValue, _args: &[JsValue]| {
                let err = interp.create_error_in_realm(
                    realm_id,
                    "TypeError",
                    "'caller', 'callee', and 'arguments' properties may not be accessed on strict mode functions or the arguments objects for calls to them",
                );
                Completion::Throw(err)
            },
        );
        self.create_function(func)
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
            self.realm()
                .async_generator_function_prototype
                .clone()
                .or(self.realm().object_prototype.clone())
        } else if is_gen {
            self.realm()
                .generator_function_prototype
                .clone()
                .or(self.realm().object_prototype.clone())
        } else if is_async_non_gen {
            self.realm()
                .async_function_prototype
                .clone()
                .or(self.realm().object_prototype.clone())
        } else {
            self.realm()
                .function_prototype
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
        // own caller/arguments to shadow the ThrowTypeError accessor.
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
            if let Some(ref getter) = self.realm().sloppy_caller_getter.clone() {
                obj_data.insert_property(
                    "caller".to_string(),
                    PropertyDescriptor::accessor(Some(getter.clone()), None, false, true),
                );
            } else {
                obj_data.insert_property(
                    "caller".to_string(),
                    PropertyDescriptor::data(JsValue::Null, false, false, true),
                );
            }
            if let Some(ref getter) = self.realm().sloppy_arguments_getter.clone() {
                obj_data.insert_property(
                    "arguments".to_string(),
                    PropertyDescriptor::accessor(Some(getter.clone()), None, false, true),
                );
            } else {
                obj_data.insert_property(
                    "arguments".to_string(),
                    PropertyDescriptor::data(JsValue::Null, false, false, true),
                );
            }
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
        self.function_realm_map
            .insert(func_id, self.current_realm_id);
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
                let mut mapped_names: HashSet<&str> = HashSet::new();
                for i in (0..param_names.len()).rev() {
                    let name = &param_names[i];
                    if mapped_names.contains(name.as_str()) {
                        continue;
                    }
                    mapped_names.insert(name.as_str());
                    if i < args.len() {
                        map.insert(i.to_string(), (env.clone(), name.clone()));
                    }
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
                .realm()
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

        // Add Symbol.iterator = %Array.prototype.values% (spec §10.4.4.6 step 20 / §10.4.4.7 step 22)
        // Must be the exact same function object as Array.prototype[@@iterator]
        if let Some(key) = self.get_symbol_iterator_key() {
            let array_iter_fn = self
                .realm()
                .array_prototype
                .as_ref()
                .map(|proto| proto.borrow().get_property(&key));
            if let Some(iter_fn) = array_iter_fn
                && !matches!(iter_fn, JsValue::Undefined)
                && let JsValue::Object(ref o) = result
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
        self.gc_safepoint();
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
        self.gc_safepoint();
        match program.source_type {
            SourceType::Script => {
                let prev = self.current_module_path.take();
                self.current_module_path = Some(path.to_path_buf());
                let global = self.realm().global_env.clone();
                if program.body_is_strict {
                    global.borrow_mut().strict = true;
                }
                let r = self.exec_statements(&program.body, &global);
                // Drain microtasks before restoring path so async callbacks can use relative imports
                self.drain_microtasks();
                self.current_module_path = prev;
                r
            }
            SourceType::Module => {
                let r = self.run_module(program, Some(path.to_path_buf()));
                // Keep path set during microtask draining so async callbacks can use relative imports
                let prev = self.current_module_path.take();
                self.current_module_path = Some(path.to_path_buf());
                self.drain_microtasks();
                self.current_module_path = prev;
                r
            }
        }
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
        let has_tla_entry = Self::module_has_tla(program);
        let loaded_module = Rc::new(RefCell::new(LoadedModule {
            path: canon_path_entry.clone(),
            env: module_env.clone(),
            exports: HashMap::new(),
            export_bindings: HashMap::new(),
            cached_namespace: None,
            cached_deferred_namespace: None,
            cached_import_meta: None,
            error: None,
            namespace_imports: HashMap::new(),
            star_export_sources: Vec::new(),
            evaluated: false,
            is_evaluating: false,
            deferred_only: false,
            has_tla: has_tla_entry,
            program_ast: Some(program.clone()),
            async_evaluation_order: None,
            pending_async_dependencies: 0,
            async_parent_modules: Vec::new(),
            cycle_root: None,
            top_level_capability: None,
            dfs_index: None,
            dfs_ancestor_index: None,
        }));
        if let Some(ref path) = module_path {
            let canon_path = path.canonicalize().unwrap_or_else(|_| path.clone());
            self.module_registry
                .insert(canon_path, loaded_module.clone());
        }
        // Note: is_evaluating is managed by inner_module_evaluation

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
                if let ExportDeclaration::All {
                    source,
                    exported: None,
                } = export
                {
                    loaded_module
                        .borrow_mut()
                        .star_export_sources
                        .push(source.clone());
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

        // Pre-load pass: load ALL referenced modules in source order (§16.2.1.6.2 step 6)
        // For deferred imports, load without evaluation.
        for item in &program.module_items {
            let (specifier, is_deferred, import_type) = match item {
                ModuleItem::ImportDeclaration(import) => {
                    let is_defer = import
                        .specifiers
                        .iter()
                        .any(|s| matches!(s, ImportSpecifier::DeferredNamespace(_)));
                    (
                        Some(import.source.as_str()),
                        is_defer,
                        import_module_type(&import.attributes),
                    )
                }
                ModuleItem::ExportDeclaration(ExportDeclaration::All { source, .. }) => {
                    (Some(source.as_str()), false, None)
                }
                ModuleItem::ExportDeclaration(ExportDeclaration::Named {
                    source: Some(source),
                    ..
                }) => (Some(source.as_str()), false, None),
                _ => (None, false, None),
            };
            if let Some(spec) = specifier {
                let module_path = self.current_module_path.clone();
                if let Ok(resolved) = self.resolve_module_specifier(spec, module_path.as_deref()) {
                    match import_type {
                        Some(ImportModuleType::Text) => {
                            if let Err(e) = self.load_text_module(&resolved) {
                                self.current_module_path = prev_module_path;
                                return Completion::Throw(e);
                            }
                        }
                        Some(ImportModuleType::Bytes) => {
                            if let Err(e) = self.load_bytes_module(&resolved) {
                                self.current_module_path = prev_module_path;
                                return Completion::Throw(e);
                            }
                        }
                        None if is_deferred => {
                            if let Err(e) = self.load_module_no_eval(&resolved) {
                                self.current_module_path = prev_module_path;
                                return Completion::Throw(e);
                            }
                        }
                        None => {
                            let _ = self.load_module(&resolved);
                        }
                    }
                }
            }
        }

        // Second pass: process re-exports (export * from) — before imports
        // so that self-importing namespaces include star re-exported keys
        for item in &program.module_items {
            if let ModuleItem::ExportDeclaration(ExportDeclaration::All { source, exported }) = item
                && let Err(e) =
                    self.process_star_reexport(source, exported.as_ref(), &loaded_module)
            {
                self.current_module_path = prev_module_path;
                return Completion::Throw(e);
            }
        }

        // Third pass: process imports (after hoisting and re-exports)
        for item in &program.module_items {
            if let ModuleItem::ImportDeclaration(import) = item
                && let Err(e) = self.process_import(import, &module_env)
            {
                self.current_module_path = prev_module_path;
                return Completion::Throw(e);
            }
        }

        // Validate named re-exports (export { x } from './mod')
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

        // Fourth pass: evaluate via inner_module_evaluation (Tarjan SCC + async protocol)
        let mut stack = vec![];
        if let Err(ref e) = self.inner_module_evaluation(&canon_path_entry, &mut stack, 0) {
            // Per spec §16.2.1.5.3 step 9: mark all modules on stack as evaluated with error
            for m_path in &stack {
                if let Some(m) = self.module_registry.get(m_path) {
                    let mut mb = m.borrow_mut();
                    mb.evaluated = true;
                    mb.is_evaluating = false;
                    if mb.error.is_none() {
                        mb.error = Some(e.clone());
                    }
                }
            }
            self.current_module_path = prev_module_path;
            return Completion::Throw(e.clone());
        }

        // Drain microtasks to complete TLA module evaluation
        self.drain_microtasks();

        // Check if the entry module has an error (e.g. from rejected TLA await)
        if let Some(module) = self.module_registry.get(&canon_path_entry).cloned()
            && let Some(err) = module.borrow().error.clone()
        {
            self.current_module_path = prev_module_path;
            return Completion::Throw(err);
        }

        self.current_module_path = prev_module_path;
        Completion::Normal(JsValue::Undefined)
    }

    fn process_import(&mut self, import: &ImportDeclaration, env: &EnvRef) -> Result<(), JsValue> {
        let module_path = self.current_module_path.clone();
        let resolved = self.resolve_module_specifier(&import.source, module_path.as_deref())?;

        let itype = import_module_type(&import.attributes);

        // Text/bytes imports use synthetic module registry
        if let Some(ref it) = itype {
            let canon = resolved.canonicalize().unwrap_or_else(|_| resolved.clone());
            let key = (canon, it.clone());
            let loaded = self
                .synthetic_module_registry
                .get(&key)
                .cloned()
                .ok_or_else(|| {
                    JsValue::String(JsString::from_str(&format!(
                        "Synthetic module not found for '{}'",
                        import.source
                    )))
                })?;
            for spec in &import.specifiers {
                match spec {
                    ImportSpecifier::Default(local) => {
                        let val = loaded
                            .borrow()
                            .exports
                            .get("default")
                            .cloned()
                            .unwrap_or(JsValue::Undefined);
                        env.borrow_mut().declare(local, BindingKind::Const);
                        env.borrow_mut().initialize_binding(local, val);
                    }
                    ImportSpecifier::Namespace(local) => {
                        let ns = self.create_module_namespace(&loaded);
                        env.borrow_mut().declare(local, BindingKind::Const);
                        env.borrow_mut().initialize_binding(local, ns);
                    }
                    _ => {}
                }
            }
            return Ok(());
        }

        let is_deferred = import
            .specifiers
            .iter()
            .any(|s| matches!(s, ImportSpecifier::DeferredNamespace(_)));

        // For deferred imports or when loading in deferred context,
        // use load_module_no_eval to avoid premature evaluation
        let loaded = if is_deferred || self.loading_deferred {
            self.load_module_no_eval(&resolved)?
        } else {
            self.load_module(&resolved)?
        };

        for spec in &import.specifiers {
            match spec {
                ImportSpecifier::Default(local) => {
                    self.create_import_binding_for(local, "default", &loaded, &resolved, env)?;
                }
                ImportSpecifier::Named { imported, local } => {
                    self.create_import_binding_for(local, imported, &loaded, &resolved, env)?;
                }
                ImportSpecifier::Namespace(local) => {
                    let ns = self.create_module_namespace(&loaded);
                    env.borrow_mut().declare(local, BindingKind::Const);
                    env.borrow_mut().initialize_binding(local, ns);
                    if let Some(ref mp) = self.current_module_path {
                        let canon = mp.canonicalize().unwrap_or_else(|_| mp.clone());
                        if let Some(current_mod) = self.module_registry.get(&canon) {
                            current_mod
                                .borrow_mut()
                                .namespace_imports
                                .insert(local.clone(), resolved.clone());
                        }
                    }
                }
                ImportSpecifier::DeferredNamespace(local) => {
                    let ns = self.create_deferred_module_namespace(&loaded);
                    env.borrow_mut().declare(local, BindingKind::Const);
                    env.borrow_mut().initialize_binding(local, ns);
                }
                ImportSpecifier::SourcePhase(_) => {
                    return Err(self.create_type_error(
                        "Source phase imports are not supported for Source Text Module Records",
                    ));
                }
            }
        }

        Ok(())
    }

    fn create_import_binding_for(
        &mut self,
        local: &str,
        imported: &str,
        loaded: &Rc<RefCell<LoadedModule>>,
        resolved: &Path,
        env: &EnvRef,
    ) -> Result<(), JsValue> {
        let binding_info = loaded.borrow().export_bindings.get(imported).cloned();
        if let Some(ref binding) = binding_info
            && binding.starts_with("*ns:")
        {
            // Namespace re-export: copy the namespace value directly
            if let Some(val) = loaded.borrow().exports.get(imported).cloned() {
                env.borrow_mut().declare(local, BindingKind::Const);
                env.borrow_mut().initialize_binding(local, val);
            }
            return Ok(());
        }
        let has_export = binding_info.is_some();
        if has_export {
            let mut visited = std::collections::HashSet::new();
            match self.resolve_export_binding(resolved, imported, &mut visited) {
                Ok((source_env, binding_name)) => {
                    if binding_name == "*namespace*" {
                        // Resolved to a namespace — find the module and create its namespace object
                        let ns = self.create_namespace_for_env(&source_env);
                        env.borrow_mut().declare(local, BindingKind::Const);
                        env.borrow_mut().initialize_binding(local, ns);
                    } else {
                        env.borrow_mut()
                            .create_import_binding(local, source_env, binding_name);
                    }
                }
                Err(e) => return Err(e),
            }
        } else {
            return Err(JsValue::String(JsString::from_str(&format!(
                "Module '{}' has no export named '{}'",
                loaded.borrow().path.display(),
                imported
            ))));
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
            module.borrow_mut().exports.insert(name.clone(), ns.clone());
            module
                .borrow_mut()
                .export_bindings
                .insert(name.clone(), format!("*ns:{}", source));
            // Store in module env so indirect bindings can reference it
            let mod_env = module.borrow().env.clone();
            mod_env.borrow_mut().declare(name, BindingKind::Const);
            mod_env.borrow_mut().initialize_binding(name, ns);
        } else {
            // export * from './mod' - re-export all non-default exports
            let source_exports = source_module.borrow().exports.clone();
            let module_path = self.current_module_path.clone();
            for (export_name, val) in source_exports {
                if export_name != "default" {
                    let existing_binding =
                        module.borrow().export_bindings.get(&export_name).cloned();
                    let new_reexport = format!("*reexport:{}:{}", source, export_name);
                    if let Some(ref existing) = existing_binding {
                        if existing == "*ambiguous*" {
                            continue;
                        }
                        // Local/indirect exports take precedence over star exports
                        if !existing.starts_with("*reexport:") {
                            continue;
                        }
                        // §16.2.1.6.3 step 8d.ii: two star exports with same name
                        // — check if they resolve to the same (module, binding)
                        if existing != &new_reexport {
                            let is_ambiguous = if let Some(ref mp) = module_path {
                                let mut v1 = std::collections::HashSet::new();
                                let r1 = self.resolve_export_binding(mp, &export_name, &mut v1);
                                module
                                    .borrow_mut()
                                    .export_bindings
                                    .insert(export_name.clone(), new_reexport.clone());
                                let mut v2 = std::collections::HashSet::new();
                                let r2 = self.resolve_export_binding(mp, &export_name, &mut v2);
                                module
                                    .borrow_mut()
                                    .export_bindings
                                    .insert(export_name.clone(), existing.clone());
                                match (r1, r2) {
                                    (Ok((env1, name1)), Ok((env2, name2))) => {
                                        !std::rc::Rc::ptr_eq(&env1, &env2) || name1 != name2
                                    }
                                    _ => true,
                                }
                            } else {
                                true
                            };
                            if is_ambiguous {
                                module.borrow_mut().exports.remove(&export_name);
                                module
                                    .borrow_mut()
                                    .export_bindings
                                    .insert(export_name.clone(), "*ambiguous*".to_string());
                            }
                            continue;
                        }
                    }
                    module.borrow_mut().exports.insert(export_name.clone(), val);
                    module
                        .borrow_mut()
                        .export_bindings
                        .insert(export_name.clone(), new_reexport);
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
        self.static_module_load_depth += 1;
        let result = self.load_module_inner(path);
        self.static_module_load_depth -= 1;
        result
    }

    fn load_module_inner(&mut self, path: &Path) -> Result<Rc<RefCell<LoadedModule>>, JsValue> {
        let canon_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        // Check if module is already loaded
        if let Some(existing) = self.module_registry.get(&canon_path).cloned() {
            // If the module previously errored, re-throw the same error
            if let Some(ref err) = existing.borrow().error.clone() {
                return Err(err.clone());
            }
            // If loaded via load_module_no_eval (deferred), evaluate it now
            // since this is a non-deferred import
            if existing.borrow().deferred_only {
                existing.borrow_mut().deferred_only = false;
            }
            return Ok(existing);
        }

        // Handle JSON modules: parse JSON and expose as default export
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let source = std::fs::read_to_string(path).map_err(|e| {
                JsValue::String(JsString::from_str(&format!(
                    "Cannot read module '{}': {}",
                    path.display(),
                    e
                )))
            })?;
            let parsed = match crate::interpreter::helpers::json_parse_value(self, &source) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _other => {
                    return Err(JsValue::String(JsString::from_str(&format!(
                        "JSON parse error in '{}'",
                        path.display()
                    ))));
                }
            };
            let module_env = Environment::new_function_scope(Some(self.realm().global_env.clone()));
            module_env.borrow_mut().strict = true;
            let loaded_module = Rc::new(RefCell::new(LoadedModule {
                path: canon_path.clone(),
                env: module_env.clone(),
                exports: {
                    let mut m = HashMap::new();
                    m.insert("default".to_string(), parsed.clone());
                    m
                },
                export_bindings: {
                    let mut m = HashMap::new();
                    m.insert("default".to_string(), "*default*".to_string());
                    m
                },
                cached_namespace: None,
                cached_deferred_namespace: None,
                cached_import_meta: None,
                error: None,
                namespace_imports: HashMap::new(),
                star_export_sources: Vec::new(),
                evaluated: true,
                is_evaluating: false,
                deferred_only: false,
                has_tla: false,
                program_ast: None,
                async_evaluation_order: None,
                pending_async_dependencies: 0,
                async_parent_modules: Vec::new(),
                cycle_root: None,
                top_level_capability: None,
                dfs_index: None,
                dfs_ancestor_index: None,
            }));
            module_env
                .borrow_mut()
                .declare("*default*", BindingKind::Const);
            module_env
                .borrow_mut()
                .initialize_binding("*default*", parsed);
            self.module_registry
                .insert(canon_path.clone(), loaded_module.clone());
            return Ok(loaded_module);
        }

        // Read and parse the module
        let source = std::fs::read_to_string(path).map_err(|e| {
            JsValue::String(JsString::from_str(&format!(
                "Cannot read module '{}': {}",
                path.display(),
                e
            )))
        })?;

        let mut parser = match parser::Parser::new(&source) {
            Ok(p) => p,
            Err(e) => {
                return Err(self.create_error(
                    "SyntaxError",
                    &format!("Parse error in '{}': {:?}", path.display(), e),
                ));
            }
        };

        let program = match parser.parse_program_as_module() {
            Ok(p) => p,
            Err(e) => {
                return Err(self.create_error("SyntaxError", &e.message.to_string()));
            }
        };

        // Create module environment
        let module_env = Environment::new_function_scope(Some(self.realm().global_env.clone()));
        module_env.borrow_mut().strict = true;
        module_env.borrow_mut().module_path = Some(canon_path.clone());
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
            cached_namespace: None,
            cached_deferred_namespace: None,
            cached_import_meta: None,
            error: None,
            namespace_imports: HashMap::new(),
            star_export_sources: Vec::new(),
            evaluated: false,
            is_evaluating: false,
            deferred_only: false,
            has_tla: false,
            program_ast: None,
            async_evaluation_order: None,
            pending_async_dependencies: 0,
            async_parent_modules: Vec::new(),
            cycle_root: None,
            top_level_capability: None,
            dfs_index: None,
            dfs_ancestor_index: None,
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
                if let ExportDeclaration::All {
                    source,
                    exported: None,
                } = export
                {
                    loaded_module
                        .borrow_mut()
                        .star_export_sources
                        .push(source.clone());
                }
            }
        }

        // Detect top-level await
        let has_tla = Self::module_has_tla(&program);
        loaded_module.borrow_mut().has_tla = has_tla;

        // Store AST for deferred evaluation
        loaded_module.borrow_mut().program_ast = Some(program.clone());

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

        // Pre-load pass: load ALL referenced modules in source order (§16.2.1.6.2 step 6)
        // For deferred imports, load without evaluation.
        // For non-deferred, load normally (which includes evaluation).
        for item in &program.module_items {
            let (specifier, is_deferred, itype) = match item {
                ModuleItem::ImportDeclaration(import) => {
                    let is_defer = import
                        .specifiers
                        .iter()
                        .any(|s| matches!(s, ImportSpecifier::DeferredNamespace(_)));
                    (
                        Some(import.source.as_str()),
                        is_defer,
                        import_module_type(&import.attributes),
                    )
                }
                ModuleItem::ExportDeclaration(ExportDeclaration::All { source, .. }) => {
                    (Some(source.as_str()), false, None)
                }
                ModuleItem::ExportDeclaration(ExportDeclaration::Named {
                    source: Some(source),
                    ..
                }) => (Some(source.as_str()), false, None),
                _ => (None, false, None),
            };
            if let Some(spec) = specifier {
                let module_path = self.current_module_path.clone();
                if let Ok(resolved) = self.resolve_module_specifier(spec, module_path.as_deref()) {
                    match itype {
                        Some(ImportModuleType::Text) => {
                            self.load_text_module(&resolved)?;
                        }
                        Some(ImportModuleType::Bytes) => {
                            self.load_bytes_module(&resolved)?;
                        }
                        None if is_deferred => {
                            self.load_module_no_eval(&resolved)?;
                        }
                        None => {
                            let _ = self.load_module(&resolved);
                        }
                    }
                }
            }
        }

        // Second pass: process re-exports (export * from) — before imports
        // so that self-importing namespaces include star re-exported keys
        for item in &program.module_items {
            if let ModuleItem::ExportDeclaration(ExportDeclaration::All { source, exported }) = item
            {
                self.process_star_reexport(source, exported.as_ref(), &loaded_module)?;
            }
        }

        // Third pass: process imports (after re-exports)
        for item in &program.module_items {
            if let ModuleItem::ImportDeclaration(import) = item {
                self.process_import(import, &module_env)?;
            }
        }

        // Validate named re-exports (export { x } from './mod')
        {
            let canon = canon_path.clone();
            for item in &program.module_items {
                if let ModuleItem::ExportDeclaration(ExportDeclaration::Named {
                    specifiers,
                    source: Some(source),
                    ..
                }) = item
                    && let Err(e) = self.validate_named_reexports(&canon, source, specifiers)
                {
                    self.current_module_path = prev_path;
                    loaded_module.borrow_mut().error = Some(e.clone());
                    return Err(e);
                }
            }
        }

        // No evaluation — handled by inner_module_evaluation

        self.current_module_path = prev_path;
        Ok(loaded_module)
    }

    /// Load a module without evaluating it (for deferred imports).
    /// Parses, links, resolves exports, but does NOT execute the module body.
    fn load_module_no_eval(&mut self, path: &Path) -> Result<Rc<RefCell<LoadedModule>>, JsValue> {
        let canon_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        if let Some(existing) = self.module_registry.get(&canon_path) {
            return Ok(existing.clone());
        }

        // JSON modules are always fully evaluated
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            return self.load_module(path);
        }

        let source = std::fs::read_to_string(path).map_err(|e| {
            JsValue::String(JsString::from_str(&format!(
                "Cannot read module '{}': {}",
                path.display(),
                e
            )))
        })?;

        let mut parser = match parser::Parser::new(&source) {
            Ok(p) => p,
            Err(e) => {
                return Err(self.create_error(
                    "SyntaxError",
                    &format!("Parse error in '{}': {:?}", path.display(), e),
                ));
            }
        };

        let program = match parser.parse_program_as_module() {
            Ok(p) => p,
            Err(e) => {
                return Err(self.create_error("SyntaxError", &e.message.to_string()));
            }
        };

        let module_env = Environment::new_function_scope(Some(self.realm().global_env.clone()));
        module_env.borrow_mut().strict = true;
        module_env.borrow_mut().module_path = Some(canon_path.clone());
        {
            let mut env = module_env.borrow_mut();
            env.declare("this", BindingKind::Var);
        }

        let has_tla = Self::module_has_tla(&program);

        let loaded_module = Rc::new(RefCell::new(LoadedModule {
            path: canon_path.clone(),
            env: module_env.clone(),
            exports: HashMap::new(),
            export_bindings: HashMap::new(),
            cached_namespace: None,
            cached_deferred_namespace: None,
            cached_import_meta: None,
            error: None,
            namespace_imports: HashMap::new(),
            star_export_sources: Vec::new(),
            evaluated: false,
            is_evaluating: false,
            deferred_only: true,
            has_tla,
            program_ast: Some(program.clone()),
            async_evaluation_order: None,
            pending_async_dependencies: 0,
            async_parent_modules: Vec::new(),
            cycle_root: None,
            top_level_capability: None,
            dfs_index: None,
            dfs_ancestor_index: None,
        }));
        self.module_registry
            .insert(canon_path.clone(), loaded_module.clone());

        // Collect export names and bindings
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
                if let ExportDeclaration::All {
                    source,
                    exported: None,
                } = export
                {
                    loaded_module
                        .borrow_mut()
                        .star_export_sources
                        .push(source.clone());
                }
            }
        }

        let prev_path = self.current_module_path.take();
        self.current_module_path = Some(canon_path.clone());

        // Hoist declarations
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

        // Mark as loading deferred context so nested process_import uses load_module_no_eval
        let prev_loading_deferred = self.loading_deferred;
        self.loading_deferred = true;

        // Pre-load pass: load sub-dependencies
        for item in &program.module_items {
            let (specifier, itype) = match item {
                ModuleItem::ImportDeclaration(import) => (
                    Some(import.source.as_str()),
                    import_module_type(&import.attributes),
                ),
                ModuleItem::ExportDeclaration(ExportDeclaration::All { source, .. }) => {
                    (Some(source.as_str()), None)
                }
                ModuleItem::ExportDeclaration(ExportDeclaration::Named {
                    source: Some(source),
                    ..
                }) => (Some(source.as_str()), None),
                _ => (None, None),
            };
            if let Some(spec) = specifier {
                let module_path = self.current_module_path.clone();
                if let Ok(resolved) = self.resolve_module_specifier(spec, module_path.as_deref()) {
                    let result = match itype {
                        Some(ImportModuleType::Text) => self.load_text_module(&resolved),
                        Some(ImportModuleType::Bytes) => self.load_bytes_module(&resolved),
                        None => self.load_module_no_eval(&resolved),
                    };
                    if let Err(e) = result {
                        self.loading_deferred = prev_loading_deferred;
                        return Err(e);
                    }
                }
            }
        }

        // Process re-exports
        for item in &program.module_items {
            if let ModuleItem::ExportDeclaration(ExportDeclaration::All { source, exported }) = item
                && let Err(e) =
                    self.process_star_reexport(source, exported.as_ref(), &loaded_module)
            {
                self.loading_deferred = prev_loading_deferred;
                return Err(e);
            }
        }

        // Process imports
        for item in &program.module_items {
            if let ModuleItem::ImportDeclaration(import) = item
                && let Err(e) = self.process_import(import, &module_env)
            {
                self.loading_deferred = prev_loading_deferred;
                return Err(e);
            }
        }

        // Validate named re-exports
        {
            let canon = canon_path.clone();
            for item in &program.module_items {
                if let ModuleItem::ExportDeclaration(ExportDeclaration::Named {
                    specifiers,
                    source: Some(source),
                    ..
                }) = item
                    && let Err(e) = self.validate_named_reexports(&canon, source, specifiers)
                {
                    self.loading_deferred = prev_loading_deferred;
                    self.current_module_path = prev_path;
                    loaded_module.borrow_mut().error = Some(e.clone());
                    return Err(e);
                }
            }
        }

        // Async transitive deps are evaluated by inner_module_evaluation, not here

        self.loading_deferred = prev_loading_deferred;
        self.current_module_path = prev_path;
        Ok(loaded_module)
    }

    fn create_synthetic_default_module(
        &mut self,
        canon_path: PathBuf,
        value: JsValue,
    ) -> Rc<RefCell<LoadedModule>> {
        let module_env = Environment::new_function_scope(Some(self.realm().global_env.clone()));
        module_env.borrow_mut().strict = true;
        module_env
            .borrow_mut()
            .declare("*default*", BindingKind::Const);
        module_env
            .borrow_mut()
            .initialize_binding("*default*", value.clone());
        Rc::new(RefCell::new(LoadedModule {
            path: canon_path,
            env: module_env,
            exports: {
                let mut m = HashMap::new();
                m.insert("default".to_string(), value);
                m
            },
            export_bindings: {
                let mut m = HashMap::new();
                m.insert("default".to_string(), "*default*".to_string());
                m
            },
            cached_namespace: None,
            cached_deferred_namespace: None,
            cached_import_meta: None,
            error: None,
            namespace_imports: HashMap::new(),
            star_export_sources: Vec::new(),
            evaluated: true,
            is_evaluating: false,
            deferred_only: false,
            has_tla: false,
            program_ast: None,
            async_evaluation_order: None,
            pending_async_dependencies: 0,
            async_parent_modules: Vec::new(),
            cycle_root: None,
            top_level_capability: None,
            dfs_index: None,
            dfs_ancestor_index: None,
        }))
    }

    fn load_text_module(&mut self, path: &Path) -> Result<Rc<RefCell<LoadedModule>>, JsValue> {
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let key = (canon.clone(), ImportModuleType::Text);
        if let Some(existing) = self.synthetic_module_registry.get(&key) {
            return Ok(existing.clone());
        }
        let source = std::fs::read_to_string(path).map_err(|e| {
            JsValue::String(JsString::from_str(&format!(
                "Cannot read module '{}': {}",
                path.display(),
                e
            )))
        })?;
        let value = JsValue::String(JsString::from_str(&source));
        let module = self.create_synthetic_default_module(canon, value);
        self.synthetic_module_registry.insert(key, module.clone());
        Ok(module)
    }

    fn load_bytes_module(&mut self, path: &Path) -> Result<Rc<RefCell<LoadedModule>>, JsValue> {
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let key = (canon.clone(), ImportModuleType::Bytes);
        if let Some(existing) = self.synthetic_module_registry.get(&key) {
            return Ok(existing.clone());
        }
        let bytes = std::fs::read(path).map_err(|e| {
            JsValue::String(JsString::from_str(&format!(
                "Cannot read module '{}': {}",
                path.display(),
                e
            )))
        })?;
        let value = self.create_immutable_uint8array(&bytes);
        let module = self.create_synthetic_default_module(canon, value);
        self.synthetic_module_registry.insert(key, module.clone());
        Ok(module)
    }

    fn create_immutable_uint8array(&mut self, bytes: &[u8]) -> JsValue {
        use std::cell::Cell;
        let len = bytes.len();
        let buf_rc = Rc::new(RefCell::new(BufferData::Owned(bytes.to_vec())));
        let detached = Rc::new(Cell::new(false));

        let ab_obj = self.create_object();
        {
            let mut ab = ab_obj.borrow_mut();
            ab.class_name = "ArrayBuffer".to_string();
            ab.prototype = self.realm().arraybuffer_prototype.clone();
            ab.arraybuffer_data = Some(buf_rc.clone());
            ab.arraybuffer_detached = Some(detached.clone());
            ab.arraybuffer_is_immutable = true;
        }
        self.gc_track_external_bytes(len);
        let ab_id = ab_obj.borrow().id.unwrap();
        let buf_val = JsValue::Object(crate::types::JsObject { id: ab_id });

        let ta_info = TypedArrayInfo {
            kind: TypedArrayKind::Uint8,
            buffer: buf_rc,
            byte_offset: 0,
            byte_length: len,
            array_length: len,
            is_detached: detached,
            is_length_tracking: false,
        };

        let proto = self.realm().uint8array_prototype.clone().unwrap();
        let ta_obj = self.create_typed_array_object_with_proto(ta_info, buf_val, &proto);
        let ta_id = ta_obj.borrow().id.unwrap();
        JsValue::Object(crate::types::JsObject { id: ta_id })
    }

    /// Execute a module's body synchronously (no DFS into dependencies).
    fn execute_module_body_sync(&mut self, module_path: &Path) -> Result<(), JsValue> {
        let module = match self.module_registry.get(module_path).cloned() {
            Some(m) => m,
            None => return Ok(()),
        };
        let program = match module.borrow().program_ast.clone() {
            Some(p) => p,
            None => return Ok(()),
        };
        let module_env = module.borrow().env.clone();
        let prev_path = self.current_module_path.take();
        self.current_module_path = Some(module_path.to_path_buf());
        self.static_module_load_depth += 1;

        let mut err = None;
        for item in &program.module_items {
            match item {
                ModuleItem::Statement(stmt) => {
                    let result = self.exec_statement(stmt, &module_env);
                    if let Completion::Throw(e) = result {
                        module.borrow_mut().error = Some(e.clone());
                        err = Some(e);
                        break;
                    }
                }
                ModuleItem::ImportDeclaration(_) => {}
                ModuleItem::ExportDeclaration(export) => {
                    let result = self.exec_export_declaration(export, &module_env);
                    if let Completion::Throw(e) = result {
                        module.borrow_mut().error = Some(e.clone());
                        err = Some(e);
                        break;
                    }
                    self.collect_exports(export, &module_env, &module);
                }
            }
        }
        module.borrow_mut().program_ast = None;
        self.static_module_load_depth -= 1;
        self.current_module_path = prev_path;
        match err {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    fn collect_all_exports(&mut self, module_path: &Path) {
        let module = match self.module_registry.get(module_path).cloned() {
            Some(m) => m,
            None => return,
        };
        let program = match module.borrow().program_ast.clone() {
            Some(p) => p,
            None => return,
        };
        let module_env = module.borrow().env.clone();
        let prev_path = self.current_module_path.take();
        self.current_module_path = Some(module_path.to_path_buf());
        for item in &program.module_items {
            if let ModuleItem::ExportDeclaration(export) = item {
                self.collect_exports(export, &module_env, &module);
            }
        }
        self.current_module_path = prev_path;
    }

    fn module_items_to_statements(program: &crate::ast::Program) -> Vec<crate::ast::Statement> {
        use crate::ast::*;
        let mut stmts = Vec::new();
        for item in &program.module_items {
            match item {
                ModuleItem::Statement(s) => stmts.push(s.clone()),
                ModuleItem::ImportDeclaration(_) => {}
                ModuleItem::ExportDeclaration(export) => match export {
                    ExportDeclaration::Named {
                        declaration: Some(decl),
                        ..
                    } => {
                        stmts.push(*decl.clone());
                    }
                    ExportDeclaration::Default(expr) => {
                        stmts.push(Statement::Variable(VariableDeclaration {
                            kind: VarKind::Let,
                            declarations: vec![VariableDeclarator {
                                pattern: Pattern::Identifier("*default*".to_string()),
                                init: Some(*expr.clone()),
                            }],
                        }));
                    }
                    ExportDeclaration::DefaultClass(c) => {
                        stmts.push(Statement::ClassDeclaration(c.clone()));
                    }
                    _ => {}
                },
            }
        }
        stmts
    }

    fn execute_async_module(&mut self, module_path: &Path) {
        let module = match self.module_registry.get(module_path).cloned() {
            Some(m) => m,
            None => return,
        };
        let program = match module.borrow().program_ast.clone() {
            Some(p) => p,
            None => return,
        };
        let module_env = module.borrow().env.clone();
        let stmts = Self::module_items_to_statements(&program);
        let sm =
            Rc::new(crate::interpreter::generator_transform::transform_async_function(&stmts, &[]));
        for tv in &sm.temp_vars {
            if !module_env.borrow().bindings.contains_key(tv) {
                module_env.borrow_mut().declare(tv, BindingKind::Var);
            }
        }
        for lv in &sm.local_vars {
            if !module_env.borrow().bindings.contains_key(&lv.name) {
                let bk = match lv.kind {
                    crate::ast::VarKind::Let
                    | crate::ast::VarKind::Const
                    | crate::ast::VarKind::Using
                    | crate::ast::VarKind::AwaitUsing => BindingKind::Let,
                    _ => BindingKind::Var,
                };
                module_env.borrow_mut().declare(&lv.name, bk);
            }
        }
        let path_for_resolve = module_path.to_path_buf();
        let path_for_reject = module_path.to_path_buf();
        let resolve_fn = self.create_function(JsFunction::native(
            "asyncModuleResolve".to_string(),
            0,
            move |interp, _this, _args| {
                let prev = interp.current_module_path.take();
                interp.current_module_path = Some(path_for_resolve.clone());
                interp.async_module_execution_fulfilled(&path_for_resolve.clone());
                interp.current_module_path = prev;
                Completion::Normal(JsValue::Undefined)
            },
        ));
        let reject_fn = self.create_function(JsFunction::native(
            "asyncModuleReject".to_string(),
            1,
            move |interp, _this, args| {
                let error = args.first().cloned().unwrap_or(JsValue::Undefined);
                interp.async_module_execution_rejected(&path_for_reject.clone(), &error);
                Completion::Normal(JsValue::Undefined)
            },
        ));
        let async_id = self.next_async_function_id;
        self.next_async_function_id += 1;
        self.module_async_info
            .insert(async_id, module_path.to_path_buf());
        self.async_function_states.insert(
            async_id,
            AsyncFunctionState {
                state_machine: sm,
                func_env: module_env,
                is_strict: true,
                current_state: 0,
                try_stack: vec![],
                pending_binding: None,
                pending_return: None,
                saved_finally_exception: None,
                resolve_fn,
                reject_fn,
                for_of_stack: vec![],
                for_of_iter_env: None,
                module_path: Some(module_path.to_path_buf()),
            },
        );
        let prev_path = self.current_module_path.take();
        self.current_module_path = Some(module_path.to_path_buf());
        self.static_module_load_depth += 1;
        self.async_function_resume(async_id, JsValue::Undefined, false);
        self.static_module_load_depth -= 1;
        self.current_module_path = prev_path;
    }

    fn inner_module_evaluation(
        &mut self,
        module_path: &Path,
        stack: &mut Vec<PathBuf>,
        index: u32,
    ) -> Result<u32, JsValue> {
        let canon = module_path
            .canonicalize()
            .unwrap_or_else(|_| module_path.to_path_buf());
        let module = match self.module_registry.get(&canon).cloned() {
            Some(m) => m,
            None => return Ok(index),
        };
        {
            let m = module.borrow();
            if m.evaluated && !m.is_evaluating {
                if let Some(ref err) = m.error {
                    return Err(err.clone());
                }
                return Ok(index);
            }
            if m.is_evaluating {
                return Ok(index);
            }
        }
        let mut idx = index;
        {
            let mut m = module.borrow_mut();
            m.is_evaluating = true;
            m.dfs_index = Some(idx);
            m.dfs_ancestor_index = Some(idx);
            m.pending_async_dependencies = 0;
        }
        idx += 1;
        stack.push(canon.clone());

        // Build evaluationList per spec §16.2.1.5.3.1 step 7
        let dep_paths = self.get_module_dep_paths(&canon);
        let mut evaluation_list: Vec<PathBuf> = Vec::new();
        for (dep_canon, is_deferred) in &dep_paths {
            if *is_deferred {
                let mut to_eval = Vec::new();
                let mut seen = std::collections::HashSet::new();
                self.gather_async_transitive_deps(dep_canon, &mut to_eval, &mut seen);
                for async_dep in to_eval {
                    if !evaluation_list.contains(&async_dep) {
                        evaluation_list.push(async_dep);
                    }
                }
            } else if !evaluation_list.contains(dep_canon) {
                evaluation_list.push(dep_canon.clone());
            }
        }

        // Evaluate each dep and set up async parent relationships (spec §16.2.1.5.3.1 step 11)
        for dep_canon in evaluation_list.into_iter() {
            idx = self.inner_module_evaluation(&dep_canon, stack, idx)?;
            let dep_mod = match self.module_registry.get(&dep_canon).cloned() {
                Some(m) => m,
                None => continue,
            };
            let dep = dep_mod.borrow();
            let dep_on_stack = stack.contains(&dep_canon);

            // Step 11.c.iii/iv: determine requiredModule for step v
            let (_req_path, req_mod) = if dep_on_stack {
                // iii: on stack → update dfs_ancestor_index, requiredModule stays as dep
                let dep_ancestor = dep.dfs_ancestor_index.unwrap_or(u32::MAX);
                drop(dep);
                let mut m = module.borrow_mut();
                let my_ancestor = m.dfs_ancestor_index.unwrap_or(u32::MAX);
                m.dfs_ancestor_index = Some(my_ancestor.min(dep_ancestor));
                drop(m);
                (dep_canon.clone(), dep_mod.clone())
            } else {
                // iv: not on stack → requiredModule = cycleRoot
                let cycle_root_path = dep.cycle_root.clone().unwrap_or_else(|| dep_canon.clone());
                drop(dep);
                let root_mod = self
                    .module_registry
                    .get(&cycle_root_path)
                    .cloned()
                    .unwrap_or_else(|| dep_mod.clone());
                let root = root_mod.borrow();
                if let Some(ref err) = root.error {
                    return Err(err.clone());
                }
                drop(root);
                (cycle_root_path, root_mod)
            };

            // Step 11.c.v: if requiredModule has AsyncEvaluationOrder, track as pending dep
            let req = req_mod.borrow();
            if req.async_evaluation_order.is_some() && !req.evaluated {
                drop(req);
                module.borrow_mut().pending_async_dependencies += 1;
                req_mod
                    .borrow_mut()
                    .async_parent_modules
                    .push(canon.clone());
            }
        }

        let has_tla = module.borrow().has_tla;
        let pending = module.borrow().pending_async_dependencies;
        if has_tla || pending > 0 {
            let order = self.module_async_evaluation_count;
            self.module_async_evaluation_count += 1;
            module.borrow_mut().async_evaluation_order = Some(order);
            if pending == 0 {
                self.execute_async_module(&canon);
            }
        } else {
            self.execute_module_body_sync(&canon)?
        }

        let my_dfs = module.borrow().dfs_index.unwrap_or(0);
        let my_ancestor = module.borrow().dfs_ancestor_index.unwrap_or(0);
        if my_dfs == my_ancestor {
            let has_async = module.borrow().async_evaluation_order.is_some();
            while let Some(popped) = stack.pop() {
                if let Some(popped_mod) = self.module_registry.get(&popped).cloned() {
                    popped_mod.borrow_mut().cycle_root = Some(canon.clone());
                    if !has_async {
                        let mut pm = popped_mod.borrow_mut();
                        pm.evaluated = true;
                        pm.is_evaluating = false;
                    }
                }
                if popped == canon {
                    break;
                }
            }
        }
        Ok(idx)
    }

    fn get_module_dep_paths(&self, canon_path: &Path) -> Vec<(PathBuf, bool)> {
        let module = match self.module_registry.get(canon_path) {
            Some(m) => m.clone(),
            None => return Vec::new(),
        };
        let program = match module.borrow().program_ast.clone() {
            Some(p) => p,
            None => return Vec::new(),
        };
        let mut deps = Vec::new();
        for item in &program.module_items {
            let (specifier, is_deferred) = match item {
                ModuleItem::ImportDeclaration(import) => {
                    // Skip synthetic (text/bytes) imports — they're not DFS dependencies
                    if import_module_type(&import.attributes).is_some() {
                        continue;
                    }
                    let is_defer = import
                        .specifiers
                        .iter()
                        .any(|s| matches!(s, ImportSpecifier::DeferredNamespace(_)));
                    (Some(import.source.as_str()), is_defer)
                }
                ModuleItem::ExportDeclaration(ExportDeclaration::All { source, .. }) => {
                    (Some(source.as_str()), false)
                }
                ModuleItem::ExportDeclaration(ExportDeclaration::Named {
                    source: Some(source),
                    ..
                }) => (Some(source.as_str()), false),
                _ => (None, false),
            };
            if let Some(spec) = specifier
                && let Ok(resolved) = self.resolve_module_specifier_pure(spec, Some(canon_path))
            {
                deps.push((resolved, is_deferred));
            }
        }
        deps
    }

    fn gather_available_ancestors(&mut self, module_path: &Path) -> Vec<PathBuf> {
        let parents = match self.module_registry.get(module_path) {
            Some(m) => m.borrow().async_parent_modules.clone(),
            None => return Vec::new(),
        };
        let mut result = Vec::new();
        for parent_path in parents {
            let parent = match self.module_registry.get(&parent_path).cloned() {
                Some(m) => m,
                None => continue,
            };
            let should_add = {
                let mut pm = parent.borrow_mut();
                pm.pending_async_dependencies = pm.pending_async_dependencies.saturating_sub(1);
                pm.pending_async_dependencies == 0
                    && pm.async_evaluation_order.is_some()
                    && pm.error.is_none()
            };
            if should_add {
                result.push(parent_path);
            }
        }
        result.sort_by_key(|p| {
            self.module_registry
                .get(p)
                .and_then(|m| m.borrow().async_evaluation_order)
                .unwrap_or(u64::MAX)
        });
        result
    }

    fn async_module_execution_rejected(&mut self, module_path: &Path, error: &JsValue) {
        let module = match self.module_registry.get(module_path).cloned() {
            Some(m) => m,
            None => return,
        };
        if module.borrow().error.is_some() {
            return;
        }
        {
            let mut m = module.borrow_mut();
            m.error = Some(error.clone());
            m.evaluated = true;
            m.is_evaluating = false;
        }
        if let Some((_promise, _resolve, reject)) = module.borrow().top_level_capability.clone() {
            let _ = self.call_function(&reject, &JsValue::Undefined, std::slice::from_ref(error));
        }
        let parents = module.borrow().async_parent_modules.clone();
        for parent in parents {
            self.async_module_execution_rejected(&parent, error);
        }
    }

    fn async_module_execution_fulfilled(&mut self, module_path: &Path) {
        let module = match self.module_registry.get(module_path).cloned() {
            Some(m) => m,
            None => return,
        };
        if module.borrow().error.is_some() {
            return;
        }
        {
            let mut m = module.borrow_mut();
            m.evaluated = true;
            m.is_evaluating = false;
        }
        self.collect_all_exports(module_path);
        if let Some((_promise, resolve, _reject)) = module.borrow().top_level_capability.clone() {
            let _ = self.call_function(&resolve, &JsValue::Undefined, &[JsValue::Undefined]);
        }
        let mut exec_list = self.gather_available_ancestors(module_path);
        let mut i = 0;
        while i < exec_list.len() {
            let ancestor = exec_list[i].clone();
            i += 1;
            let ancestor_has_tla = self
                .module_registry
                .get(&ancestor)
                .map(|m| m.borrow().has_tla)
                .unwrap_or(false);
            if ancestor_has_tla {
                self.execute_async_module(&ancestor);
            } else {
                match self.execute_module_body_sync(&ancestor) {
                    Ok(()) => {
                        if let Some(m) = self.module_registry.get(&ancestor) {
                            let mut mb = m.borrow_mut();
                            mb.evaluated = true;
                            mb.is_evaluating = false;
                        }
                        if let Some(m) = self.module_registry.get(&ancestor).cloned()
                            && let Some((_p, resolve, _r)) = m.borrow().top_level_capability.clone()
                        {
                            let _ = self.call_function(
                                &resolve,
                                &JsValue::Undefined,
                                &[JsValue::Undefined],
                            );
                        }
                    }
                    Err(e) => {
                        self.async_module_execution_rejected(&ancestor, &e);
                        continue;
                    }
                }
                let more = self.gather_available_ancestors(&ancestor);
                exec_list.extend(more);
            }
        }
    }

    /// Detect if a module has top-level await
    fn module_has_tla(program: &crate::ast::Program) -> bool {
        for item in &program.module_items {
            match item {
                ModuleItem::Statement(stmt) if Self::stmt_has_tla(stmt) => {
                    return true;
                }
                ModuleItem::ExportDeclaration(export) if Self::export_has_tla(export) => {
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    fn stmt_has_tla(stmt: &crate::ast::Statement) -> bool {
        use crate::ast::Statement;
        match stmt {
            Statement::Expression(expr) => Self::expr_has_await(expr),
            Statement::Variable(decl) => {
                if matches!(decl.kind, crate::ast::VarKind::AwaitUsing) {
                    return true;
                }
                decl.declarations
                    .iter()
                    .any(|d| d.init.as_ref().is_some_and(Self::expr_has_await))
            }
            Statement::Return(Some(expr)) | Statement::Throw(expr) => Self::expr_has_await(expr),
            Statement::If(if_stmt) => {
                Self::expr_has_await(&if_stmt.test)
                    || Self::stmt_has_tla(&if_stmt.consequent)
                    || if_stmt
                        .alternate
                        .as_ref()
                        .is_some_and(|s| Self::stmt_has_tla(s))
            }
            Statement::For(for_stmt) => {
                for_stmt.init.as_ref().is_some_and(|init| match init {
                    crate::ast::ForInit::Expression(e) => Self::expr_has_await(e),
                    crate::ast::ForInit::Variable(v) => {
                        if matches!(v.kind, crate::ast::VarKind::AwaitUsing) {
                            return true;
                        }
                        v.declarations
                            .iter()
                            .any(|d| d.init.as_ref().is_some_and(Self::expr_has_await))
                    }
                }) || for_stmt.test.as_ref().is_some_and(Self::expr_has_await)
                    || for_stmt.update.as_ref().is_some_and(Self::expr_has_await)
                    || Self::stmt_has_tla(&for_stmt.body)
            }
            Statement::ForIn(fi) => Self::expr_has_await(&fi.right) || Self::stmt_has_tla(&fi.body),
            Statement::ForOf(fo) => {
                fo.is_await || Self::expr_has_await(&fo.right) || Self::stmt_has_tla(&fo.body)
            }
            Statement::While(w) => Self::expr_has_await(&w.test) || Self::stmt_has_tla(&w.body),
            Statement::DoWhile(dw) => {
                Self::stmt_has_tla(&dw.body) || Self::expr_has_await(&dw.test)
            }
            Statement::Block(stmts) => stmts.iter().any(Self::stmt_has_tla),
            Statement::Try(t) => {
                t.block.iter().any(Self::stmt_has_tla)
                    || t.handler
                        .as_ref()
                        .is_some_and(|h| h.body.iter().any(Self::stmt_has_tla))
                    || t.finalizer
                        .as_ref()
                        .is_some_and(|f| f.iter().any(Self::stmt_has_tla))
            }
            Statement::Switch(sw) => {
                Self::expr_has_await(&sw.discriminant)
                    || sw
                        .cases
                        .iter()
                        .any(|c| c.consequent.iter().any(Self::stmt_has_tla))
            }
            Statement::Labeled(_, body) => Self::stmt_has_tla(body),
            _ => false,
        }
    }

    fn export_has_tla(export: &crate::ast::ExportDeclaration) -> bool {
        use crate::ast::ExportDeclaration;
        match export {
            ExportDeclaration::Named {
                declaration: Some(stmt),
                ..
            } => Self::stmt_has_tla(stmt),
            ExportDeclaration::Default(expr) => Self::expr_has_await(expr),
            _ => false,
        }
    }

    fn expr_has_await(expr: &crate::ast::Expression) -> bool {
        use crate::ast::Expression;
        match expr {
            Expression::Await(_) => true,
            Expression::Function(_) | Expression::ArrowFunction(_) | Expression::Class(_) => false,
            Expression::Binary(_, left, right)
            | Expression::Logical(_, left, right)
            | Expression::Assign(_, left, right) => {
                Self::expr_has_await(left) || Self::expr_has_await(right)
            }
            Expression::Unary(_, operand)
            | Expression::Update(_, _, operand)
            | Expression::Typeof(operand)
            | Expression::Void(operand)
            | Expression::Delete(operand)
            | Expression::Spread(operand) => Self::expr_has_await(operand),
            Expression::Conditional(test, consequent, alternate) => {
                Self::expr_has_await(test)
                    || Self::expr_has_await(consequent)
                    || Self::expr_has_await(alternate)
            }
            Expression::Call(callee, arguments) => {
                Self::expr_has_await(callee)
                    || arguments.iter().any(Self::expr_has_await)
            }
            Expression::New(callee, arguments) => {
                Self::expr_has_await(callee)
                    || arguments.iter().any(Self::expr_has_await)
            }
            Expression::Member(object, prop) => {
                Self::expr_has_await(object) || match prop {
                    crate::ast::MemberProperty::Computed(e) => Self::expr_has_await(e),
                    _ => false,
                }
            }
            Expression::OptionalChain(left, right) => {
                Self::expr_has_await(left) || Self::expr_has_await(right)
            }
            Expression::Array(elements, _) => {
                elements.iter().any(|e| e.as_ref().is_some_and(Self::expr_has_await))
            }
            Expression::Object(props) => {
                props.iter().any(|p| {
                    Self::expr_has_await(&p.value)
                        || matches!(&p.key, crate::ast::PropertyKey::Computed(e) if Self::expr_has_await(e))
                })
            }
            Expression::Comma(exprs) | Expression::Sequence(exprs) => {
                exprs.iter().any(Self::expr_has_await)
            }
            Expression::TaggedTemplate(tag, quasi) => {
                Self::expr_has_await(tag)
                    || quasi.expressions.iter().any(Self::expr_has_await)
            }
            Expression::Template(tl) => {
                tl.expressions.iter().any(Self::expr_has_await)
            }
            Expression::Yield(arg, _) => {
                arg.as_ref().is_some_and(|a| Self::expr_has_await(a))
            }
            Expression::Import(src, opts) | Expression::ImportDefer(src, opts) => {
                Self::expr_has_await(src)
                    || opts.as_ref().is_some_and(|o| Self::expr_has_await(o))
            }
            _ => false,
        }
    }

    /// Eagerly evaluate async transitive dependencies of a deferred module
    fn evaluate_async_transitive_deps(&mut self, deferred_path: &Path) {
        let mut to_eval = Vec::new();
        let mut seen = std::collections::HashSet::new();
        self.gather_async_transitive_deps(deferred_path, &mut to_eval, &mut seen);

        for path in to_eval {
            if let Some(module) = self.module_registry.get(&path).cloned()
                && !module.borrow().evaluated
            {
                let mut stack = vec![];
                let _ = self.inner_module_evaluation(&path, &mut stack, 0);
            }
        }
    }

    fn gather_async_transitive_deps(
        &self,
        module_path: &Path,
        result: &mut Vec<PathBuf>,
        seen: &mut std::collections::HashSet<PathBuf>,
    ) {
        let canon = module_path
            .canonicalize()
            .unwrap_or_else(|_| module_path.to_path_buf());
        if !seen.insert(canon.clone()) {
            return;
        }

        let module = match self.module_registry.get(&canon) {
            Some(m) => m.clone(),
            None => return,
        };

        let module_ref = module.borrow();
        if module_ref.evaluated {
            return;
        }

        if module_ref.has_tla {
            if !result.contains(&canon) {
                result.push(canon.clone());
            }
            return;
        }

        // Check transitive deps
        if let Some(ref program) = module_ref.program_ast {
            let items: Vec<_> = program.module_items.clone();
            drop(module_ref);
            for item in &items {
                let specifier = match item {
                    ModuleItem::ImportDeclaration(import) => {
                        if import_module_type(&import.attributes).is_some() {
                            continue;
                        }
                        Some(import.source.as_str())
                    }
                    ModuleItem::ExportDeclaration(ExportDeclaration::All { source, .. }) => {
                        Some(source.as_str())
                    }
                    ModuleItem::ExportDeclaration(ExportDeclaration::Named {
                        source: Some(source),
                        ..
                    }) => Some(source.as_str()),
                    _ => None,
                };
                if let Some(spec) = specifier
                    && let Ok(resolved) = self.resolve_module_specifier_pure(spec, Some(&canon))
                {
                    self.gather_async_transitive_deps(&resolved, result, seen);
                }
            }
        }
    }

    /// Like resolve_module_specifier but doesn't need &mut self
    fn resolve_module_specifier_pure(
        &self,
        specifier: &str,
        referrer: Option<&Path>,
    ) -> Result<PathBuf, JsValue> {
        if specifier.starts_with("./") || specifier.starts_with("../") || specifier.starts_with('/')
        {
            let base = referrer.and_then(|r| r.parent()).unwrap_or(Path::new("."));
            let resolved = base.join(specifier);
            if resolved.exists() {
                return Ok(resolved.canonicalize().unwrap_or(resolved));
            }
            // Try .js extension
            let with_js = resolved.with_extension("js");
            if with_js.exists() {
                return Ok(with_js.canonicalize().unwrap_or(with_js));
            }
            // Try /index.js
            let index = resolved.join("index.js");
            if index.exists() {
                return Ok(index.canonicalize().unwrap_or(index));
            }
        }
        Err(JsValue::Undefined)
    }

    /// Check if a module and all its transitive deps are ready for synchronous execution
    fn ready_for_sync_execution(
        &self,
        path: &Path,
        seen: &mut std::collections::HashSet<PathBuf>,
    ) -> bool {
        let canon = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if !seen.insert(canon.clone()) {
            return true; // cycle — spec says return true
        }

        let module = match self.module_registry.get(&canon) {
            Some(m) => m.clone(),
            None => return true,
        };

        let module_ref = module.borrow();
        if module_ref.evaluated {
            return true;
        }
        if module_ref.is_evaluating {
            return false;
        }
        if module_ref.has_tla {
            return false;
        }

        if let Some(ref program) = module_ref.program_ast {
            let items: Vec<_> = program.module_items.clone();
            drop(module_ref);
            for item in &items {
                let specifier = match item {
                    ModuleItem::ImportDeclaration(import) => {
                        if import_module_type(&import.attributes).is_some() {
                            continue;
                        }
                        Some(import.source.as_str())
                    }
                    ModuleItem::ExportDeclaration(ExportDeclaration::All { source, .. }) => {
                        Some(source.as_str())
                    }
                    ModuleItem::ExportDeclaration(ExportDeclaration::Named {
                        source: Some(source),
                        ..
                    }) => Some(source.as_str()),
                    _ => None,
                };
                if let Some(spec) = specifier
                    && let Ok(resolved) = self.resolve_module_specifier_pure(spec, Some(&canon))
                    && !self.ready_for_sync_execution(&resolved, seen)
                {
                    return false;
                }
            }
        }

        true
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

    /// Resolve an export to its source environment and binding name.
    /// Returns (source_env, binding_name) for creating indirect import bindings.
    fn resolve_export_binding(
        &mut self,
        module_path: &Path,
        export_name: &str,
        visited: &mut std::collections::HashSet<(PathBuf, String)>,
    ) -> Result<(EnvRef, String), JsValue> {
        let canon_path = module_path
            .canonicalize()
            .unwrap_or_else(|_| module_path.to_path_buf());
        let key = (canon_path.clone(), export_name.to_string());

        if visited.contains(&key) {
            return Err(self.create_error(
                "SyntaxError",
                &format!("Circular re-export of '{}'", export_name),
            ));
        }
        visited.insert(key);

        let module = self.load_module(&canon_path)?;

        let reexport_info = {
            let module_ref = module.borrow();
            if let Some(binding) = module_ref.export_bindings.get(export_name) {
                if binding == "*ambiguous*" {
                    return Err(self.create_error(
                        "SyntaxError",
                        &format!(
                            "Ambiguous export '{}' in module '{}'",
                            export_name,
                            canon_path.display()
                        ),
                    ));
                }
                if let Some(ns_source) = binding.strip_prefix("*ns:") {
                    // Namespace re-export — resolve to the actual source module's env
                    // so that two re-exports of the same namespace compare equal
                    let ns_source = ns_source.to_string();
                    drop(module_ref);
                    if let Ok(resolved) =
                        self.resolve_module_specifier(&ns_source, Some(&canon_path))
                        && let Ok(ns_mod) = self.load_module(&resolved)
                    {
                        return Ok((ns_mod.borrow().env.clone(), "*namespace*".to_string()));
                    }
                    let module_ref = module.borrow();
                    return Ok((module_ref.env.clone(), export_name.to_string()));
                }
                if let Some(info) = binding.strip_prefix("*reexport:") {
                    if let Some(colon_idx) = info.rfind(':') {
                        let source = info[..colon_idx].to_string();
                        let name = info[colon_idx + 1..].to_string();
                        Some((source, name))
                    } else {
                        None
                    }
                } else {
                    let binding_name = binding.clone();
                    let env = module_ref.env.clone();
                    let ns_path = module_ref.namespace_imports.get(&binding_name).cloned();
                    drop(module_ref);
                    // Namespace import (import * as foo) resolves to the source module
                    if let Some(ns_path) = ns_path
                        && let Ok(ns_mod) = self.load_module(&ns_path)
                    {
                        return Ok((ns_mod.borrow().env.clone(), "*namespace*".to_string()));
                    }
                    // Check if it's an indirect (imported) binding
                    if let Some(ref indirect_map) = env.borrow().indirect_bindings
                        && let Some((src_env, src_name)) = indirect_map.get(&binding_name)
                    {
                        return Ok((src_env.clone(), src_name.clone()));
                    }
                    return Ok((env, binding_name));
                }
            } else {
                // Check star re-exports
                None
            }
        };

        if let Some((source_specifier, source_export)) = reexport_info {
            let resolved = self.resolve_module_specifier(&source_specifier, Some(&canon_path))?;
            return self.resolve_export_binding(&resolved, &source_export, visited);
        }

        Err(self.create_error(
            "SyntaxError",
            &format!(
                "Module '{}' has no export named '{}'",
                canon_path.display(),
                export_name
            ),
        ))
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

        // §16.2.1.6.3 step 2: circular reference → return null (not resolved)
        if visited.contains(&key) {
            return Err(self.create_error(
                "SyntaxError",
                &format!(
                    "Circular re-export of '{}' in module '{}'",
                    export_name,
                    canon_path.display()
                ),
            ));
        }
        visited.insert(key);

        // Load the module if not already loaded
        let module = self.load_module(&canon_path)?;

        // Check if this export is a local binding or a re-export (steps 4-5)
        let (reexport_info, star_sources) = {
            let module_ref = module.borrow();
            let reexport = if let Some(binding) = module_ref.export_bindings.get(export_name) {
                if binding.starts_with("*ns:") {
                    return Ok(());
                }
                if binding == "*ambiguous*" {
                    return Err(self.create_error(
                        "SyntaxError",
                        &format!(
                            "Ambiguous export '{}' in module '{}'",
                            export_name,
                            canon_path.display()
                        ),
                    ));
                }
                if let Some(info) = binding.strip_prefix("*reexport:") {
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
            };
            let stars = module_ref.star_export_sources.clone();
            (reexport, stars)
        };

        // Step 5: follow indirect re-exports
        if let Some((source_specifier, source_export)) = reexport_info {
            let resolved = self.resolve_module_specifier(&source_specifier, Some(&canon_path))?;
            return self.resolve_export(&resolved, &source_export, visited);
        }

        // §16.2.1.6.3 step 8: check star re-exports
        let mut found_in_star = false;
        let mut first_star_source: Option<PathBuf> = None;
        for star_source in &star_sources {
            let resolved = self.resolve_module_specifier(star_source, Some(&canon_path))?;
            let mut v2 = visited.clone();
            if self.resolve_export(&resolved, export_name, &mut v2).is_ok() {
                if found_in_star {
                    // §16.2.1.6.3 step 10.d.ii: ambiguous — same name from multiple stars
                    // Check if they resolve to the same (module, binding)
                    let mut va = std::collections::HashSet::new();
                    let ra = self.resolve_export_binding(
                        first_star_source.as_ref().unwrap(),
                        export_name,
                        &mut va,
                    );
                    let mut vb = std::collections::HashSet::new();
                    let rb = self.resolve_export_binding(&resolved, export_name, &mut vb);
                    match (ra, rb) {
                        (Ok((env1, name1)), Ok((env2, name2))) => {
                            if !std::rc::Rc::ptr_eq(&env1, &env2) || name1 != name2 {
                                return Err(self.create_error(
                                    "SyntaxError",
                                    &format!(
                                        "Ambiguous export '{}' in module '{}'",
                                        export_name,
                                        canon_path.display()
                                    ),
                                ));
                            }
                        }
                        _ => {
                            return Err(self.create_error(
                                "SyntaxError",
                                &format!(
                                    "Ambiguous export '{}' in module '{}'",
                                    export_name,
                                    canon_path.display()
                                ),
                            ));
                        }
                    }
                }
                found_in_star = true;
                if first_star_source.is_none() {
                    first_star_source = Some(resolved);
                }
            }
        }
        if found_in_star {
            return Ok(());
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

        // Per §16.2.1.5.2 GetModuleNamespace: return cached namespace if present
        if let Some(cached) = module.borrow().cached_namespace.clone() {
            return cached;
        }

        let obj = self.create_object();
        let (env, export_bindings, module_path, export_names) = {
            let module_ref = module.borrow();
            let env = module_ref.env.clone();
            let export_bindings = module_ref.export_bindings.clone();
            let module_path = if module_ref.path.as_os_str().is_empty() {
                None
            } else {
                Some(module_ref.path.clone())
            };
            let mut export_names: Vec<String> = module_ref
                .exports
                .keys()
                .filter(|k| {
                    module_ref
                        .export_bindings
                        .get(*k)
                        .is_none_or(|b| b != "*ambiguous*")
                })
                .cloned()
                .collect();
            export_names.sort();
            (env, export_bindings, module_path, export_names)
        }; // module_ref borrow dropped here

        // Set module namespace data for live bindings
        obj.borrow_mut().module_namespace = Some(ModuleNamespaceData {
            env: env.clone(),
            export_names: export_names.clone(),
            export_to_binding: export_bindings,
            module_path,
            deferred: false,
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
        let ns = JsValue::Object(crate::types::JsObject { id });
        module.borrow_mut().cached_namespace = Some(ns.clone());
        ns
    }

    fn create_deferred_module_namespace(&mut self, module: &Rc<RefCell<LoadedModule>>) -> JsValue {
        use crate::interpreter::types::ModuleNamespaceData;

        if let Some(cached) = module.borrow().cached_deferred_namespace.clone() {
            return cached;
        }

        let obj = self.create_object();
        let (env, export_bindings, module_path, export_names) = {
            let module_ref = module.borrow();
            let env = module_ref.env.clone();
            let export_bindings = module_ref.export_bindings.clone();
            let module_path = if module_ref.path.as_os_str().is_empty() {
                None
            } else {
                Some(module_ref.path.clone())
            };
            let mut export_names: Vec<String> = module_ref
                .exports
                .keys()
                .filter(|k| {
                    module_ref
                        .export_bindings
                        .get(*k)
                        .is_none_or(|b| b != "*ambiguous*")
                })
                .cloned()
                .collect();
            export_names.sort();
            (env, export_bindings, module_path, export_names)
        };

        obj.borrow_mut().module_namespace = Some(ModuleNamespaceData {
            env: env.clone(),
            export_names: export_names.clone(),
            export_to_binding: export_bindings,
            module_path,
            deferred: true,
        });
        obj.borrow_mut().class_name = "Module".to_string();
        obj.borrow_mut().extensible = false;
        obj.borrow_mut().prototype = None;

        for name in &export_names {
            obj.borrow_mut().insert_property(
                name.clone(),
                PropertyDescriptor::data(JsValue::Undefined, true, true, false),
            );
        }

        let sym_key = self
            .get_symbol_key("toStringTag")
            .unwrap_or_else(|| "Symbol(Symbol.toStringTag)".to_string());
        obj.borrow_mut().insert_property(
            sym_key,
            PropertyDescriptor::data(
                JsValue::String(JsString::from_str("Deferred Module")),
                false,
                false,
                false,
            ),
        );

        let id = obj.borrow().id.unwrap();
        let ns = JsValue::Object(crate::types::JsObject { id });
        module.borrow_mut().cached_deferred_namespace = Some(ns.clone());
        ns
    }

    fn create_namespace_for_env(&mut self, target_env: &EnvRef) -> JsValue {
        let found = self
            .module_registry
            .values()
            .find(|m| Rc::ptr_eq(&m.borrow().env, target_env))
            .cloned();
        if let Some(module) = found {
            return self.create_module_namespace(&module);
        }
        JsValue::Undefined
    }

    fn hoist_module_statement(&mut self, stmt: &Statement, env: &EnvRef) {
        match stmt {
            Statement::Variable(decl) if decl.kind == VarKind::Var => {
                for d in &decl.declarations {
                    self.hoist_pattern(&d.pattern, env, false);
                }
            }
            // §16.2.1.6.1 InitializeEnvironment: pre-declare let/const/using bindings (TDZ)
            Statement::Variable(decl)
                if matches!(
                    decl.kind,
                    VarKind::Let | VarKind::Const | VarKind::Using | VarKind::AwaitUsing
                ) =>
            {
                let kind = match decl.kind {
                    VarKind::Let => BindingKind::Let,
                    _ => BindingKind::Const,
                };
                for d in &decl.declarations {
                    let mut names = Vec::new();
                    self.get_pattern_names(&d.pattern, &mut names);
                    for name in names {
                        env.borrow_mut().declare(&name, kind);
                    }
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
                    captured_new_target: None,
                };
                let val = self.create_function(func);
                let _ = env.borrow_mut().set(&f.name, val);
            }
            Statement::ClassDeclaration(c) => {
                if !c.name.is_empty() {
                    env.borrow_mut().declare(&c.name, BindingKind::Const);
                }
            }
            other => {
                self.hoist_vars_from_stmt(other, env, false);
            }
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
                // §16.2.1.6.1: function declarations are hoisted and initialized
                let name = if f.name.is_empty() {
                    "*default*".to_string()
                } else {
                    f.name.clone()
                };
                env.borrow_mut().declare(&name, BindingKind::Const);
                let func = JsFunction::User {
                    name: Some(if f.name.is_empty() {
                        "default".to_string()
                    } else {
                        f.name.clone()
                    }),
                    params: f.params.clone(),
                    body: f.body.clone(),
                    closure: env.clone(),
                    is_arrow: false,
                    is_strict: true,
                    is_generator: f.is_generator,
                    is_async: f.is_async,
                    is_method: false,
                    source_text: f.source_text.clone(),
                    captured_new_target: None,
                };
                let val = self.create_function(func);
                env.borrow_mut().initialize_binding(&name, val);
            }
            // §16.2.1.6.1: pre-declare *default* binding for default expressions/classes (TDZ)
            ExportDeclaration::Default(_) => {
                env.borrow_mut().declare("*default*", BindingKind::Let);
            }
            ExportDeclaration::DefaultClass(c) => {
                env.borrow_mut().declare("*default*", BindingKind::Const);
                if !c.name.is_empty() {
                    env.borrow_mut().declare(&c.name, BindingKind::Const);
                }
            }
            _ => {}
        }
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
                let val = if let Expression::Class(ce) = expr.as_ref()
                    && ce.name.is_none()
                {
                    match self.eval_class(
                        "default",
                        "",
                        &ce.super_class,
                        &ce.body,
                        env,
                        ce.source_text.clone(),
                    ) {
                        Completion::Normal(v) => v,
                        c => return c,
                    }
                } else {
                    let v = match self.eval_expr(expr, env) {
                        Completion::Normal(v) => v,
                        c => return c,
                    };
                    if expr.is_anonymous_function_definition() {
                        self.set_function_name(&v, "default");
                    }
                    v
                };
                env.borrow_mut().declare("*default*", BindingKind::Const);
                env.borrow_mut().initialize_binding("*default*", val);
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
                    captured_new_target: None,
                };
                let fn_obj = self.create_function(js_func);
                if !func.name.is_empty() {
                    env.borrow_mut().declare(&func.name, BindingKind::Let);
                    env.borrow_mut()
                        .initialize_binding(&func.name, fn_obj.clone());
                }
                env.borrow_mut().declare("*default*", BindingKind::Let);
                env.borrow_mut().initialize_binding("*default*", fn_obj);
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
                    &class.name,
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
                    env.borrow_mut()
                        .initialize_binding(&class.name, class_val.clone());
                }
                env.borrow_mut().declare("*default*", BindingKind::Const);
                env.borrow_mut().initialize_binding("*default*", class_val);
                Completion::Normal(JsValue::Undefined)
            }
            ExportDeclaration::All { .. } => {
                // Re-exports handled in Phase 3
                Completion::Normal(JsValue::Undefined)
            }
        }
    }

    pub(crate) fn drain_microtasks(&mut self) {
        let mut iterations = 0u64;
        loop {
            if !self.microtask_queue.is_empty() {
                let (roots, job) = self.microtask_queue.remove(0);
                for val in &roots {
                    self.gc_root_value(val);
                }
                let _ = job(self);
                for val in &roots {
                    self.gc_unroot_value(val);
                }
                iterations += 1;
                // Periodically check agent async completions (e.g. waitAsync timeouts)
                // so they get processed even when the microtask queue stays busy
                if iterations.is_multiple_of(64) {
                    let completions: Vec<_> = {
                        let mut lock = self.agent_async_completions.0.lock().unwrap();
                        lock.drain(..).collect()
                    };
                    for f in completions {
                        f(self);
                    }
                }
                continue;
            }
            let completions: Vec<_> = {
                let mut lock = self.agent_async_completions.0.lock().unwrap();
                lock.drain(..).collect()
            };
            if completions.is_empty() {
                break;
            }
            for f in completions {
                f(self);
            }
        }
    }

    /// Like drain_microtasks but blocks waiting for async completions via Condvar.
    /// Used by agent threads that need to wait for waitAsync resolutions.
    pub(crate) fn drain_microtasks_blocking(&mut self) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        loop {
            while !self.microtask_queue.is_empty() {
                let (roots, job) = self.microtask_queue.remove(0);
                for val in &roots {
                    self.gc_root_value(val);
                }
                let _ = job(self);
                for val in &roots {
                    self.gc_unroot_value(val);
                }
            }
            let completions: Vec<_> = {
                let mut lock = self.agent_async_completions.0.lock().unwrap();
                lock.drain(..).collect()
            };
            if !completions.is_empty() {
                for f in completions {
                    f(self);
                }
                continue;
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            // Wait for async completions with a timeout
            let (ref mtx, ref cvar) = *self.agent_async_completions;
            let lock = mtx.lock().unwrap();
            if !lock.is_empty() {
                drop(lock);
                continue;
            }
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                break;
            }
            let (_guard, _timeout_result) = cvar
                .wait_timeout(lock, remaining.min(std::time::Duration::from_millis(100)))
                .unwrap();
            drop(_guard);
        }
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

        // 3. Let newLen be ? ToUint32(Desc.[[Value]]).
        //    ToUint32 internally calls ToNumber — this is valueOf call #1
        let num_for_uint32 = self.to_number_value(&desc_value)?;
        let new_len = to_uint32_f64(num_for_uint32);

        // 4. Let numberLen be ? ToNumber(Desc.[[Value]]).
        //    This is a separate ToNumber call — valueOf call #2
        let number_len = self.to_number_value(&desc_value)?;

        // 5. If SameValueZero(newLen, numberLen) is false, throw RangeError.
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
                    k.parse::<u64>()
                        .ok()
                        .filter(|&idx| idx <= 0xFFFF_FFFE && idx.to_string() == *k)
                        .map(|idx| idx as u32)
                        .filter(|&idx| idx >= new_len && idx < old_len)
                        .map(|idx| (idx, k.clone()))
                })
                .collect();
            idx_keys.sort_by_key(|a| std::cmp::Reverse(a.0));

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

fn setup_agent_side_262(interp: &mut Interpreter) {
    use crate::types::JsObject;

    let dollar_262 = interp.create_object();
    let dollar_262_id = dollar_262.borrow().id.unwrap();

    let agent_obj = interp.create_object();
    let agent_obj_id = agent_obj.borrow().id.unwrap();

    // $262.agent.receiveBroadcast(callback)
    let receive_fn = interp.create_function(JsFunction::native(
        "receiveBroadcast".to_string(),
        1,
        |interp, _this, args| {
            let callback = args.first().cloned().unwrap_or(JsValue::Undefined);
            let rx = interp.agent_broadcast_rx.take();
            if let Some(rx) = rx {
                if let Ok(msg) = rx.recv() {
                    let sab_inner = msg.sab_shared;
                    let buf = BufferData::Shared(sab_inner.clone());
                    let buf_rc = Rc::new(RefCell::new(buf));
                    let sab_obj = interp.create_object();
                    {
                        let mut o = sab_obj.borrow_mut();
                        o.class_name = "SharedArrayBuffer".to_string();
                        o.prototype = interp.realm().shared_arraybuffer_prototype.clone();
                        o.arraybuffer_data = Some(buf_rc);
                        o.arraybuffer_detached = None;
                        o.arraybuffer_is_shared = true;
                        o.sab_shared = Some(sab_inner);
                    }
                    let sab_val = JsValue::Object(JsObject {
                        id: sab_obj.borrow().id.unwrap(),
                    });
                    interp.agent_broadcast_rx = Some(rx);
                    let _ = interp.call_function(&callback, &JsValue::Undefined, &[sab_val]);
                    interp.drain_microtasks();
                } else {
                    interp.agent_broadcast_rx = Some(rx);
                }
            }
            Completion::Normal(JsValue::Undefined)
        },
    ));
    agent_obj
        .borrow_mut()
        .insert_builtin("receiveBroadcast".to_string(), receive_fn);

    // $262.agent.report(value)
    let report_fn = interp.create_function(JsFunction::native(
        "report".to_string(),
        1,
        |interp, _this, args| {
            let val = args.first().cloned().unwrap_or(JsValue::Undefined);
            let s = match interp.to_string_value(&val) {
                Ok(s) => s,
                Err(e) => return Completion::Throw(e),
            };
            interp.agent_reports.lock().unwrap().push_back(s);
            Completion::Normal(JsValue::Undefined)
        },
    ));
    agent_obj
        .borrow_mut()
        .insert_builtin("report".to_string(), report_fn);

    // $262.agent.sleep(ms)
    let sleep_fn = interp.create_function(JsFunction::native(
        "sleep".to_string(),
        1,
        |interp, _this, args| {
            let ms_val = args.first().cloned().unwrap_or(JsValue::Undefined);
            let ms = match interp.to_number_value(&ms_val) {
                Ok(n) => n.max(0.0) as u64,
                Err(e) => return Completion::Throw(e),
            };
            std::thread::sleep(std::time::Duration::from_millis(ms));
            Completion::Normal(JsValue::Undefined)
        },
    ));
    agent_obj
        .borrow_mut()
        .insert_builtin("sleep".to_string(), sleep_fn);

    // $262.agent.leaving()
    let leaving_fn = interp.create_function(JsFunction::native(
        "leaving".to_string(),
        0,
        |_interp, _this, _args| Completion::Normal(JsValue::Undefined),
    ));
    agent_obj
        .borrow_mut()
        .insert_builtin("leaving".to_string(), leaving_fn);

    // $262.agent.monotonicNow()
    let start_time = std::time::Instant::now();
    let monotonic_fn = interp.create_function(JsFunction::native(
        "monotonicNow".to_string(),
        0,
        move |_interp, _this, _args| {
            let elapsed = start_time.elapsed().as_millis() as f64;
            Completion::Normal(JsValue::Number(elapsed))
        },
    ));
    agent_obj
        .borrow_mut()
        .insert_builtin("monotonicNow".to_string(), monotonic_fn);

    let agent_val = JsValue::Object(JsObject { id: agent_obj_id });
    dollar_262
        .borrow_mut()
        .insert_builtin("agent".to_string(), agent_val);

    let dollar_262_val = JsValue::Object(JsObject { id: dollar_262_id });
    interp
        .realm()
        .global_env
        .borrow_mut()
        .declare("$262", BindingKind::Const);
    interp
        .realm()
        .global_env
        .borrow_mut()
        .initialize_binding("$262", dollar_262_val);
}
