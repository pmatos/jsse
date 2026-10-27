pub(crate) mod array;
pub(crate) mod array_dense;
mod atomics;
pub(crate) mod bigint;
mod collections;
mod date;
mod disposable;
mod dynamic_source;
mod errors;
mod find_via_predicate;
mod function;
mod function_constructors;
mod global_functions;
mod global_object;
mod host;
mod host_base64;
mod intl;
mod iterators;
mod json;
mod math;
mod merge_sort;
pub(crate) mod node_host;
mod node_host_builtins;
mod node_host_crypto;
mod node_host_esm;
#[cfg(unix)]
mod node_host_fs;
#[cfg(unix)]
mod node_host_fs_args;
#[cfg(unix)]
mod node_host_fs_async;
#[cfg(unix)]
mod node_host_fs_error;
#[cfg(unix)]
mod node_host_fs_jobs;
#[cfg(unix)]
mod node_host_fs_meta;
#[cfg(unix)]
mod node_host_fs_ops;
mod node_host_idna;
mod node_host_loop;
mod node_host_os;
mod node_host_os_constants;
mod node_host_os_sys;
mod node_host_process;
mod node_host_process_metrics;
pub(crate) mod node_host_stack;
mod node_host_stack_sources;
#[cfg(unix)]
pub(crate) mod node_host_tty;
mod node_host_util;
mod node_prelude;
mod number;
mod object;
pub(crate) mod plural_rules_state;
mod predicate_quantification;
mod primitive_constructors;
mod promise;
mod proxy;
mod reduction;
mod reflect;
pub(crate) mod regexp;
mod regexp_lookbehind;
mod shadow_realm;
pub(crate) mod string;
mod structured_clone;
mod temporal;
pub(crate) mod typedarray;

use super::*;

fn is_identifier_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        None => return false,
        Some(c) => {
            if !unicode_ident::is_xid_start(c) && c != '$' && c != '_' {
                return false;
            }
        }
    }
    for c in chars {
        if !unicode_ident::is_xid_continue(c) && c != '$' && c != '\u{200C}' && c != '\u{200D}' {
            return false;
        }
    }
    true
}

fn sanitize_native_fn_name(name: &str) -> String {
    if name.is_empty() {
        return String::new();
    }
    // Handle "get X" / "set X" accessor prefix
    if let Some(rest) = name.strip_prefix("get ") {
        if is_identifier_name(rest) {
            return format!("get {rest}");
        }
        return "get ".to_string();
    }
    if let Some(rest) = name.strip_prefix("set ") {
        if is_identifier_name(rest) {
            return format!("set {rest}");
        }
        return "set ".to_string();
    }
    // Handle bracket notation like "[Symbol.iterator]"
    if name.starts_with('[') && name.ends_with(']') {
        return name.to_string();
    }
    if name.starts_with('[') {
        return String::new();
    }
    if is_identifier_name(name) {
        return name.to_string();
    }
    String::new()
}

impl Interpreter {
    pub(crate) fn setup_globals(&mut self) {
        // Create a placeholder Function.prototype early so all functions created during
        // setup get the correct [[Prototype]]. It will be adopted by Function.prototype later.
        if self.realm().function_prototype.is_none() {
            let fp_id = self.create_object_id();
            self.get_object_cell_expect(fp_id).borrow_mut().prototype_id =
                self.realm().object_prototype;
            self.get_object_cell_expect(fp_id).borrow_mut().callable =
                Some(Box::new(JsFunction::native(String::new(), 0, |_, _, _| {
                    Completion::Normal(JsValue::UNDEFINED)
                })));
            self.get_object_cell_expect(fp_id).borrow_mut().class_name = "Function";
            self.realm_mut().function_prototype = Some(fp_id);
        }

        // Create %ThrowTypeError% intrinsic (§10.2.4) — must exist before anything uses it
        {
            let thrower = self.create_thrower_function();
            if let Some(thrower_id) = thrower.as_object_id()
                && let Some(obj) = self.get_object_cell(thrower_id)
            {
                let mut b = obj.borrow_mut();
                b.extensible = false;
                b.insert_property(
                    "length".to_string(),
                    PropertyDescriptor::data(JsValue::number(0.0), false, false, false),
                );
                b.insert_property(
                    "name".to_string(),
                    PropertyDescriptor::data(
                        JsValue::string(JsString::from_str("")),
                        false,
                        false,
                        false,
                    ),
                );
            }
            self.realm_mut().throw_type_error = Some(thrower);
        }

        self.setup_host_globals();

        self.setup_error_builtins();

        self.setup_object_constructor();

        self.setup_object_statics();

        self.setup_array_constructor();

        self.setup_symbol_constructor();

        self.setup_iterator_prototypes();
        self.setup_generator_prototype();
        self.setup_async_generator_prototype();
        self.setup_array_prototype();
        self.setup_string_constructor();
        self.setup_string_prototype();

        self.setup_string_raw();

        self.setup_number_constructor();

        self.setup_number_statics();

        self.setup_boolean_constructor();

        self.setup_bigint_prototype();
        self.setup_symbol_prototype();
        self.cached_has_instance_key = self.get_symbol_key("hasInstance");
        self.setup_number_prototype();
        self.setup_boolean_prototype();
        self.setup_map_prototype();
        self.setup_set_prototype();
        self.setup_weakmap_prototype();
        self.setup_weakset_prototype();
        self.setup_weakref();
        self.setup_finalization_registry();
        self.setup_date_builtin();
        self.setup_disposable_stack();
        self.setup_async_disposable_stack();

        self.setup_global_functions();

        self.setup_math();

        self.setup_function_constructors();

        // JSON object
        self.setup_json();

        self.setup_string_from_char_code();

        // RegExp constructor and prototype
        self.setup_regexp();

        // Reflect and Proxy built-ins
        self.setup_reflect();
        self.setup_proxy();

        // TypedArray, ArrayBuffer, DataView built-ins
        self.setup_typedarray_builtins();

        // Atomics built-in
        self.setup_atomics();

        // Promise built-in
        self.setup_promise();

        // Temporal built-in
        self.setup_temporal();

        // Intl built-in
        self.setup_intl();

        // ShadowRealm built-in
        self.setup_shadow_realm();

        self.setup_global_object();
    }
}
