//! Construction helpers for function objects and their own-name bindings.

use super::*;

impl Environment {
    pub(crate) fn declare_function_name(&mut self, name: Atom) {
        self.bindings.push_unique(
            name,
            Binding::new(JsValue::UNDEFINED, BindingKind::FunctionName, false),
        );
    }

    pub(crate) fn initialize_function_name(&mut self, value: JsValue) {
        let (_, binding) = self
            .bindings
            .entry_at_mut(0)
            .expect("declare_function_name made entry 0");
        binding.value = value;
        binding.initialized = true;
    }
}

impl JsObjectData {
    /// Append an own data property to an object whose map has room for it
    /// (one from `PropertyMap::with_exact_capacity`, or a fresh empty one):
    /// the key is absent, interned and not index-like (a `CommonKeys` key).
    #[inline]
    pub(crate) fn push_exact_data(
        &mut self,
        key: &JsPropertyKey,
        value: JsValue,
        writable: bool,
        enumerable: bool,
        configurable: bool,
    ) {
        debug_assert!(self.array_data().is_none());
        self.properties.push_non_index_record(
            key.clone(),
            PropertyRecord::data(value, writable, enumerable, configurable),
        );
    }

    /// `push_exact_data` for a getter-only accessor property.
    #[inline]
    pub(crate) fn push_exact_getter(
        &mut self,
        key: &JsPropertyKey,
        getter: JsValue,
        enumerable: bool,
        configurable: bool,
    ) {
        debug_assert!(self.array_data().is_none());
        self.properties.push_non_index_record(
            key.clone(),
            PropertyRecord::getter_only(getter, enumerable, configurable),
        );
    }
}
