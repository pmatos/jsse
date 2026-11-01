//! `import.meta` (§13.3.12.1). Under `--node` the host function fills in
//! `dirname`, `filename`, `main`, `resolve` and `url` like Node's
//! `initializeImportMeta`; otherwise the object carries only `url`.

use super::super::*;
use super::js_str;

impl Interpreter {
    pub(in crate::interpreter) fn eval_import_meta(&mut self, env: &EnvRef) -> Completion {
        // §16.2.1.5.2 GetActiveScriptOrModule: walk env chain to find module path
        let module_path =
            Environment::find_module_path(env).or_else(|| self.current_module_path.clone());
        if let Some(ref path) = module_path {
            let canon = path.canonicalize();
            if let Some(module) = self.module_registry_get(&canon)
                && let Some(ref cached) = module.borrow().cached_import_meta
            {
                return Completion::Normal(cached.clone());
            }
        }
        let meta_id = self.create_object_id();
        self.get_object_cell_expect(meta_id)
            .borrow_mut()
            .set_proto(None);
        let meta_val = JsValue::object(meta_id);
        let file = module_path
            .as_ref()
            .and_then(ModuleKey::file_path)
            .map(Path::to_path_buf);
        if self.node_host.esm.is_active() {
            if let Some(path) = file {
                let is_main = module_path
                    .as_ref()
                    .is_some_and(|key| self.node_esm_is_main(key));
                let args = [
                    js_str("meta"),
                    meta_val.clone(),
                    js_str(&path.to_string_lossy()),
                    JsValue::boolean(is_main),
                ];
                if let Err(error) = self.node_esm_call(&args) {
                    return Completion::Throw(error);
                }
            }
        } else if let Some(path) = file {
            let url = format!("file://{}", path.display());
            self.get_object_cell_expect(meta_id)
                .borrow_mut()
                .insert_property(
                    "url".to_string(),
                    PropertyDescriptor::data(
                        JsValue::string(JsString::from_str(&url)),
                        true,
                        true,
                        true,
                    ),
                );
        }
        if let Some(ref path) = module_path {
            let canon = path.canonicalize();
            if let Some(module) = self.module_registry_get(&canon) {
                module.borrow_mut().cached_import_meta = Some(meta_val.clone());
            }
        }
        Completion::Normal(meta_val)
    }
}
