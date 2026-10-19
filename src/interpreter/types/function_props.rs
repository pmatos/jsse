//! Construction helpers for function objects and their own-name bindings.

use super::*;

impl Environment {
    pub(crate) fn declare_function_name(&mut self, name: Atom) {
        self.bindings.push_unique(
            name,
            Binding {
                value: JsValue::UNDEFINED,
                kind: BindingKind::FunctionName,
                initialized: false,
                deletable: false,
            },
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
    pub(crate) fn with_properties(properties: PropertyMap) -> Self {
        let mut data = Self::new();
        data.properties = properties;
        data
    }

    /// Append an own property to an object whose map came from
    /// `PropertyMap::with_exact_capacity` and has room for it: the key is
    /// absent, interned and not index-like (a `CommonKeys` key).
    pub(crate) fn push_exact_property(&mut self, key: &JsPropertyKey, desc: PropertyDescriptor) {
        debug_assert!(self.array_data().is_none());
        self.properties.push_non_index(key.clone(), desc);
    }
}
