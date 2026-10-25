use super::*;

/// PerformEval step: the private names of every class body enclosing the
/// caller. One class's scope is shared by its class, field-initializer and
/// static-block envs, so each distinct scope is read once.
pub(super) fn enclosing_private_names(caller_env: &EnvRef) -> Option<HashSet<String>> {
    let mut found: Option<HashSet<String>> = None;
    let mut seen: Vec<*const PrivateNameScope> = Vec::new();
    let mut env_walk = Some(caller_env.clone());
    while let Some(e) = env_walk {
        let borrowed = e.borrow();
        if let Some(ref scope) = borrowed.class_private_names {
            let ptr = Rc::as_ptr(scope);
            if !seen.contains(&ptr) {
                seen.push(ptr);
                found
                    .get_or_insert_with(HashSet::new)
                    .extend(scope.names().map(str::to_string));
            }
        }
        env_walk = borrowed.parent.clone();
    }
    found
}
