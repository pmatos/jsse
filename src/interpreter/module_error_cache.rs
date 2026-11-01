use super::*;

impl Interpreter {
    pub(super) fn cache_module_error(module: &Rc<RefCell<LoadedModule>>, err: &JsValue) {
        module.borrow_mut().error = Some(err.clone());
    }
}
