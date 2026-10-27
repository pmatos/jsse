//! Host module resolution and the synthetic (JSON/text/bytes) module records,
//! split out of `mod.rs`.

use super::*;

impl Interpreter {
    pub(super) fn resolve_module_specifier(
        &mut self,
        specifier: &str,
        referrer: Option<&Path>,
    ) -> Result<ModuleKey, JsValue> {
        // Host-provided synthetic module. Resolved here rather than in the
        // source-phase path so that every phase names the same Module Record.
        if specifier == MODULE_SOURCE_SPECIFIER {
            return Ok(ModuleKey::module_source());
        }

        if self.node_host.esm.is_active() {
            return self.node_esm_resolve(specifier, referrer);
        }

        // Relative paths: ./ or ../
        if specifier.starts_with("./") || specifier.starts_with("../") {
            if let Some(referrer) = referrer {
                let base = referrer.parent().unwrap_or(Path::new("."));
                let resolved = base.join(specifier);
                if resolved.exists() {
                    return Ok(ModuleKey::for_file(resolved));
                }
                return Err(JsValue::string(JsString::from_str(&format!(
                    "Cannot find module '{}'",
                    specifier
                ))));
            } else {
                return Err(JsValue::string(JsString::from_str(
                    "Relative imports require a referrer path",
                )));
            }
        }

        // Absolute paths
        let path = Path::new(specifier);
        if path.is_absolute() && path.exists() {
            return Ok(ModuleKey::for_file(path.to_path_buf()));
        }

        // Bare specifiers not supported
        Err(JsValue::string(JsString::from_str(&format!(
            "Cannot resolve bare module specifier '{}'",
            specifier
        ))))
    }

    pub(super) fn create_synthetic_default_module(
        &mut self,
        canon_path: ModuleKey,
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
            synthetic_namespace_imports: HashMap::new(),
            source_imports: HashMap::new(),
            module_source: None,
            star_export_sources: Vec::new(),
            evaluated: true,
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
        }))
    }

    /// Load a resource as the synthetic default-export module for `import_type`,
    /// memoized per realm by canonical path and type. All three representations
    /// share the read/cache/registry path; only the value construction differs.
    pub(super) fn load_typed_module(
        &mut self,
        key: &ModuleKey,
        path: &Path,
        import_type: ImportModuleType,
    ) -> Result<Rc<RefCell<LoadedModule>>, JsValue> {
        let canon = key.clone();
        let registry_key = (self.current_realm_id, canon.clone(), import_type);
        if let Some(existing) = self.synthetic_module_registry.get(&registry_key) {
            return Ok(existing.clone());
        }
        let value = match import_type {
            ImportModuleType::Json => {
                let source = Self::read_module_text(path)?;
                let text = crate::interpreter::helpers::JsonText::from_str(&source);
                match crate::interpreter::helpers::json_parse_value(self, &text) {
                    Completion::Normal(value) => value,
                    Completion::Throw(error) => return Err(error),
                    _ => {
                        return Err(self.create_error(
                            "SyntaxError",
                            &format!("JSON parse error in '{}'", path.display()),
                        ));
                    }
                }
            }
            ImportModuleType::Text => {
                let source = Self::read_module_text(path)?;
                JsValue::string(JsString::from_str(&source))
            }
            ImportModuleType::Bytes => {
                let bytes = std::fs::read(path).map_err(|e| Self::module_read_error(path, &e))?;
                self.create_immutable_uint8array(&bytes)
            }
        };
        let module = self.create_synthetic_default_module(canon, value);
        self.synthetic_module_registry
            .insert(registry_key, module.clone());
        Ok(module)
    }

    pub(super) fn read_module_text(path: &Path) -> Result<String, JsValue> {
        if let Some(source) = node_esm::virtual_source(path) {
            return Ok(source);
        }
        std::fs::read_to_string(path).map_err(|e| Self::module_read_error(path, &e))
    }

    pub(super) fn module_read_error(path: &Path, err: &std::io::Error) -> JsValue {
        JsValue::string(JsString::from_str(&format!(
            "Cannot read module '{}': {}",
            path.display(),
            err
        )))
    }

    pub(super) fn create_immutable_uint8array(&mut self, bytes: &[u8]) -> JsValue {
        use std::cell::Cell;
        let len = bytes.len();
        let buf_rc = Rc::new(RefCell::new(BufferData::Owned(bytes.to_vec())));
        let detached = Rc::new(Cell::new(false));

        let ab_obj_id = self.create_object_id();
        {
            let mut ab = self.get_object_cell_expect(ab_obj_id).borrow_mut();
            ab.class_name = "ArrayBuffer";
            ab.prototype_id = self.realm().arraybuffer_prototype;
            ab.kind = crate::interpreter::types::ObjectKind::ArrayBuffer(Box::new(
                crate::interpreter::types::ArrayBufferData {
                    data: buf_rc.clone(),
                    detached: Some(detached.clone()),
                    max_byte_length: None,
                    is_shared: false,
                    is_immutable: true,
                    sab_shared: None,
                },
            ));
        }
        self.gc_track_external_bytes(len);
        let ab_id = ab_obj_id;
        let buf_val = JsValue::object(ab_id);

        let ta_info = TypedArrayInfo {
            kind: TypedArrayKind::Uint8,
            buffer: buf_rc,
            byte_offset: 0,
            byte_length: len,
            array_length: len,
            is_detached: detached,
            is_length_tracking: false,
            buffer_object_id: None,
        };

        let proto_id = self.realm().uint8array_prototype.unwrap();
        let ta_id = self.create_typed_array_object_with_proto(ta_info, buf_val, proto_id);
        JsValue::object(ta_id)
    }

    /// Like resolve_module_specifier but doesn't need &mut self
    pub(super) fn resolve_module_specifier_pure(
        &self,
        specifier: &str,
        referrer: Option<&Path>,
    ) -> Result<ModuleKey, JsValue> {
        if specifier == MODULE_SOURCE_SPECIFIER {
            return Ok(ModuleKey::module_source());
        }
        if self.node_host.esm.is_active() {
            return self
                .node_host
                .esm
                .lookup(specifier, referrer)
                .ok_or(JsValue::UNDEFINED);
        }
        if specifier.starts_with("./") || specifier.starts_with("../") || specifier.starts_with('/')
        {
            let base = referrer.and_then(|r| r.parent()).unwrap_or(Path::new("."));
            let resolved = base.join(specifier);
            if resolved.exists() {
                return Ok(ModuleKey::for_file(resolved));
            }
            // Try .js extension
            let with_js = resolved.with_extension("js");
            if with_js.exists() {
                return Ok(ModuleKey::for_file(with_js));
            }
            // Try /index.js
            let index = resolved.join("index.js");
            if index.exists() {
                return Ok(ModuleKey::for_file(index));
            }
        }
        Err(JsValue::UNDEFINED)
    }
}
