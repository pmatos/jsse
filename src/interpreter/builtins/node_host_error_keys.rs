//! Own keys of the errors the engine throws under `--node` (issue #1379). V8
//! gives them `stack` and `message` and no `name`, which they read from their
//! prototype; mocha's `json` reporter lists the keys in that order.

use super::super::*;

impl Interpreter {
    pub(super) fn node_inherit_error_name(&mut self, id: u64) {
        if !self.node_host.stack.installed {
            return;
        }
        let Some(own) = self.own_data_string(id, "name") else {
            return;
        };
        let Some(proto) = self.get_object(id).and_then(|o| o.borrow().prototype_id) else {
            return;
        };
        let inherited = self.get_property_on_id(proto, "name");
        if inherited.as_string().is_some_and(|s| s.to_string() == own)
            && let Some(error) = self.get_object(id)
        {
            error.borrow_mut().remove_property("name");
        }
    }
}
