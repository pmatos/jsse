use super::*;

/// PerformEval step: the private names of every class body enclosing the
/// caller, innermost to outermost.
pub(super) fn enclosing_private_names(caller_env: &EnvRef) -> Option<HashSet<String>> {
    let mut found: Option<HashSet<String>> = None;
    let mut env_walk = Some(caller_env.clone());
    while let Some(e) = env_walk {
        let borrowed = e.borrow();
        if let Some(ref names) = borrowed.class_private_names {
            found
                .get_or_insert_with(HashSet::new)
                .extend(names.names().map(str::to_string));
        }
        env_walk = borrowed.parent.clone();
    }
    found
}
