use super::super::*;

impl Interpreter {
    pub(super) fn setup_global_object(&mut self) {
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
}
