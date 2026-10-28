//! Own keys of the errors the engine throws under `--node` (issue #1379). V8
//! gives them `stack` and `message` and no `name`, which they read from their
//! prototype; mocha's `json` reporter lists the keys in that order.

use super::super::*;

impl Interpreter {
    pub(super) fn node_inherit_error_name(&mut self, id: u64) {
        if !self.node_host.stack.installed {
            return;
        }
        if let Some(obj) = self.get_object(id) {
            let mut error = obj.borrow_mut();
            if error.prototype_id.is_some() {
                error.properties.remove("name");
            }
        }
    }
}
