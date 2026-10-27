//! Floor for the ES module loader of Node host mode (issue #1240). The
//! prelude installs one host function that resolves and loads modules
//! (`node-lib/internal/modules/esm/host.js`) and tags the functions it
//! compiles with their file, so `import()` inside them resolves against it;
//! the engine side is `src/interpreter/node_esm.rs`.

use super::super::*;

impl Interpreter {
    pub(super) fn setup_node_host_esm(&mut self) {
        let install = self.create_function(JsFunction::native(
            "__host_esm_install".to_string(),
            1,
            |interp, _this, args| {
                let host = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                if let Some(global) = interp.realm().global_object {
                    interp.pin_native_root(&JsValue::object(global), &host);
                }
                interp.node_host.esm.install(host);
                Completion::Normal(JsValue::UNDEFINED)
            },
        ));
        self.install_host_global("__host_esm_install", install);

        // __host_tag_function(fn, filename): `fn` came from the Function
        // constructor, whose closure environment is unique to it.
        let tag = self.create_function(JsFunction::native(
            "__host_tag_function".to_string(),
            2,
            |interp, _this, args| {
                let filename =
                    match interp.to_string_lossy(args.get(1).unwrap_or(JsValue::undefined_ref())) {
                        Ok(s) => s,
                        Err(e) => return Completion::Throw(e),
                    };
                let cell = args
                    .first()
                    .and_then(JsValue::as_object_id)
                    .and_then(|id| interp.get_object_cell(id));
                if let Some(cell) = cell
                    && let Some(JsFunction::User { closure, .. }) =
                        cell.borrow().callable.as_deref()
                {
                    closure.borrow_mut().module_path =
                        Some(ModuleKey::for_file(std::path::PathBuf::from(filename)));
                }
                Completion::Normal(JsValue::UNDEFINED)
            },
        ));
        self.install_host_global("__host_tag_function", tag);

        let require = self.create_function(JsFunction::native(
            "__host_esm_require".to_string(),
            1,
            |interp, _this, args| {
                let filename = match interp
                    .to_string_lossy(args.first().unwrap_or(JsValue::undefined_ref()))
                {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                match interp.node_esm_require(&filename) {
                    Ok(value) => Completion::Normal(value),
                    Err(error) => Completion::Throw(error),
                }
            },
        ));
        self.install_host_global("__host_esm_require", require);

        let facade = self.create_function(JsFunction::native(
            "__host_esm_facade".to_string(),
            1,
            |interp, _this, args| {
                let namespace = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                match interp.node_esm_facade(&namespace) {
                    Ok(value) => Completion::Normal(value),
                    Err(error) => Completion::Throw(error),
                }
            },
        ));
        self.install_host_global("__host_esm_facade", facade);
        self.setup_node_host_cjs_lexer();
    }
}
