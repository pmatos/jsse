//! `require()` of an ES module (issue #1241): load, link and evaluate the
//! graph synchronously from CommonJS, and the namespace facade Node adds when
//! the module has a default export. See
//! `docs/adr/2026-10-26-2300-node-esm-interop.md`.

use super::super::*;
use super::{EsmFormat, js_str};
use std::cell::RefCell as StdRefCell;

thread_local! {
    static VIRTUAL_SOURCES: StdRefCell<HashMap<PathBuf, String>> = StdRefCell::new(HashMap::new());
}

/// Source text of a module that has no file (the require facade).
pub(in crate::interpreter) fn virtual_source(path: &Path) -> Option<String> {
    VIRTUAL_SOURCES.with(|m| m.borrow().get(path).cloned())
}

const FACADE_SOURCE: &str = "export * from 'original';\nexport { default } from 'original';\nexport const __esModule = true;\n";

enum Graph {
    Ready,
    Cycle,
    Async,
}

impl Interpreter {
    /// Whether the graph under `root` can be evaluated synchronously: no
    /// module in it is mid-evaluation (a cycle through `require`) or uses
    /// top-level await.
    fn node_esm_graph(&self, root: &ModuleKey) -> Graph {
        let mut seen = HashSet::new();
        let mut todo = vec![root.canonicalize()];
        let mut is_async = false;
        while let Some(key) = todo.pop() {
            if !seen.insert(key.clone()) {
                continue;
            }
            let Some(module) = self.module_registry_get(&key) else {
                continue;
            };
            {
                let module = module.borrow();
                if module.is_evaluating {
                    return Graph::Cycle;
                }
                is_async |= module.has_tla;
            }
            todo.extend(self.get_module_dep_paths(&key).into_iter().map(|(k, _)| k));
        }
        if is_async { Graph::Async } else { Graph::Ready }
    }

    /// The namespace of the ES module at `filename`, evaluated; or the string
    /// `"async"` or `"cycle"` when the graph cannot be required.
    pub(in crate::interpreter) fn node_esm_require(
        &mut self,
        filename: &str,
    ) -> Result<JsValue, JsValue> {
        let key = ModuleKey::for_file(PathBuf::from(filename));
        self.node_host
            .esm
            .targets
            .entry(key.clone())
            .or_insert((EsmFormat::Module, filename.to_string()));
        let module = self.load_module_for_type(&key, None, ModuleLoadMode::Evaluate)?;
        match self.node_esm_graph(&key) {
            Graph::Cycle => return Ok(js_str("cycle")),
            Graph::Async => return Ok(js_str("async")),
            Graph::Ready => {}
        }
        self.node_esm_evaluate(&key)?;
        Ok(self.create_module_namespace(&module))
    }

    fn node_esm_evaluate(&mut self, key: &ModuleKey) -> Result<(), JsValue> {
        let canon = key.canonicalize();
        let mut stack = vec![];
        if let Err(error) = self.inner_module_evaluation(&canon, &mut stack, 0) {
            for path in &stack {
                if let Some(m) = self.module_registry_get(path) {
                    let mut m = m.borrow_mut();
                    m.evaluated = true;
                    m.is_evaluating = false;
                    if m.error.is_none() {
                        m.error = Some(error.clone());
                    }
                }
            }
            return Err(error);
        }
        Ok(())
    }

    /// The facade namespace of the module `namespace` belongs to: its exports
    /// plus `default` and `__esModule: true`, with live bindings.
    pub(in crate::interpreter) fn node_esm_facade(
        &mut self,
        namespace: &JsValue,
    ) -> Result<JsValue, JsValue> {
        let original = namespace
            .as_object_id()
            .and_then(|id| self.get_object_cell(id))
            .and_then(|cell| match &cell.borrow().kind {
                ObjectKind::ModuleNamespace(data) => data.module_path.clone(),
                _ => None,
            });
        let Some(original) = original.filter(|key| key.file_path().is_some()) else {
            return Err(self.create_error("TypeError", "not a module namespace"));
        };
        let mut name = original.to_string();
        name.push_str("?require-facade");
        let facade = ModuleKey::for_file(PathBuf::from(name));
        let Some(facade_path) = facade.file_path().map(Path::to_path_buf) else {
            return Err(self.create_error("TypeError", "no facade path"));
        };
        VIRTUAL_SOURCES.with(|m| {
            m.borrow_mut()
                .insert(facade_path.clone(), FACADE_SOURCE.to_string())
        });
        self.node_host
            .esm
            .resolved
            .insert((Some(facade_path), "original".to_string()), original);
        let module = self.load_module_for_type(&facade, None, ModuleLoadMode::Evaluate)?;
        self.node_esm_evaluate(&facade)?;
        Ok(self.create_module_namespace(&module))
    }
}
