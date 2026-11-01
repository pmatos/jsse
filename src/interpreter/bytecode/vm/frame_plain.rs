use super::*;

pub(super) fn is_plain_declarative(env: &EnvRef) -> bool {
    let scope = env.borrow();
    scope.with_object.is_none()
        && scope.indirect_bindings.is_none()
        && scope.global_object_id.is_none()
}
