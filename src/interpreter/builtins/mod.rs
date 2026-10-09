pub(crate) mod array;
mod atomics;
pub(crate) mod bigint;
mod collections;
mod date;
mod disposable;
mod errors;
mod function;
mod function_constructors;
mod global_functions;
mod host;
mod intl;
mod iterators;
mod json;
mod math;
pub(crate) mod node_host;
mod number;
mod object;
mod primitive_constructors;
mod promise;
mod proxy;
mod reflect;
pub(crate) mod regexp;
mod regexp_lookbehind;
pub(crate) mod string;
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
                Some(JsFunction::native(String::new(), 0, |_, _, _| {
                    Completion::Normal(JsValue::UNDEFINED)
                }));
            self.get_object_cell_expect(fp_id).borrow_mut().class_name = "Function".to_string();
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

        // globalThis - create a global object
        let global_obj_id = self.create_object_id();
        let global_val = JsValue::object(global_obj_id);
        self.realm()
            .global_env
            .borrow_mut()
            .declare("globalThis", BindingKind::Var);
        let env = self.realm().global_env.clone();
        let _ = self.env_set(&env, "globalThis", global_val.clone());
        self.realm().global_env.borrow_mut().bindings.insert(
            "this".to_string(),
            Binding {
                value: global_val,
                kind: BindingKind::Const,
                initialized: true,
                deletable: false,
            },
        );

        // Populate globalThis with built-in constructors and functions as
        // non-enumerable, writable, configurable properties (per spec §19.1)
        let global_names = [
            "Object",
            "Function",
            "Array",
            "String",
            "Number",
            "Boolean",
            "Symbol",
            "Error",
            "SyntaxError",
            "TypeError",
            "ReferenceError",
            "RangeError",
            "URIError",
            "EvalError",
            "Date",
            "RegExp",
            "Map",
            "Set",
            "WeakMap",
            "WeakSet",
            "WeakRef",
            "FinalizationRegistry",
            "Promise",
            "ArrayBuffer",
            "DataView",
            "JSON",
            "Math",
            "Reflect",
            "Proxy",
            "eval",
            "parseInt",
            "parseFloat",
            "isNaN",
            "isFinite",
            "encodeURI",
            "decodeURI",
            "encodeURIComponent",
            "decodeURIComponent",
            "NaN",
            "Infinity",
            "undefined",
            "Int8Array",
            "Uint8Array",
            "Uint8ClampedArray",
            "Int16Array",
            "Uint16Array",
            "Int32Array",
            "Uint32Array",
            "Float16Array",
            "Float32Array",
            "Float64Array",
            "BigInt64Array",
            "BigUint64Array",
            "BigInt",
            "AggregateError",
            "SharedArrayBuffer",
            "Atomics",
            "Temporal",
            "Intl",
            "setTimeout",
            "setInterval",
            "clearTimeout",
            "clearInterval",
            "escape",
            "unescape",
            "DisposableStack",
            "AsyncDisposableStack",
            "SuppressedError",
            "ShadowRealm",
            "Iterator",
        ];
        let vals: Vec<(String, JsValue)> = global_names
            .iter()
            .filter_map(|name| self.get_global_var(name).map(|v| (name.to_string(), v)))
            .collect();
        for (name, val) in vals {
            let (writable, configurable) = match name.as_str() {
                "NaN" | "Infinity" | "undefined" => (false, false),
                _ => (true, true),
            };
            self.get_object_cell_expect(global_obj_id)
                .borrow_mut()
                .insert_property(
                    name,
                    PropertyDescriptor::data(val, writable, false, configurable),
                );
        }
        // Also set globalThis on itself
        let gt_val = JsValue::object(global_obj_id);
        self.get_object_cell_expect(global_obj_id)
            .borrow_mut()
            .insert_property(
                "globalThis".to_string(),
                PropertyDescriptor::data(gt_val, true, false, true),
            );

        // Fix .prototype descriptors on built-in constructors.
        // create_function sets writable=true (correct for user-defined constructors per §10.2.5),
        // but built-in constructors need writable=false per their respective spec sections.
        let builtin_ctors = [
            "Object",
            "Function",
            "Array",
            "RegExp",
            "Promise",
            "Error",
            "TypeError",
            "RangeError",
            "SyntaxError",
            "ReferenceError",
            "URIError",
            "EvalError",
            "DataView",
            "ArrayBuffer",
            "SharedArrayBuffer",
            "WeakRef",
            "FinalizationRegistry",
            "ShadowRealm",
        ];
        let ctor_vals: Vec<JsValue> = builtin_ctors
            .iter()
            .filter_map(|name| self.get_global_var(name))
            .collect();
        for ctor_val in &ctor_vals {
            if let Some(ctor_id) = ctor_val.as_object_id()
                && let Some(ctor_obj) = self.get_object_cell(ctor_id)
            {
                let proto_val = ctor_obj.borrow().get_property_value("prototype");
                if let Some(val) = proto_val {
                    ctor_obj.borrow_mut().insert_property(
                        "prototype".to_string(),
                        PropertyDescriptor::data(val, false, false, false),
                    );
                }
            }
        }

        // Wire up global object as backing for global environment lookups
        // Per spec §9.1.1.4, the Global Environment Record has an Object Environment
        // Record whose binding object is the global object. Variable lookups in global
        // scope should check global object properties.
        let gid = global_obj_id;
        self.realm().global_env.borrow_mut().global_object_id = Some(gid);
        self.realm_mut().global_object = Some(gid);

        // Per §9.1.1.4, built-in global names live on the global object (Object
        // Environment Record), not as declarative bindings. Remove the bootstrapping
        // bindings so that identifier resolution falls through to the global object.
        // Keep NaN/Infinity/undefined — they are non-writable/non-configurable
        // (ImmutableValue bindings that must shadow global object writes).
        {
            let mut env = self.realm().global_env.borrow_mut();
            for name in &global_names {
                match *name {
                    "NaN" | "Infinity" | "undefined" => continue,
                    _ => {
                        env.bindings.remove(*name);
                    }
                }
            }
        }

        // $262 test harness object (must be after global object is created)
        {
            let realm_id = self.current_realm_id;
            let dollar_262_val = self.create_dollar_262(realm_id);
            let global_env = self.realm().global_env.clone();
            global_env.borrow_mut().declare("$262", BindingKind::Var);
            let _ = self.env_set(&global_env, "$262", dollar_262_val);
        }
    }

    pub(crate) fn setup_shadow_realm(&mut self) {
        let my_realm_id = self.current_realm_id;
        let proto_id = self.create_object_id();
        let op = self.realm().object_prototype;
        {
            let mut p = self.get_object_cell_expect(proto_id).borrow_mut();
            p.class_name = "ShadowRealm".to_string();
            p.prototype_id = op;
        }

        // ShadowRealm.prototype[Symbol.toStringTag] = "ShadowRealm"
        self.define_to_string_tag(proto_id, "ShadowRealm");

        // ShadowRealm.prototype.evaluate
        let evaluate_fn = self.create_function(JsFunction::native(
            "evaluate".to_string(),
            1,
            move |interp, this, args| {
                let eval_realm_id = if let Some(shadow_realm_id) = this.as_object_id()
                    && let Some(obj) = interp.get_object_cell(shadow_realm_id)
                    && let Some(realm_id) = obj.borrow().shadow_realm_id()
                {
                    realm_id
                } else {
                    return Completion::Throw(interp.create_error_in_realm(
                        my_realm_id,
                        "TypeError",
                        "ShadowRealm.prototype.evaluate called on non-ShadowRealm",
                    ));
                };

                let source_val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let source_text = if let Some(source) = source_val.as_string() {
                    source.to_string()
                } else {
                    return Completion::Throw(interp.create_error_in_realm(
                        my_realm_id,
                        "TypeError",
                        "ShadowRealm.prototype.evaluate: sourceText must be a string",
                    ));
                };

                interp.perform_realm_eval(&source_text, my_realm_id, eval_realm_id)
            },
        ));
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_builtin("evaluate".to_string(), evaluate_fn);

        // ShadowRealm.prototype.importValue
        let import_value_fn = self.create_function(JsFunction::native(
            "importValue".to_string(),
            2,
            move |interp, this, args| {
                let eval_realm_id = if let Some(shadow_realm_id) = this.as_object_id()
                    && let Some(obj) = interp.get_object_cell(shadow_realm_id)
                    && let Some(realm_id) = obj.borrow().shadow_realm_id()
                {
                    realm_id
                } else {
                    return Completion::Throw(interp.create_error_in_realm(
                        my_realm_id,
                        "TypeError",
                        "ShadowRealm.prototype.importValue called on non-ShadowRealm",
                    ));
                };

                let specifier_val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let export_name_val = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);

                let specifier = match interp.to_string_value(&specifier_val) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };

                let export_name = if let Some(export_name) = export_name_val.as_string() {
                    export_name.to_string()
                } else {
                    return Completion::Throw(interp.create_error_in_realm(
                        my_realm_id,
                        "TypeError",
                        "ShadowRealm.prototype.importValue: exportName must be a string",
                    ));
                };

                let caller_realm_id = my_realm_id;
                let referrer = interp.current_module_path.clone();

                let old_realm = interp.current_realm_id;
                interp.current_realm_id = eval_realm_id;

                let module_path = match interp.resolve_module_specifier(
                    &specifier,
                    referrer.as_ref().and_then(ModuleKey::file_path),
                ) {
                    Ok(p) => p,
                    Err(_) => {
                        interp.current_realm_id = old_realm;
                        let err = interp.create_error_in_realm(
                            caller_realm_id,
                            "TypeError",
                            "ShadowRealm importValue: cannot resolve module",
                        );
                        return interp.create_rejected_promise(err);
                    }
                };

                let module = match interp.load_module_for_type(
                    &module_path,
                    None,
                    super::ModuleLoadMode::Evaluate,
                ) {
                    Ok(m) => m,
                    Err(_) => {
                        interp.current_realm_id = old_realm;
                        let err = interp.create_error_in_realm(
                            caller_realm_id,
                            "TypeError",
                            "ShadowRealm importValue: module load error",
                        );
                        return interp.create_rejected_promise(err);
                    }
                };
                let resolved_canon = module_path.canonicalize();
                let mut stack = vec![];
                if interp
                    .inner_module_evaluation(&resolved_canon, &mut stack, 0)
                    .is_err()
                {
                    interp.current_realm_id = old_realm;
                    let err = interp.create_error_in_realm(
                        caller_realm_id,
                        "TypeError",
                        "ShadowRealm importValue: module evaluation error",
                    );
                    return interp.create_rejected_promise(err);
                }
                interp.drain_microtasks();
                interp.current_realm_id = old_realm;

                // Get the named export
                let export_val = {
                    let m = module.borrow();
                    if let Some(v) = m.exports.get(&export_name) {
                        Some(v.clone())
                    } else {
                        // Also check export_bindings -> env
                        if let Some(binding_name) = m.export_bindings.get(&export_name) {
                            m.env.borrow().get(binding_name)
                        } else {
                            None
                        }
                    }
                };

                let export_val = match export_val {
                    Some(v) => v,
                    None => {
                        let err = interp.create_error_in_realm(
                            caller_realm_id,
                            "TypeError",
                            &format!(
                                "ShadowRealm importValue: export '{}' not found",
                                export_name
                            ),
                        );
                        return interp.create_rejected_promise(err);
                    }
                };

                match interp.get_wrapped_value(caller_realm_id, &export_val) {
                    Ok(wrapped) => interp.create_resolved_promise(wrapped),
                    Err(e) => interp.create_rejected_promise(e),
                }
            },
        ));
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_builtin("importValue".to_string(), import_value_fn);

        let proto_val = JsValue::object(proto_id);

        // ShadowRealm constructor
        let proto_val_for_ctor = proto_val.clone();
        let shadow_realm_ctor = self.create_function(JsFunction::constructor(
            "ShadowRealm".to_string(),
            0,
            move |interp, _this, _args| {
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("ShadowRealm must be called with 'new'"),
                    );
                }

                let new_realm_id = interp.create_new_realm();

                let obj_id = interp.create_object_id();
                {
                    let mut o = interp.get_object_cell_expect(obj_id).borrow_mut();
                    o.class_name = "ShadowRealm".to_string();
                    o.kind = crate::interpreter::types::ObjectKind::ShadowRealm(new_realm_id);
                    if let Some(proto_id) = proto_val_for_ctor.as_object_id() {
                        o.prototype_id = Some(proto_id);
                    }
                }
                Completion::Normal(JsValue::object(obj_id))
            },
        ));

        // Set ShadowRealm.prototype on constructor
        if let Some(constructor_id) = shadow_realm_ctor.as_object_id()
            && let Some(ctor_obj) = self.get_object_cell(constructor_id)
        {
            ctor_obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(proto_val.clone(), false, false, false),
            );
        }

        // Set ShadowRealm.prototype.constructor = ShadowRealm
        if let Some(proto_id) = proto_val.as_object_id()
            && let Some(proto_obj) = self.get_object_cell(proto_id)
        {
            proto_obj
                .borrow_mut()
                .insert_builtin("constructor".to_string(), shadow_realm_ctor.clone());
        }

        // Store prototype in realm
        self.realm_mut().shadow_realm_prototype = Some(proto_id);

        // Register ShadowRealm as global
        let global_env = self.realm().global_env.clone();
        global_env
            .borrow_mut()
            .declare("ShadowRealm", BindingKind::Var);
        let _ = global_env
            .borrow_mut()
            .set("ShadowRealm", shadow_realm_ctor);
    }
}
