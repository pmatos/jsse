//! Module namespace exotic object state (ECMA-262 §10.4.6) and construction
//! (§16.2.1.10 GetModuleNamespace). Export-name membership is a hash lookup and
//! the whole record is `Rc`-shared, so cloning it per property access is O(1).

use super::{EnvRef, Interpreter, LoadedModule, ModuleKey};
use crate::interpreter::types::{ObjectKind, PropertyDescriptor};
use crate::types::{JsPropertyKey, JsString, JsValue};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// The sorted export-name list (§10.4.6.11 [[Exports]]) plus an O(1) index.
pub(crate) struct ExportNames {
    sorted: Vec<String>,
    index: HashSet<String>,
}

impl ExportNames {
    fn new(mut names: Vec<String>) -> Self {
        names.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
        let index = names.iter().cloned().collect();
        Self {
            sorted: names,
            index,
        }
    }

    pub(crate) fn contains(&self, name: &str) -> bool {
        self.index.contains(name)
    }

    pub(crate) fn iter(&self) -> std::slice::Iter<'_, String> {
        self.sorted.iter()
    }
}

#[derive(Clone)]
pub(crate) struct ModuleNamespaceData {
    pub env: EnvRef,
    pub export_names: Rc<ExportNames>,
    pub export_to_binding: Rc<HashMap<String, String>>,
    pub module_path: Option<ModuleKey>,
    pub deferred: bool,
}

impl Interpreter {
    pub(super) fn build_module_namespace(
        &mut self,
        module: &Rc<RefCell<LoadedModule>>,
        deferred: bool,
    ) -> JsValue {
        let obj_id = self.create_object_id();
        let (env, export_bindings, module_path, names) = {
            let module_ref = module.borrow();
            let module_path = if module_ref.path.is_empty() {
                None
            } else {
                Some(module_ref.path.clone())
            };
            let names: Vec<String> = module_ref
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
            (
                module_ref.env.clone(),
                module_ref.export_bindings.clone(),
                module_path,
                names,
            )
        };
        let export_names = Rc::new(ExportNames::new(names));

        let sym_key = self
            .get_symbol_key("toStringTag")
            .unwrap_or_else(|| JsPropertyKey::well_known_symbol("toStringTag"));
        let tag = if deferred {
            "Deferred Module"
        } else {
            "Module"
        };
        {
            let mut obj = self.get_object_cell_expect(obj_id).borrow_mut();
            obj.kind = ObjectKind::ModuleNamespace(Box::new(ModuleNamespaceData {
                env,
                export_names: export_names.clone(),
                export_to_binding: Rc::new(export_bindings),
                module_path,
                deferred,
            }));
            obj.class_name = "Module";
            obj.extensible = false;
            obj.set_proto(None);
            // writable=true, enumerable=true, configurable=false; the value is
            // looked up dynamically from the live binding.
            for name in export_names.iter() {
                obj.insert_property(
                    name.clone(),
                    PropertyDescriptor::data(JsValue::UNDEFINED, true, true, false),
                );
            }
            obj.insert_property(
                sym_key,
                PropertyDescriptor::data(
                    JsValue::string(JsString::from_str(tag)),
                    false,
                    false,
                    false,
                ),
            );
        }
        JsValue::object(obj_id)
    }
}
