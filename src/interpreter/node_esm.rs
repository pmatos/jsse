//! The ES module host hook for `--node` (issue #1240). Inert until the
//! prelude installs its host function with `__host_esm_install`; without it
//! the engine's own relative/absolute resolution and file loading apply, so
//! test262 never reaches this file. The host function (JavaScript, in
//! `node-lib/internal/modules/esm/host.js`) resolves specifiers with Node's
//! algorithm and loads `commonjs` and `builtin` modules; Rust turns its answers
//! into Module Records and keeps the engine's linking and evaluation. See
//! `docs/adr/2026-10-26-1700-node-esm-loader.md`.

use super::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum EsmFormat {
    Module,
    Json,
    CommonJs,
    Builtin,
}

impl EsmFormat {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "module" => Some(Self::Module),
            "json" => Some(Self::Json),
            "commonjs" => Some(Self::CommonJs),
            "builtin" => Some(Self::Builtin),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Module => "module",
            Self::Json => "json",
            Self::CommonJs => "commonjs",
            Self::Builtin => "builtin",
        }
    }
}

#[derive(Default)]
pub(crate) struct NodeEsm {
    host: Option<JsValue>,
    resolved: HashMap<(Option<PathBuf>, String), ModuleKey>,
    targets: HashMap<ModuleKey, (EsmFormat, String)>,
    checked: HashSet<(ModuleKey, Option<ImportModuleType>)>,
    main: Option<ModuleKey>,
    main_ready: bool,
}

impl NodeEsm {
    pub(crate) fn install(&mut self, host: JsValue) {
        self.host = Some(host);
    }

    pub(crate) fn is_active(&self) -> bool {
        self.host.is_some()
    }

    pub(crate) fn lookup(&self, specifier: &str, referrer: Option<&Path>) -> Option<ModuleKey> {
        self.resolved
            .get(&(referrer.map(Path::to_path_buf), specifier.to_string()))
            .cloned()
    }
}

fn js_str(s: &str) -> JsValue {
    JsValue::string(JsString::from_str(s))
}

impl Interpreter {
    fn node_esm_call(&mut self, args: &[JsValue]) -> Result<JsValue, JsValue> {
        let host = self
            .node_host
            .esm
            .host
            .clone()
            .unwrap_or(JsValue::UNDEFINED);
        let frame = self.gc_root_frame();
        for arg in args {
            self.gc_root_value(arg);
        }
        let result = self.call_function(&host, &JsValue::UNDEFINED, args);
        self.gc_unroot_frame(frame);
        match result {
            Completion::Normal(value) => Ok(value),
            Completion::Throw(error) => Err(error),
            _ => Ok(JsValue::UNDEFINED),
        }
    }

    pub(super) fn node_esm_resolve(
        &mut self,
        specifier: &str,
        referrer: Option<&Path>,
    ) -> Result<ModuleKey, JsValue> {
        if let Some(key) = self.node_host.esm.lookup(specifier, referrer) {
            return Ok(key);
        }
        let parent = referrer.map_or(JsValue::UNDEFINED, |p| js_str(&p.to_string_lossy()));
        let answer = self.node_esm_call(&[js_str("resolve"), js_str(specifier), parent])?;
        let (format, target) = self.node_esm_answer(&answer);
        let key = match format {
            EsmFormat::Builtin => ModuleKey(PathBuf::from(format!("node:{target}"))),
            _ => ModuleKey::for_file(PathBuf::from(&target)),
        };
        let esm = &mut self.node_host.esm;
        esm.targets.insert(key.clone(), (format, target));
        esm.resolved.insert(
            (referrer.map(Path::to_path_buf), specifier.to_string()),
            key.clone(),
        );
        Ok(key)
    }

    fn node_esm_answer(&mut self, answer: &JsValue) -> (EsmFormat, String) {
        let part = |interp: &mut Interpreter, index: usize| {
            let value = answer.as_object_id().map_or(JsValue::UNDEFINED, |id| {
                interp.get_property_on_id(id, &index.to_string())
            });
            interp.to_string_lossy(&value).unwrap_or_default()
        };
        let format = EsmFormat::parse(&part(self, 0)).unwrap_or(EsmFormat::Module);
        (format, part(self, 1))
    }

    /// The module a hook-resolved key names, or `None` when the engine's own
    /// loader applies (the key was not resolved by the host, or it is a source
    /// text or JSON module).
    pub(super) fn node_esm_load(
        &mut self,
        key: &ModuleKey,
        import_type: Option<ImportModuleType>,
    ) -> Option<Result<Rc<RefCell<LoadedModule>>, JsValue>> {
        let (format, target) = self.node_host.esm.targets.get(key).cloned()?;
        if !self
            .node_host
            .esm
            .checked
            .contains(&(key.clone(), import_type))
        {
            let kind = import_type.map_or(JsValue::UNDEFINED, |t| js_str(t.attr_value()));
            let check = [
                js_str("check"),
                js_str(format.as_str()),
                js_str(&target),
                kind,
            ];
            if let Err(error) = self.node_esm_call(&check) {
                return Some(Err(error));
            }
            self.node_host
                .esm
                .checked
                .insert((key.clone(), import_type));
        }
        match format {
            EsmFormat::Module | EsmFormat::Json => None,
            EsmFormat::Builtin | EsmFormat::CommonJs => {
                Some(self.node_esm_synthetic(key, format, &target))
            }
        }
    }

    fn node_esm_synthetic(
        &mut self,
        key: &ModuleKey,
        format: EsmFormat,
        target: &str,
    ) -> Result<Rc<RefCell<LoadedModule>>, JsValue> {
        if let Some(existing) = self.module_registry_get(key) {
            return Ok(existing);
        }
        let mut names: Vec<(String, JsValue)> = Vec::new();
        let evaluated = format == EsmFormat::Builtin;
        if evaluated {
            let exports =
                self.node_esm_call(&[js_str("load"), js_str("builtin"), js_str(target)])?;
            names.push(("default".to_string(), exports.clone()));
            if let Some(id) = exports.as_object_id() {
                for name in crate::interpreter::helpers::enumerable_own_keys(self, id)? {
                    let name = name.to_string();
                    if name == "default" {
                        continue;
                    }
                    match self.get_object_property(id, name.as_str(), &exports) {
                        Completion::Normal(value) => names.push((name, value)),
                        Completion::Throw(error) => return Err(error),
                        _ => {}
                    }
                }
            }
        } else {
            names.push(("default".to_string(), JsValue::UNDEFINED));
            names.push(("module.exports".to_string(), JsValue::UNDEFINED));
        }

        let env = Environment::new_function_scope(Some(self.realm().global_env.clone()));
        env.borrow_mut().strict = true;
        let mut exports = HashMap::new();
        let mut export_bindings = HashMap::new();
        for (name, value) in names {
            let binding = format!("*esm:{name}*");
            env.borrow_mut().declare(&binding, BindingKind::Const);
            if evaluated {
                env.borrow_mut().initialize_binding(&binding, value.clone());
            }
            exports.insert(name.clone(), value);
            export_bindings.insert(name, binding);
        }
        let module = Rc::new(RefCell::new(LoadedModule {
            path: key.clone(),
            env,
            exports,
            export_bindings,
            cached_namespace: None,
            cached_deferred_namespace: None,
            cached_import_meta: None,
            error: None,
            namespace_imports: HashMap::new(),
            synthetic_namespace_imports: HashMap::new(),
            source_imports: HashMap::new(),
            module_source: None,
            star_export_sources: Vec::new(),
            evaluated,
            is_evaluating: false,
            deferred_only: false,
            has_tla: false,
            program_ast: None,
            requested_modules: Vec::new(),
            async_evaluation_order: None,
            pending_async_dependencies: 0,
            async_parent_modules: Vec::new(),
            cycle_root: None,
            top_level_capability: None,
            dfs_index: None,
            dfs_ancestor_index: None,
        }));
        self.module_registry_insert(key.clone(), module.clone());
        Ok(module)
    }

    /// Run a CommonJS module record's body: load it through the CommonJS
    /// loader and bind `default` and `module.exports` to its `module.exports`.
    /// `None` when `key` is not such a record.
    pub(super) fn node_esm_execute(&mut self, key: &ModuleKey) -> Option<Result<(), JsValue>> {
        let (format, target) = self.node_host.esm.targets.get(key).cloned()?;
        if format != EsmFormat::CommonJs {
            return None;
        }
        let module = self.module_registry_get(key)?;
        let loaded = self.node_esm_call(&[js_str("load"), js_str("commonjs"), js_str(&target)]);
        let value = match loaded {
            Ok(value) => value,
            Err(error) => {
                module.borrow_mut().error = Some(error.clone());
                return Some(Err(error));
            }
        };
        let env = module.borrow().env.clone();
        for name in ["default", "module.exports"] {
            env.borrow_mut()
                .initialize_binding(&format!("*esm:{name}*"), value.clone());
            module
                .borrow_mut()
                .exports
                .insert(name.to_string(), value.clone());
        }
        Some(Ok(()))
    }

    /// `import()` resolves against the module the calling code was compiled
    /// for (the CommonJS wrapper's file, or the ES module), not against the
    /// main program, and always loads from a microtask like Node.
    pub(super) fn dynamic_import_from(
        &mut self,
        specifier: &str,
        import_type: Option<ImportModuleType>,
        env: &EnvRef,
    ) -> Completion {
        if !self.node_host.esm.is_active() {
            return self.dynamic_import(specifier, import_type);
        }
        let referrer =
            Environment::find_module_path(env).or_else(|| self.current_module_path.clone());
        self.static_module_load_depth += 1;
        let result = self.with_module_key(referrer, |interp| {
            interp.dynamic_import(specifier, import_type)
        });
        self.static_module_load_depth -= 1;
        result
    }

    pub(super) fn node_esm_enter_main(&mut self, key: &ModuleKey) {
        if self.node_host.esm.is_active() {
            self.node_host.esm.main = Some(key.clone());
        }
    }

    /// The main module has finished its first run; later failures of its
    /// top-level await are uncaught exceptions.
    pub(crate) fn node_esm_main_ready(&mut self) {
        self.node_host.esm.main_ready = true;
    }

    /// A top-level await of the main module rejected after the first run.
    pub(super) fn node_esm_main_rejected(&mut self, key: &ModuleKey, error: &JsValue) {
        let esm = &self.node_host.esm;
        if esm.main_ready && esm.main.as_ref() == Some(key) {
            self.node_uncaught(error.clone());
        }
    }

    /// Node exits 13 when the event loop empties while the main module's
    /// top-level await is still pending.
    pub(crate) fn node_esm_unsettled_main(&mut self) -> bool {
        let Some(key) = self.node_host.esm.main.clone() else {
            return false;
        };
        let Some(module) = self.module_registry_get(&key) else {
            return false;
        };
        let module = module.borrow();
        !module.evaluated && module.error.is_none()
    }
}
