use super::*;

impl Interpreter {
    /// Module environments are not necessarily reachable from `global_env`,
    /// and a record keeps handing out its cached namespaces and `import.meta`
    /// after user code has dropped them.
    pub(super) fn collect_module_roots(&self, roots: &mut Vec<u64>, seen_envs: &mut EnvVisits) {
        for module in self.module_registry.values() {
            let m = module.borrow();
            Self::collect_env_roots(&m.env, roots, seen_envs);
            for val in m.exports.values() {
                Self::collect_value_roots(val, roots);
            }
            let cached = [
                &m.module_source,
                &m.cached_namespace,
                &m.cached_deferred_namespace,
                &m.cached_import_meta,
                &m.error,
            ];
            for val in cached.into_iter().flatten() {
                Self::collect_value_roots(val, roots);
            }
            if let Some((promise, resolve, reject)) = &m.top_level_capability {
                Self::collect_value_roots(promise, roots);
                Self::collect_value_roots(resolve, roots);
                Self::collect_value_roots(reject, roots);
            }
        }
        for module in self.synthetic_module_registry.values() {
            let m = module.borrow();
            Self::collect_env_roots(&m.env, roots, seen_envs);
            for val in m.exports.values() {
                Self::collect_value_roots(val, roots);
            }
            let cached = [
                &m.module_source,
                &m.cached_namespace,
                &m.cached_deferred_namespace,
            ];
            for val in cached.into_iter().flatten() {
                Self::collect_value_roots(val, roots);
            }
        }
    }
}
