//! Function-object construction: closures from function and arrow literals,
//! and the own properties every function object starts with.

use super::*;
use crate::interpreter::eval::ClosureSpec;

impl Interpreter {
    /// Evaluates a function or arrow literal (sec-function-definitions-
    /// runtime-semantics-evaluation, sec-arrow-function-definitions-runtime-
    /// semantics-evaluation) in `env`. `inferred_name` is the name
    /// NamedEvaluation gives an anonymous function definition.
    pub(crate) fn make_closure(
        &mut self,
        spec: &ClosureSpec<'_>,
        env: &EnvRef,
        is_method: bool,
        inferred_name: Option<&JsString>,
    ) -> JsValue {
        let own_name = if spec.is_arrow { None } else { spec.name };
        let closure_env = if let Some(name) = own_name {
            let func_env = Environment::new(Some(env.clone()));
            if spec.body_is_strict {
                func_env.borrow_mut().strict = true;
            }
            func_env
                .borrow_mut()
                .declare_function_name(spec.info.frame.own_name_atom(name));
            func_env
        } else {
            env.clone()
        };
        let enclosing_strict = env.borrow().strict;
        let func = JsFunction::User {
            bytecode_cache: Default::default(),
            frame: spec.info.frame.clone(),
            name: own_name.map(str::to_string),
            params: spec.info.params.clone(),
            body: spec.body.clone(),
            closure: closure_env.clone(),
            is_arrow: spec.is_arrow,
            is_strict: spec.body_is_strict || enclosing_strict,
            is_generator: spec.is_generator,
            is_async: spec.is_async,
            is_method,
            source_text: spec.source_text.clone(),
            // Arrows never have their own `arguments`; their literal info
            // records `false`.
            uses_arguments: spec.info.uses_arguments,
            has_simple_params: spec.info.has_simple_params,
        };
        let func_val = self.create_function_with_name(func, FunctionInit::Default, inferred_name);
        if own_name.is_some() {
            closure_env
                .borrow_mut()
                .initialize_function_name(func_val.clone());
        }
        func_val
    }

    /// `create_function_with` for a function whose `name` property is already
    /// known: `name_override` is what SetFunctionName would have stored after
    /// the fact (NamedEvaluation of an anonymous function definition).
    pub(super) fn create_function_with_name(
        &mut self,
        func: JsFunction,
        init: FunctionInit,
        name_override: Option<&JsString>,
    ) -> JsValue {
        let (is_gen, is_async) = match &func {
            JsFunction::User {
                is_generator,
                is_async,
                ..
            } => (*is_generator, *is_async),
            JsFunction::Native(..) => (false, false),
        };
        let is_async_gen = is_gen && is_async;
        let (proto_id, class_name) = if is_async_gen {
            (
                self.realm().async_generator_function_prototype,
                "AsyncGeneratorFunction",
            )
        } else if is_gen {
            (
                self.realm().generator_function_prototype,
                "GeneratorFunction",
            )
        } else if is_async {
            (self.realm().async_function_prototype, "AsyncFunction")
        } else {
            (self.realm().function_prototype, "Function")
        };
        // Annex B: sloppy non-arrow, non-generator, non-async, non-method
        // functions get own caller/arguments to shadow the ThrowTypeError
        // accessor.
        let has_restricted_props = matches!(
            &func,
            JsFunction::User {
                is_strict: false,
                is_arrow: false,
                is_generator: false,
                is_async: false,
                is_method: false,
                ..
            }
        );
        let is_constructable = match &func {
            JsFunction::User {
                is_arrow,
                is_async,
                is_generator,
                is_method,
                ..
            } => !is_arrow && !is_method && !*is_generator && !*is_async,
            JsFunction::Native(_, _, _, is_ctor) => *is_ctor,
        };
        // Generators/async-generators always need .prototype (for generator prototype chain)
        // even when they are class methods (non-constructable)
        let needs_prototype =
            !matches!(init, FunctionInit::NoPrototype) && (is_constructable || is_gen);
        let (name, fn_length, frame) = match &func {
            JsFunction::User {
                name,
                params,
                frame,
                ..
            } => {
                let len = params
                    .iter()
                    .take_while(|p| !matches!(p, Pattern::Assign(..) | Pattern::Rest(_)))
                    .count();
                (name.as_deref().unwrap_or(""), len, Some(frame))
            }
            JsFunction::Native(name, arity, _, _) => (name.as_str(), *arity, None),
        };
        let name_val = if let Some(inferred) = name_override {
            inferred.clone()
        } else if name.is_empty() {
            self.common_keys.empty_name.clone()
        } else if let Some(frame) = frame {
            frame.name_string(name)
        } else {
            JsString::from_str(name)
        };
        let mut obj_data = JsObjectData::with_properties(PropertyMap::with_exact_capacity(
            2 + 2 * usize::from(has_restricted_props) + usize::from(needs_prototype),
        ));
        obj_data.set_proto(proto_id.or(self.realm().object_prototype));
        obj_data.set_class(class_name);
        obj_data.set_function_realm(self.current_realm_id);
        obj_data.callable = Some(Box::new(func));

        let prototype = needs_prototype.then(|| {
            let parent = if is_async_gen {
                self.realm().async_generator_prototype
            } else if is_gen {
                self.realm().generator_prototype
            } else {
                self.realm().object_prototype
            };
            (parent, matches!(init, FunctionInit::Default))
        });
        let prototype_id = self.push_function_props(
            &mut obj_data,
            fn_length,
            name_val,
            has_restricted_props,
            prototype,
        );
        let func_id = self.alloc_object(obj_data);
        let func_val = JsValue::object(func_id);
        // Set prototype.constructor = func (not for generators)
        if is_constructable
            && !is_gen
            && let Some(proto_id) = prototype_id
            && let Some(proto_obj) = self.get_object_cell(proto_id)
        {
            proto_obj.borrow_mut().push_exact_data(
                &self.common_keys.constructor,
                func_val.clone(),
                true,
                false,
                true,
            );
        }
        func_val
    }

    /// Fills `obj_data` (built by `with_exact_capacity`) with the own
    /// properties a function object starts with: `length`, `name`, the Annex B
    /// `caller`/`arguments` of a sloppy function, and a fresh `prototype`
    /// object with the given parent and writability. Returns that object's id.
    /// Shared by every function kind and kept out of line so the create path's
    /// hot code stays compact.
    #[inline(never)]
    fn push_function_props(
        &mut self,
        obj_data: &mut JsObjectData,
        fn_length: usize,
        name_val: JsString,
        has_restricted_props: bool,
        prototype: Option<(Option<u64>, bool)>,
    ) -> Option<u64> {
        obj_data.push_exact_data(
            &self.common_keys.length,
            JsValue::number(fn_length as f64),
            false,
            false,
            true,
        );
        obj_data.push_exact_data(
            &self.common_keys.name,
            JsValue::string(name_val),
            false,
            false,
            true,
        );
        if has_restricted_props {
            for (key, getter) in [
                (&self.common_keys.caller, &self.realm().sloppy_caller_getter),
                (
                    &self.common_keys.arguments,
                    &self.realm().sloppy_arguments_getter,
                ),
            ] {
                match getter {
                    Some(getter) => obj_data.push_exact_getter(key, getter.clone(), false, true),
                    None => obj_data.push_exact_data(key, JsValue::NULL, false, false, true),
                }
            }
        }
        let (parent, writable) = prototype?;
        let mut proto_data = JsObjectData::new();
        proto_data.set_proto(parent);
        let proto_id = self.alloc_object(proto_data);
        obj_data.push_exact_data(
            &self.common_keys.prototype,
            JsValue::object(proto_id),
            writable,
            false,
            false,
        );
        Some(proto_id)
    }

    /// A fresh closure for a hoisted function declaration over `env`.
    pub(super) fn hoisted_function_value(&mut self, f: &FunctionDecl, env: &EnvRef) -> JsValue {
        let enclosing_strict = env.borrow().strict;
        let info = f.info();
        let func = JsFunction::User {
            bytecode_cache: Default::default(),
            frame: info.frame.clone(),
            name: Some(f.name.clone()),
            params: info.params.clone(),
            body: f.body.clone(),
            closure: env.clone(),
            is_arrow: false,
            is_strict: f.body_is_strict || enclosing_strict,
            is_generator: f.is_generator,
            is_async: f.is_async,
            is_method: false,
            source_text: f.source_text.clone(),
            uses_arguments: info.uses_arguments,
            has_simple_params: info.has_simple_params,
        };
        self.create_function(func)
    }

    /// A function declaration hoisted into the function or block scope `env`
    /// (global functions are created by `create_global_function_binding`).
    pub(super) fn hoist_function_decl(&mut self, f: &FunctionDecl, env: &EnvRef) {
        if f.phase == DeclPhase::CopyOnly {
            return;
        }
        let val = self.hoisted_function_value(f, env);
        let local = {
            let e = env.borrow();
            e.global_object_id.is_none() && e.indirect_bindings.is_none()
        };
        if local {
            env.borrow_mut().bindings.insert_atom(
                &f.info().frame.own_name_atom(&f.name),
                Binding {
                    value: val,
                    kind: BindingKind::Var,
                    initialized: true,
                    deletable: false,
                },
            );
        } else {
            env.borrow_mut().declare(&f.name, BindingKind::Var);
            let _ = self.env_set(env, &f.name, val);
        }
    }
}
