//! The ES module host hook for `--node` (issue #1240). Inert until the
//! prelude installs its host function with `__host_esm_install`; without it
//! the engine's own relative/absolute resolution and file loading apply, so
//! test262 never reaches this file. The host function (JavaScript, in
//! `node-lib/internal/modules/esm/host.js`) resolves specifiers with Node's
//! algorithm and loads `commonjs` and `builtin` modules; Rust turns its answers
//! into Module Records and keeps the engine's linking and evaluation. See
//! `docs/adr/2026-10-26-1700-node-esm-loader.md`.

use super::*;

mod meta;
mod require;
pub(super) use require::virtual_source;

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
    cjs_order: HashMap<ModuleKey, Vec<String>>,
    checked: HashSet<(ModuleKey, Option<ImportModuleType>)>,
    main: Option<ModuleKey>,
    main_ready: bool,
    entry_reported: bool,
}

impl NodeEsm {
    pub(crate) fn install(&mut self, host: JsValue) {
        self.host = Some(host);
    }

    pub(crate) fn is_active(&self) -> bool {
        self.host.is_some()
    }

    pub(crate) fn has_main(&self) -> bool {
        self.main.is_some()
    }

    /// Whether a rejection of the entry point's top-level await was already
    /// delivered, so the throw the first run surfaces is not delivered again.
    pub(crate) fn entry_reported(&self) -> bool {
        self.entry_reported
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
        if referrer.is_some()
            && let Some(key) = self.node_host.esm.lookup(specifier, referrer)
        {
            return Ok(key);
        }
        let parent = referrer.map_or(JsValue::UNDEFINED, |p| js_str(&p.to_string_lossy()));
        let answer = self.node_esm_call(&[js_str("resolve"), js_str(specifier), parent])?;
        let (format, target) = self.node_esm_answer(&answer)?;
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

    fn node_esm_answer(&mut self, answer: &JsValue) -> Result<(EsmFormat, String), JsValue> {
        let part = |interp: &mut Interpreter, index: usize| -> Option<String> {
            let value = interp.get_property_on_id(answer.as_object_id()?, &index.to_string());
            value
                .is_string()
                .then(|| interp.to_string_lossy(&value).ok())?
        };
        let format = part(self, 0).and_then(|name| EsmFormat::parse(&name));
        match (format, part(self, 1)) {
            (Some(format), Some(target)) => Ok((format, target)),
            _ => Err(self.create_error(
                "TypeError",
                "the ES module host hook returned a malformed [format, target] answer",
            )),
        }
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

    /// `default` plus every enumerable own property of a builtin module's
    /// exports. Property getters run JavaScript, so everything gathered so far
    /// stays rooted until the walk ends.
    fn node_esm_builtin_names(&mut self, target: &str) -> Result<Vec<(String, JsValue)>, JsValue> {
        let exports = self.node_esm_call(&[js_str("load"), js_str("builtin"), js_str(target)])?;
        let frame = self.gc_root_frame();
        self.gc_root_value(&exports);
        let mut names = vec![("default".to_string(), exports.clone())];
        let result = self.node_esm_collect_names(&exports, &mut names);
        self.gc_unroot_frame(frame);
        result.map(|()| names)
    }

    fn node_esm_collect_names(
        &mut self,
        exports: &JsValue,
        names: &mut Vec<(String, JsValue)>,
    ) -> Result<(), JsValue> {
        let Some(id) = exports.as_object_id() else {
            return Ok(());
        };
        for name in crate::interpreter::helpers::enumerable_own_keys(self, id)? {
            let name = name.to_string();
            if name == "default" {
                continue;
            }
            match self.get_object_property(id, name.as_str(), exports) {
                Completion::Normal(value) => {
                    self.gc_root_value(&value);
                    names.push((name, value));
                }
                Completion::Throw(error) => return Err(error),
                _ => {}
            }
        }
        Ok(())
    }

    /// The names a CommonJS module exports to `import`: those its source
    /// assigns to `exports` (the host follows re-exports), then `default` and
    /// `module.exports`.
    fn node_esm_cjs_names(&mut self, target: &str) -> Result<Vec<(String, JsValue)>, JsValue> {
        let list = self.node_esm_call(&[js_str("names"), js_str(target)])?;
        let mut names = Vec::new();
        if let Some(id) = list.as_object_id() {
            let length = self.get_property_on_id(id, "length");
            for index in 0..length.as_number().unwrap_or(0.0) as usize {
                let item = self.get_property_on_id(id, &index.to_string());
                if item.is_string() {
                    names.push(self.to_string_lossy(&item)?);
                }
            }
        }
        for fixed in ["default", "module.exports"] {
            if !names.iter().any(|n| n == fixed) {
                names.push(fixed.to_string());
            }
        }
        Ok(names.into_iter().map(|n| (n, JsValue::UNDEFINED)).collect())
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
        let evaluated = format == EsmFormat::Builtin;
        let names = if evaluated {
            self.node_esm_builtin_names(target)?
        } else {
            self.node_esm_cjs_names(target)?
        };

        if !evaluated {
            let order = names.iter().map(|(name, _)| name.clone()).collect();
            self.node_host.esm.cjs_order.insert(key.clone(), order);
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
        let named = self
            .node_host
            .esm
            .cjs_order
            .get(key)
            .cloned()
            .unwrap_or_default();
        let frame = self.gc_root_frame();
        self.gc_root_value(&value);
        for name in named {
            let item = match name.as_str() {
                "default" | "module.exports" => value.clone(),
                _ => self.node_esm_cjs_named(&value, &name),
            };
            self.gc_root_value(&item);
            env.borrow_mut()
                .initialize_binding(&format!("*esm:{name}*"), item.clone());
            module.borrow_mut().exports.insert(name, item);
        }
        self.gc_unroot_frame(frame);
        Some(Ok(()))
    }

    /// `exports[name]` when it is an own property, swallowing a throwing
    /// getter, else `undefined`.
    fn node_esm_cjs_named(&mut self, exports: &JsValue, name: &str) -> JsValue {
        let Some(id) = exports.as_object_id() else {
            return JsValue::UNDEFINED;
        };
        match self.proxy_get_own_property_descriptor(id, name) {
            Ok(descriptor) if !descriptor.is_undefined() => {
                match self.get_object_property(id, name, exports) {
                    Completion::Normal(value) => value,
                    _ => JsValue::UNDEFINED,
                }
            }
            _ => JsValue::UNDEFINED,
        }
    }

    pub(super) fn import_referrer(&self, env: &EnvRef) -> Option<ModuleKey> {
        if self.node_host.esm.is_active()
            && let Some(key) = Environment::find_module_path(env)
        {
            return Some(key);
        }
        self.current_module_path.clone()
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
        let referrer = self.import_referrer(env);
        self.static_module_load_depth += 1;
        let result = self.with_module_key(referrer, |interp| {
            interp.dynamic_import(specifier, import_type)
        });
        self.static_module_load_depth -= 1;
        result
    }

    /// `key` is the program entry: the ES main the launcher ran, or the script
    /// file a CommonJS main detected as an ES module and handed to the loader.
    fn node_esm_is_main(&self, key: &ModuleKey) -> bool {
        let key = key.canonicalize();
        let script = self
            .node_host
            .argv
            .as_ref()
            .and_then(|argv| argv.script.clone())
            .map(ModuleKey::for_file);
        self.node_host.esm.main.as_ref() == Some(&key) || script == Some(key)
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
            self.node_host.esm.entry_reported = true;
            self.node_uncaught_entry(
                error.clone(),
                event_loop::node_event_loop::ESM_ENTRY_HOPS + 1,
            );
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
