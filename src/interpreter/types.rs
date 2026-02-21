use crate::ast::*;
use crate::interpreter::generator_transform::{GeneratorStateMachine, SentValueBinding};
use crate::interpreter::helpers::same_value;
use crate::types::{JsString, JsValue};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug)]
pub enum Completion {
    Normal(JsValue),
    Return(JsValue),
    Throw(JsValue),
    Break(Option<String>, Option<JsValue>),
    Continue(Option<String>, Option<JsValue>),
    Yield(JsValue),
    Empty,
}

impl Completion {
    pub(crate) fn is_abrupt(&self) -> bool {
        !matches!(self, Completion::Normal(_) | Completion::Empty)
    }
    pub(crate) fn update_empty(self, val: JsValue) -> Completion {
        match self {
            Completion::Empty => Completion::Normal(val),
            Completion::Break(label, None) => Completion::Break(label, Some(val)),
            Completion::Continue(label, None) => Completion::Continue(label, Some(val)),
            other => other,
        }
    }
    pub(crate) fn value_or(&self, default: JsValue) -> JsValue {
        match self {
            Completion::Normal(v) => v.clone(),
            Completion::Empty => default,
            _ => default,
        }
    }
}

pub(crate) struct GeneratorContext {
    pub(crate) target_yield: usize,
    pub(crate) current_yield: usize,
    pub(crate) sent_value: JsValue,
    pub(crate) is_async: bool,
}

#[derive(Debug, Clone)]
pub enum GeneratorExecutionState {
    SuspendedStart,
    SuspendedYield { target_yield: usize },
    Executing,
    Completed,
}

#[derive(Debug, Clone)]
pub enum StateMachineExecutionState {
    SuspendedStart,
    SuspendedAtState { state_id: usize },
    Executing,
    Completed,
}

#[derive(Debug, Clone)]
pub struct TryContextInfo {
    pub catch_state: Option<usize>,
    pub finally_state: Option<usize>,
    pub after_state: usize,
    pub entered_catch: bool,
    pub entered_finally: bool,
}

#[derive(Debug, Clone)]
pub struct DelegatedIteratorInfo {
    pub iterator: JsValue,
    pub next_method: JsValue,
    pub resume_state: usize,
    pub sent_value_binding: Option<SentValueBinding>,
}

pub type EnvRef = Rc<RefCell<Environment>>;

pub struct Environment {
    pub(crate) bindings: HashMap<String, Binding>,
    pub(crate) parent: Option<EnvRef>,
    pub strict: bool,
    pub(crate) is_function_scope: bool,
    pub(crate) with_object: Option<WithObject>,
    pub(crate) dispose_stack: Option<Vec<DisposableResource>>,
    pub(crate) global_object: Option<Rc<RefCell<JsObjectData>>>,
    // Annex B.3.3: names registered for block-level function var hoisting
    pub(crate) annexb_function_names: Option<Vec<String>>,
    pub(crate) class_private_names: Option<std::collections::HashMap<String, String>>,
    pub(crate) is_field_initializer: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DisposeHint {
    Sync,
    Async,
}

#[derive(Clone, Debug)]
pub(crate) struct DisposableResource {
    pub(crate) value: JsValue,
    pub(crate) hint: DisposeHint,
    pub(crate) dispose_method: JsValue,
}

impl std::fmt::Debug for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Environment")
            .field("bindings", &self.bindings)
            .field("strict", &self.strict)
            .field("has_with_object", &self.with_object.is_some())
            .finish()
    }
}

pub(crate) struct WithObject {
    pub(crate) object: Rc<RefCell<JsObjectData>>,
    pub(crate) obj_id: u64,
    pub(crate) unscopables: Option<Rc<RefCell<JsObjectData>>>,
}

#[derive(Debug, Clone)]
pub(crate) struct Binding {
    pub(crate) value: JsValue,
    pub(crate) kind: BindingKind,
    pub(crate) initialized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum BindingKind {
    Var,
    Let,
    Const,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SetBindingCheck {
    Ok,
    ConstAssign,
    TdzError,
    Unresolvable,
}

impl Environment {
    pub fn new(parent: Option<EnvRef>) -> EnvRef {
        let strict = parent.as_ref().is_some_and(|p| p.borrow().strict);
        Rc::new(RefCell::new(Environment {
            bindings: HashMap::new(),
            parent,
            strict,
            is_function_scope: false,
            with_object: None,
            dispose_stack: None,
            global_object: None,
            annexb_function_names: None,
            class_private_names: None,
            is_field_initializer: false,
        }))
    }

    pub fn new_function_scope(parent: Option<EnvRef>) -> EnvRef {
        let strict = parent.as_ref().is_some_and(|p| p.borrow().strict);
        Rc::new(RefCell::new(Environment {
            bindings: HashMap::new(),
            parent,
            strict,
            is_function_scope: true,
            with_object: None,
            dispose_stack: None,
            global_object: None,
            annexb_function_names: None,
            class_private_names: None,
            is_field_initializer: false,
        }))
    }

    /// Find the nearest function scope (for var hoisting).
    /// Returns self if this is a function scope, otherwise traverses up.
    pub fn find_var_scope(env: &EnvRef) -> EnvRef {
        if env.borrow().is_function_scope || env.borrow().global_object.is_some() {
            return env.clone();
        }
        if let Some(ref parent) = env.borrow().parent {
            return Self::find_var_scope(parent);
        }
        env.clone()
    }

    pub fn declare(&mut self, name: &str, kind: BindingKind) {
        self.bindings.insert(
            name.to_string(),
            Binding {
                value: JsValue::Undefined,
                kind,
                initialized: kind == BindingKind::Var,
            },
        );
    }

    /// Like declare, but also creates a non-configurable property on the global object.
    /// Per §9.1.1.4.17 CreateGlobalVarBinding: var declarations in global scope
    /// create non-configurable properties on the global object.
    pub fn declare_global_var(&mut self, name: &str) {
        self.declare(name, BindingKind::Var);
        if let Some(ref global_obj) = self.global_object {
            let mut gb = global_obj.borrow_mut();
            if !gb.properties.contains_key(name) {
                gb.property_order.push(name.to_string());
                gb.properties.insert(
                    name.to_string(),
                    PropertyDescriptor::data(JsValue::Undefined, true, true, false),
                );
            }
        }
    }

    pub fn set(&mut self, name: &str, value: JsValue) -> Result<(), JsValue> {
        if let Some(ref with_obj) = self.with_object {
            let obj = with_obj.object.borrow();
            if obj.has_property(name) && !Self::is_unscopable(with_obj, name) {
                drop(obj);
                with_obj.object.borrow_mut().set_property_value(name, value);
                return Ok(());
            }
        }
        if let Some(binding) = self.bindings.get_mut(name) {
            if binding.kind == BindingKind::Const && binding.initialized {
                return Err(JsValue::String(JsString::from_str(
                    "Assignment to constant variable.",
                )));
            }
            if binding.kind == BindingKind::Var {
                if let Some(ref global_obj) = self.global_object {
                    global_obj
                        .borrow_mut()
                        .set_property_value(name, value.clone());
                }
            }
            binding.value = value;
            binding.initialized = true;
            Ok(())
        } else if let Some(parent) = &self.parent {
            parent.borrow_mut().set(name, value)
        } else {
            // Global implicit declaration (sloppy mode)
            if let Some(ref global_obj) = self.global_object {
                global_obj
                    .borrow_mut()
                    .set_property_value(name, value.clone());
            }
            self.bindings.insert(
                name.to_string(),
                Binding {
                    value,
                    kind: BindingKind::Var,
                    initialized: true,
                },
            );
            Ok(())
        }
    }

    fn is_unscopable(with: &WithObject, name: &str) -> bool {
        if let Some(ref unscopables) = with.unscopables {
            let u = unscopables.borrow();
            if let Some(desc) = u.properties.get(name)
                && let Some(ref val) = desc.value
            {
                return match val {
                    JsValue::Undefined | JsValue::Null => false,
                    JsValue::Boolean(b) => *b,
                    JsValue::Number(n) => *n != 0.0 && !n.is_nan(),
                    JsValue::String(s) => !s.is_empty(),
                    _ => true,
                };
            }
        }
        false
    }

    pub fn get(&self, name: &str) -> Option<JsValue> {
        if let Some(ref with) = self.with_object {
            let obj = with.object.borrow();
            if obj.has_property(name) && !Self::is_unscopable(with, name) {
                return Some(obj.get_property(name));
            }
        }
        if let Some(binding) = self.bindings.get(name) {
            if !binding.initialized {
                return None; // TDZ
            }
            Some(binding.value.clone())
        } else if let Some(parent) = &self.parent {
            parent.borrow().get(name)
        } else if let Some(ref global_obj) = self.global_object {
            let obj = global_obj.borrow();
            if obj.has_property(name) {
                Some(obj.get_property(name))
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Check if a binding exists but is uninitialized (in TDZ).
    /// Only checks the current environment, not parents.
    pub fn is_in_tdz(&self, name: &str) -> bool {
        if let Some(binding) = self.bindings.get(name) {
            !binding.initialized
        } else {
            false
        }
    }

    /// Walk the scope chain and check what error (if any) setting a binding would produce.
    pub fn check_set_binding(env: &EnvRef, name: &str) -> SetBindingCheck {
        let e = env.borrow();
        if let Some(binding) = e.bindings.get(name) {
            if !binding.initialized && binding.kind != BindingKind::Var {
                return SetBindingCheck::TdzError;
            }
            if binding.kind == BindingKind::Const && binding.initialized {
                return SetBindingCheck::ConstAssign;
            }
            return SetBindingCheck::Ok;
        }
        if let Some(ref global_obj) = e.global_object {
            if global_obj.borrow().has_property(name) {
                return SetBindingCheck::Ok;
            }
            return SetBindingCheck::Unresolvable;
        }
        if let Some(ref parent) = e.parent {
            return Self::check_set_binding(parent, name);
        }
        SetBindingCheck::Unresolvable
    }

    pub fn has(&self, name: &str) -> bool {
        if let Some(ref with) = self.with_object {
            let obj = with.object.borrow();
            if obj.has_property(name) && !Self::is_unscopable(with, name) {
                return true;
            }
        }
        if self.bindings.contains_key(name) {
            true
        } else if let Some(parent) = &self.parent {
            parent.borrow().has(name)
        } else if let Some(ref global_obj) = self.global_object {
            global_obj.borrow().has_property(name)
        } else {
            false
        }
    }
}

pub enum JsFunction {
    User {
        name: Option<String>,
        params: Vec<Pattern>,
        body: Vec<Statement>,
        closure: EnvRef,
        is_arrow: bool,
        is_strict: bool,
        is_generator: bool,
        is_async: bool,
        is_method: bool,
        source_text: Option<String>,
    },
    Native(
        String,
        usize,
        Rc<dyn Fn(&mut super::Interpreter, &JsValue, &[JsValue]) -> Completion>,
        bool, // is_constructor
    ),
}

impl JsFunction {
    pub fn native(
        name: String,
        arity: usize,
        f: impl Fn(&mut super::Interpreter, &JsValue, &[JsValue]) -> Completion + 'static,
    ) -> Self {
        JsFunction::Native(name, arity, Rc::new(f), false)
    }

    pub fn constructor(
        name: String,
        arity: usize,
        f: impl Fn(&mut super::Interpreter, &JsValue, &[JsValue]) -> Completion + 'static,
    ) -> Self {
        JsFunction::Native(name, arity, Rc::new(f), true)
    }
}

impl Clone for JsFunction {
    fn clone(&self) -> Self {
        match self {
            JsFunction::User {
                name,
                params,
                body,
                closure,
                is_arrow,
                is_strict,
                is_generator,
                is_async,
                is_method,
                source_text,
            } => JsFunction::User {
                name: name.clone(),
                params: params.clone(),
                body: body.clone(),
                closure: closure.clone(),
                is_arrow: *is_arrow,
                is_strict: *is_strict,
                is_generator: *is_generator,
                is_async: *is_async,
                is_method: *is_method,
                source_text: source_text.clone(),
            },
            JsFunction::Native(name, arity, f, is_ctor) => {
                JsFunction::Native(name.clone(), *arity, f.clone(), *is_ctor)
            }
        }
    }
}

impl std::fmt::Debug for JsFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsFunction::User { name, .. } => write!(f, "JsFunction::User({name:?})"),
            JsFunction::Native(name, arity, _, is_ctor) => {
                write!(f, "JsFunction::Native({name:?}, {arity}, ctor={is_ctor})")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct PropertyDescriptor {
    pub value: Option<JsValue>,
    pub writable: Option<bool>,
    pub get: Option<JsValue>,
    pub set: Option<JsValue>,
    pub enumerable: Option<bool>,
    pub configurable: Option<bool>,
}

impl PropertyDescriptor {
    pub fn data(value: JsValue, writable: bool, enumerable: bool, configurable: bool) -> Self {
        Self {
            value: Some(value),
            writable: Some(writable),
            get: None,
            set: None,
            enumerable: Some(enumerable),
            configurable: Some(configurable),
        }
    }

    pub fn data_default(value: JsValue) -> Self {
        Self::data(value, true, true, true)
    }

    pub fn accessor(
        get: Option<JsValue>,
        set: Option<JsValue>,
        enumerable: bool,
        configurable: bool,
    ) -> Self {
        Self {
            value: None,
            writable: None,
            get,
            set,
            enumerable: Some(enumerable),
            configurable: Some(configurable),
        }
    }

    pub fn is_data_descriptor(&self) -> bool {
        self.value.is_some() || self.writable.is_some()
    }

    pub fn is_accessor_descriptor(&self) -> bool {
        self.get.is_some() || self.set.is_some()
    }
}

#[derive(Debug, Clone)]
pub enum PrivateFieldDef {
    Field {
        name: String,
        initializer: Option<Expression>,
    },
    Method {
        name: String,
        value: JsValue,
    },
    Accessor {
        name: String,
        get: Option<JsValue>,
        set: Option<JsValue>,
    },
}

#[derive(Debug, Clone)]
pub enum PrivateElement {
    Field(JsValue),
    Method(JsValue),
    Accessor {
        get: Option<JsValue>,
        set: Option<JsValue>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IteratorKind {
    Key,
    Value,
    KeyValue,
}

#[derive(Debug, Clone)]
pub enum IteratorState {
    ArrayIterator {
        array_id: u64,
        index: usize,
        kind: IteratorKind,
        done: bool,
    },
    StringIterator {
        string: JsString,
        position: usize,
        done: bool,
    },
    MapIterator {
        map_id: u64,
        index: usize,
        kind: IteratorKind,
        done: bool,
    },
    SetIterator {
        set_id: u64,
        index: usize,
        kind: IteratorKind,
        done: bool,
    },
    Generator {
        body: Vec<Statement>,
        func_env: EnvRef,
        is_strict: bool,
        execution_state: GeneratorExecutionState,
    },
    StateMachineGenerator {
        state_machine: Rc<GeneratorStateMachine>,
        func_env: EnvRef,
        is_strict: bool,
        execution_state: StateMachineExecutionState,
        sent_value: JsValue,
        try_stack: Vec<TryContextInfo>,
        pending_binding: Option<SentValueBinding>,
        delegated_iterator: Option<DelegatedIteratorInfo>,
        pending_exception: Option<JsValue>,
    },
    AsyncGenerator {
        body: Vec<Statement>,
        func_env: EnvRef,
        is_strict: bool,
        execution_state: GeneratorExecutionState,
    },
    StateMachineAsyncGenerator {
        state_machine: Rc<GeneratorStateMachine>,
        func_env: EnvRef,
        is_strict: bool,
        execution_state: StateMachineExecutionState,
        sent_value: JsValue,
        try_stack: Vec<TryContextInfo>,
        pending_binding: Option<SentValueBinding>,
        delegated_iterator: Option<DelegatedIteratorInfo>,
        pending_exception: Option<JsValue>,
    },
    RegExpStringIterator {
        source: String,
        flags: String,
        string: String,
        global: bool,
        last_index: usize,
        done: bool,
    },
    TypedArrayIterator {
        typed_array_id: u64,
        index: usize,
        kind: IteratorKind,
        done: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TypedArrayKind {
    Int8,
    Uint8,
    Uint8Clamped,
    Int16,
    Uint16,
    Int32,
    Uint32,
    Float32,
    Float64,
    BigInt64,
    BigUint64,
}

impl TypedArrayKind {
    pub fn bytes_per_element(&self) -> usize {
        match self {
            TypedArrayKind::Int8 | TypedArrayKind::Uint8 | TypedArrayKind::Uint8Clamped => 1,
            TypedArrayKind::Int16 | TypedArrayKind::Uint16 => 2,
            TypedArrayKind::Int32 | TypedArrayKind::Uint32 | TypedArrayKind::Float32 => 4,
            TypedArrayKind::Float64 | TypedArrayKind::BigInt64 | TypedArrayKind::BigUint64 => 8,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            TypedArrayKind::Int8 => "Int8Array",
            TypedArrayKind::Uint8 => "Uint8Array",
            TypedArrayKind::Uint8Clamped => "Uint8ClampedArray",
            TypedArrayKind::Int16 => "Int16Array",
            TypedArrayKind::Uint16 => "Uint16Array",
            TypedArrayKind::Int32 => "Int32Array",
            TypedArrayKind::Uint32 => "Uint32Array",
            TypedArrayKind::Float32 => "Float32Array",
            TypedArrayKind::Float64 => "Float64Array",
            TypedArrayKind::BigInt64 => "BigInt64Array",
            TypedArrayKind::BigUint64 => "BigUint64Array",
        }
    }

    pub fn is_bigint(&self) -> bool {
        matches!(self, TypedArrayKind::BigInt64 | TypedArrayKind::BigUint64)
    }
}

#[derive(Debug, Clone)]
pub struct TypedArrayInfo {
    pub kind: TypedArrayKind,
    pub buffer: Rc<RefCell<Vec<u8>>>,
    pub byte_offset: usize,
    pub byte_length: usize,
    pub array_length: usize,
    pub is_detached: Rc<Cell<bool>>,
    pub is_length_tracking: bool,
}

#[derive(Debug, Clone)]
pub struct DataViewInfo {
    pub buffer: Rc<RefCell<Vec<u8>>>,
    pub byte_offset: usize,
    pub byte_length: usize,
    pub is_detached: Rc<Cell<bool>>,
    pub is_length_tracking: bool,
}

#[derive(Clone)]
pub(crate) enum IntlData {
    Locale {
        tag: String,
        language: String,
        script: Option<String>,
        region: Option<String>,
        variants: Option<String>,
        calendar: Option<String>,
        case_first: Option<String>,
        collation: Option<String>,
        hour_cycle: Option<String>,
        numbering_system: Option<String>,
        numeric: Option<bool>,
        first_day_of_week: Option<String>,
    },
    Collator {
        locale: String,
        usage: String,
        sensitivity: String,
        ignore_punctuation: bool,
        collation: String,
        numeric: bool,
        case_first: String,
    },
    NumberFormat {
        locale: String,
        numbering_system: String,
        style: String,
        currency: Option<String>,
        currency_display: Option<String>,
        currency_sign: Option<String>,
        unit: Option<String>,
        unit_display: Option<String>,
        notation: String,
        compact_display: Option<String>,
        sign_display: String,
        use_grouping: String,
        minimum_integer_digits: u32,
        minimum_fraction_digits: u32,
        maximum_fraction_digits: u32,
        minimum_significant_digits: Option<u32>,
        maximum_significant_digits: Option<u32>,
        rounding_mode: String,
        rounding_increment: u32,
        rounding_priority: String,
        trailing_zero_display: String,
    },
    PluralRules {
        locale: String,
        plural_type: String,
        notation: String,
        minimum_integer_digits: u32,
        minimum_fraction_digits: u32,
        maximum_fraction_digits: u32,
        minimum_significant_digits: Option<u32>,
        maximum_significant_digits: Option<u32>,
        rounding_mode: String,
        rounding_increment: u32,
        rounding_priority: String,
        trailing_zero_display: String,
    },
}

#[derive(Clone, Debug)]
pub(crate) enum TemporalData {
    Duration {
        years: f64,
        months: f64,
        weeks: f64,
        days: f64,
        hours: f64,
        minutes: f64,
        seconds: f64,
        milliseconds: f64,
        microseconds: f64,
        nanoseconds: f64,
    },
    Instant {
        epoch_nanoseconds: num_bigint::BigInt,
    },
    PlainDate {
        iso_year: i32,
        iso_month: u8,
        iso_day: u8,
        calendar: String,
    },
    PlainTime {
        hour: u8,
        minute: u8,
        second: u8,
        millisecond: u16,
        microsecond: u16,
        nanosecond: u16,
    },
    PlainDateTime {
        iso_year: i32,
        iso_month: u8,
        iso_day: u8,
        hour: u8,
        minute: u8,
        second: u8,
        millisecond: u16,
        microsecond: u16,
        nanosecond: u16,
        calendar: String,
    },
    ZonedDateTime {
        epoch_nanoseconds: num_bigint::BigInt,
        time_zone: String,
        calendar: String,
    },
    PlainYearMonth {
        iso_year: i32,
        iso_month: u8,
        reference_iso_day: u8,
        calendar: String,
    },
    PlainMonthDay {
        iso_month: u8,
        iso_day: u8,
        reference_iso_year: i32,
        calendar: String,
    },
}

pub struct JsObjectData {
    pub id: Option<u64>,
    pub properties: HashMap<String, PropertyDescriptor>,
    pub property_order: Vec<String>,
    pub prototype: Option<Rc<RefCell<JsObjectData>>>,
    pub callable: Option<JsFunction>,
    pub array_elements: Option<Vec<JsValue>>,
    pub class_name: String,
    pub extensible: bool,
    pub primitive_value: Option<JsValue>,
    pub private_fields: HashMap<String, PrivateElement>,
    pub class_private_field_defs: Vec<PrivateFieldDef>,
    pub class_public_field_defs: Vec<(String, Option<crate::ast::Expression>)>,
    pub iterator_state: Option<IteratorState>,
    pub parameter_map: Option<HashMap<String, (EnvRef, String)>>,
    pub map_data: Option<Vec<Option<(JsValue, JsValue)>>>,
    pub set_data: Option<Vec<Option<JsValue>>>,
    pub proxy_target: Option<Rc<RefCell<JsObjectData>>>,
    pub proxy_handler: Option<Rc<RefCell<JsObjectData>>>,
    pub proxy_revoked: bool,
    pub arraybuffer_data: Option<Rc<RefCell<Vec<u8>>>>,
    pub arraybuffer_detached: Option<Rc<Cell<bool>>>,
    pub arraybuffer_max_byte_length: Option<usize>,
    pub typed_array_info: Option<TypedArrayInfo>,
    pub data_view_info: Option<DataViewInfo>,
    pub promise_data: Option<PromiseData>,
    pub is_raw_json: bool,
    pub is_class_constructor: bool,
    pub is_derived_class_constructor: bool,
    pub bound_target_function: Option<JsValue>,
    pub bound_args: Option<Vec<JsValue>>,
    pub(crate) disposable_stack: Option<DisposableStackData>,
    pub(crate) module_namespace: Option<ModuleNamespaceData>,
    pub(crate) temporal_data: Option<TemporalData>,
    pub(crate) intl_data: Option<IntlData>,
}

#[derive(Clone)]
pub(crate) struct ModuleNamespaceData {
    pub env: EnvRef,
    pub export_names: Vec<String>,
    pub export_to_binding: HashMap<String, String>,
    pub module_path: Option<std::path::PathBuf>,
}

#[derive(Clone, Debug)]
pub(crate) struct DisposableStackData {
    pub(crate) stack: Vec<DisposableResource>,
    pub(crate) disposed: bool,
}

impl JsObjectData {
    pub(crate) fn new() -> Self {
        Self {
            id: None,
            properties: HashMap::new(),
            property_order: Vec::new(),
            prototype: None,
            callable: None,
            array_elements: None,
            class_name: "Object".to_string(),
            extensible: true,
            primitive_value: None,
            private_fields: HashMap::new(),
            class_private_field_defs: Vec::new(),
            class_public_field_defs: Vec::new(),
            iterator_state: None,
            parameter_map: None,
            map_data: None,
            set_data: None,
            proxy_target: None,
            proxy_handler: None,
            proxy_revoked: false,
            arraybuffer_data: None,
            arraybuffer_detached: None,
            arraybuffer_max_byte_length: None,
            typed_array_info: None,
            data_view_info: None,
            promise_data: None,
            is_raw_json: false,
            is_class_constructor: false,
            is_derived_class_constructor: false,
            bound_target_function: None,
            bound_args: None,
            disposable_stack: None,
            module_namespace: None,
            temporal_data: None,
            intl_data: None,
        }
    }

    pub fn is_proxy(&self) -> bool {
        self.proxy_target.is_some()
    }

    fn string_exotic_value(&self, key: &str) -> Option<JsValue> {
        if let Some(JsValue::String(ref s)) = self.primitive_value {
            if self.class_name == "String" {
                let units = &s.code_units;
                if key == "length" {
                    return Some(JsValue::Number(units.len() as f64));
                }
                if let Ok(idx) = key.parse::<usize>() {
                    if idx < units.len() {
                        return Some(JsValue::String(crate::types::JsString {
                            code_units: vec![units[idx]],
                        }));
                    }
                }
            }
        }
        None
    }

    pub fn get_property(&self, key: &str) -> JsValue {
        // Module namespace: look up live binding from environment
        if let Some(ref ns_data) = self.module_namespace {
            if let Some(binding_name) = ns_data.export_to_binding.get(key) {
                return ns_data
                    .env
                    .borrow()
                    .get(binding_name)
                    .unwrap_or(JsValue::Undefined);
            }
        }
        if let Some(ref map) = self.parameter_map
            && let Some((env_ref, param_name)) = map.get(key)
            && let Some(val) = env_ref.borrow().get(param_name)
        {
            return val;
        }
        if let Some(desc) = self.properties.get(key) {
            if let Some(ref val) = desc.value {
                return val.clone();
            }
            return JsValue::Undefined;
        }
        if let Some(ref elems) = self.array_elements
            && let Ok(idx) = key.parse::<usize>()
            && idx < elems.len()
        {
            return elems[idx].clone();
        }
        if let Some(ref ta) = self.typed_array_info {
            if let Some(index) = canonical_numeric_index_string(key) {
                if is_valid_integer_index(ta, index) {
                    return typed_array_get_index(ta, index as usize);
                }
                return JsValue::Undefined;
            }
        }
        if let Some(val) = self.string_exotic_value(key) {
            return val;
        }
        if let Some(proto) = &self.prototype {
            return proto.borrow().get_property(key);
        }
        JsValue::Undefined
    }

    pub fn get_property_descriptor(&self, key: &str) -> Option<PropertyDescriptor> {
        if let Some(desc) = self.properties.get(key) {
            let mut d = desc.clone();
            if let Some(ref map) = self.parameter_map
                && let Some((env_ref, param_name)) = map.get(key)
                && let Some(val) = env_ref.borrow().get(param_name)
            {
                d.value = Some(val);
            }
            return Some(d);
        }
        if let Some(ref elems) = self.array_elements
            && let Ok(idx) = key.parse::<usize>()
            && idx < elems.len()
        {
            return Some(PropertyDescriptor {
                value: Some(elems[idx].clone()),
                writable: Some(true),
                enumerable: Some(true),
                configurable: Some(true),
                get: None,
                set: None,
            });
        }
        if let Some(ref ta) = self.typed_array_info {
            if let Some(index) = canonical_numeric_index_string(key) {
                if is_valid_integer_index(ta, index) {
                    return Some(PropertyDescriptor {
                        value: Some(typed_array_get_index(ta, index as usize)),
                        writable: Some(true),
                        enumerable: Some(true),
                        configurable: Some(false),
                        get: None,
                        set: None,
                    });
                }
                return None;
            }
        }
        if let Some(JsValue::String(ref s)) = self.primitive_value {
            if self.class_name == "String" {
                let units = &s.code_units;
                if key == "length" {
                    return Some(PropertyDescriptor {
                        value: Some(JsValue::Number(units.len() as f64)),
                        writable: Some(false),
                        enumerable: Some(false),
                        configurable: Some(false),
                        get: None,
                        set: None,
                    });
                }
                if let Ok(idx) = key.parse::<usize>() {
                    if idx < units.len() {
                        return Some(PropertyDescriptor {
                            value: Some(JsValue::String(crate::types::JsString {
                                code_units: vec![units[idx]],
                            })),
                            writable: Some(false),
                            enumerable: Some(true),
                            configurable: Some(false),
                            get: None,
                            set: None,
                        });
                    }
                }
            }
        }
        if let Some(proto) = &self.prototype {
            return proto.borrow().get_property_descriptor(key);
        }
        None
    }

    // Like get_property_descriptor but without prototype chain walk.
    // Includes parameter_map and array_elements handling.
    pub fn get_own_property_full(&self, key: &str) -> Option<PropertyDescriptor> {
        if let Some(desc) = self.properties.get(key) {
            let mut d = desc.clone();
            if let Some(ref map) = self.parameter_map
                && let Some((env_ref, param_name)) = map.get(key)
                && let Some(val) = env_ref.borrow().get(param_name)
            {
                d.value = Some(val);
            }
            return Some(d);
        }
        if let Some(ref elems) = self.array_elements
            && let Ok(idx) = key.parse::<usize>()
            && idx < elems.len()
        {
            return Some(PropertyDescriptor {
                value: Some(elems[idx].clone()),
                writable: Some(true),
                enumerable: Some(true),
                configurable: Some(true),
                get: None,
                set: None,
            });
        }
        if let Some(ref ta) = self.typed_array_info {
            if let Some(index) = canonical_numeric_index_string(key) {
                if is_valid_integer_index(ta, index) {
                    return Some(PropertyDescriptor {
                        value: Some(typed_array_get_index(ta, index as usize)),
                        writable: Some(true),
                        enumerable: Some(true),
                        configurable: Some(true),
                        get: None,
                        set: None,
                    });
                }
                return None;
            }
        }
        if let Some(JsValue::String(ref s)) = self.primitive_value {
            if self.class_name == "String" {
                let units = &s.code_units;
                if key == "length" {
                    return Some(PropertyDescriptor {
                        value: Some(JsValue::Number(units.len() as f64)),
                        writable: Some(false),
                        enumerable: Some(false),
                        configurable: Some(false),
                        get: None,
                        set: None,
                    });
                }
                if let Ok(idx) = key.parse::<usize>() {
                    if idx < units.len() {
                        return Some(PropertyDescriptor {
                            value: Some(JsValue::String(crate::types::JsString {
                                code_units: vec![units[idx]],
                            })),
                            writable: Some(false),
                            enumerable: Some(true),
                            configurable: Some(false),
                            get: None,
                            set: None,
                        });
                    }
                }
            }
        }
        None
    }

    pub fn get_own_property(&self, key: &str) -> Option<PropertyDescriptor> {
        // Module namespace exotic: §10.4.6.4 [[GetOwnProperty]]
        if let Some(ref ns_data) = self.module_namespace {
            if !key.starts_with("Symbol(") {
                if ns_data.export_names.contains(&key.to_string()) {
                    let val = if let Some(binding_name) = ns_data.export_to_binding.get(key) {
                        ns_data.env.borrow().get(binding_name).unwrap_or(JsValue::Undefined)
                    } else {
                        JsValue::Undefined
                    };
                    return Some(PropertyDescriptor {
                        value: Some(val),
                        writable: Some(true),
                        enumerable: Some(true),
                        configurable: Some(false),
                        get: None,
                        set: None,
                    });
                }
                // Not an export and not a symbol - check properties (@@toStringTag etc.)
                return self.properties.get(key).cloned();
            }
            // Symbol keys: fall through to ordinary
        }
        if let Some(desc) = self.properties.get(key) {
            return Some(desc.clone());
        }
        // TypedArray: §10.4.5.1
        if let Some(ref ta) = self.typed_array_info {
            if let Some(index) = canonical_numeric_index_string(key) {
                if is_valid_integer_index(ta, index) {
                    return Some(PropertyDescriptor {
                        value: Some(typed_array_get_index(ta, index as usize)),
                        writable: Some(true),
                        enumerable: Some(true),
                        configurable: Some(true),
                        get: None,
                        set: None,
                    });
                }
                return None;
            }
        }
        // String exotic: §10.4.3.1
        if let Some(JsValue::String(ref s)) = self.primitive_value {
            if self.class_name == "String" {
                if key == "length" {
                    return Some(PropertyDescriptor::data(
                        JsValue::Number(s.code_units.len() as f64),
                        false,
                        false,
                        false,
                    ));
                }
                if let Ok(idx) = key.parse::<usize>() {
                    if idx < s.code_units.len() {
                        let ch = std::char::from_u32(s.code_units[idx] as u32)
                            .map(|c| c.to_string())
                            .unwrap_or_else(|| String::from_utf16_lossy(&[s.code_units[idx]]));
                        return Some(PropertyDescriptor::data(
                            JsValue::String(JsString::from_str(&ch)),
                            false,
                            true,
                            false,
                        ));
                    }
                }
            }
        }
        None
    }

    pub fn has_own_property(&self, key: &str) -> bool {
        // Module namespace exotic: [[HasProperty]] checks export list
        if let Some(ref ns_data) = self.module_namespace {
            if !key.starts_with("Symbol(") {
                return ns_data.export_names.contains(&key.to_string());
            }
        }
        if self.properties.contains_key(key) {
            return true;
        }
        // TypedArray: §10.4.5.2
        if let Some(ref ta) = self.typed_array_info {
            if let Some(index) = canonical_numeric_index_string(key) {
                return is_valid_integer_index(ta, index);
            }
        }
        if let Some(JsValue::String(ref s)) = self.primitive_value {
            if self.class_name == "String" {
                if key == "length" {
                    return true;
                }
                if let Ok(idx) = key.parse::<usize>() {
                    return idx < s.code_units.len();
                }
            }
        }
        false
    }

    pub fn enumerable_keys_with_proto(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        let mut keys = Vec::new();

        // Collect own keys, separating integer indices from string keys
        let mut index_keys: Vec<(u32, String)> = Vec::new();
        let mut string_keys: Vec<String> = Vec::new();

        // String exotic: indices come first (they are enumerable)
        if let Some(JsValue::String(ref s)) = self.primitive_value {
            if self.class_name == "String" {
                let utf16_len = s.code_units.len();
                for i in 0..utf16_len {
                    let k = i.to_string();
                    if seen.insert(k.clone()) {
                        index_keys.push((i as u32, k));
                    }
                }
            }
        }

        // Own properties: add ALL to seen set (even non-enumerable, to shadow proto)
        for k in &self.property_order {
            if k.starts_with("Symbol(") {
                continue;
            }
            if let Some(desc) = self.properties.get(k) {
                let is_enumerable = desc.enumerable != Some(false);
                if seen.insert(k.clone()) {
                    if is_enumerable {
                        if let Some(idx) = parse_array_index(k) {
                            index_keys.push((idx, k.clone()));
                        } else {
                            string_keys.push(k.clone());
                        }
                    }
                }
            }
        }

        // Integer indices in ascending numeric order
        index_keys.sort_by_key(|(idx, _)| *idx);
        for (_, k) in index_keys {
            keys.push(k);
        }
        // String keys in insertion order
        keys.extend(string_keys);

        // Prototype chain
        if let Some(ref proto) = self.prototype {
            for k in proto.borrow().enumerable_keys_with_proto() {
                if seen.insert(k.clone()) {
                    keys.push(k);
                }
            }
        }
        keys
    }

    pub fn has_property(&self, key: &str) -> bool {
        if self.has_own_property(key) {
            return true;
        }
        if let Some(proto) = &self.prototype {
            return proto.borrow().has_property(key);
        }
        false
    }

    pub fn define_own_property(&mut self, key: String, desc: PropertyDescriptor) -> bool {
        // Module namespace exotic: §10.4.6.5 [[DefineOwnProperty]]
        if self.module_namespace.is_some() {
            if let Some(current) = self.get_own_property(&key) {
                // If every field is absent, return true
                if desc.value.is_none()
                    && desc.writable.is_none()
                    && desc.get.is_none()
                    && desc.set.is_none()
                    && desc.enumerable.is_none()
                    && desc.configurable.is_none()
                {
                    return true;
                }
                // Check each present field matches current
                if let Some(ref v) = desc.value {
                    if let Some(ref cv) = current.value {
                        if !same_value(v, cv) {
                            return false;
                        }
                    }
                }
                if let Some(w) = desc.writable {
                    if current.writable != Some(w) {
                        return false;
                    }
                }
                if let Some(e) = desc.enumerable {
                    if current.enumerable != Some(e) {
                        return false;
                    }
                }
                if let Some(c) = desc.configurable {
                    if current.configurable != Some(c) {
                        return false;
                    }
                }
                if desc.get.is_some() || desc.set.is_some() {
                    return false;
                }
                return true;
            }
            return false;
        }
        // TypedArray: §10.4.5.3 [[DefineOwnProperty]]
        if self.typed_array_info.is_some() {
            if let Some(index) = canonical_numeric_index_string(&key) {
                let ta = self.typed_array_info.as_ref().unwrap();
                if !is_valid_integer_index(ta, index) {
                    return false;
                }
                // If accessor descriptor, reject
                if desc.get.is_some() || desc.set.is_some() {
                    return false;
                }
                if desc.configurable == Some(false) {
                    return false;
                }
                if desc.enumerable == Some(false) {
                    return false;
                }
                if desc.writable == Some(false) {
                    return false;
                }
                if let Some(ref value) = desc.value {
                    let ta_clone = ta.clone();
                    typed_array_set_index(&ta_clone, index as usize, value);
                }
                return true;
            }
        }
        // Check array_elements for existing array index properties
        let current_from_array = if !self.properties.contains_key(&key) {
            if let Some(ref elems) = self.array_elements
                && let Ok(idx) = key.parse::<usize>()
                && idx < elems.len()
            {
                Some(PropertyDescriptor {
                    value: Some(elems[idx].clone()),
                    writable: Some(true),
                    enumerable: Some(true),
                    configurable: Some(true),
                    get: None,
                    set: None,
                })
            } else {
                None
            }
        } else {
            None
        };

        if let Some(current) = self.properties.get(&key).cloned().or(current_from_array) {
            // §10.1.6.3 step 2: if every field of desc is absent, return true
            if desc.value.is_none()
                && desc.writable.is_none()
                && desc.get.is_none()
                && desc.set.is_none()
                && desc.enumerable.is_none()
                && desc.configurable.is_none()
            {
                return true;
            }

            if current.configurable == Some(false) {
                if desc.configurable == Some(true) {
                    return false;
                }
                if desc.enumerable.is_some() && desc.enumerable != current.enumerable {
                    return false;
                }

                let current_is_data = current.is_data_descriptor();
                let current_is_accessor = current.is_accessor_descriptor();
                let desc_is_data = desc.is_data_descriptor();
                let desc_is_accessor = desc.is_accessor_descriptor();

                // Cannot change between data and accessor on non-configurable
                if current_is_data && !current_is_accessor && desc_is_accessor && !desc_is_data {
                    return false;
                }
                if current_is_accessor && !current_is_data && desc_is_data && !desc_is_accessor {
                    return false;
                }

                if current_is_data && !current_is_accessor {
                    // Non-configurable data property
                    if current.writable == Some(false) {
                        if desc.writable == Some(true) {
                            return false;
                        }
                        if let Some(ref new_val) = desc.value {
                            if let Some(ref cur_val) = current.value {
                                if !same_value(new_val, cur_val) {
                                    return false;
                                }
                            } else {
                                return false;
                            }
                        }
                    }
                } else if current_is_accessor {
                    // Non-configurable accessor property
                    if let Some(ref new_get) = desc.get {
                        let cur_get = current.get.as_ref().unwrap_or(&JsValue::Undefined);
                        if !same_value(new_get, cur_get) {
                            return false;
                        }
                    }
                    if let Some(ref new_set) = desc.set {
                        let cur_set = current.set.as_ref().unwrap_or(&JsValue::Undefined);
                        if !same_value(new_set, cur_set) {
                            return false;
                        }
                    }
                }
            }

            // Precompute type info before consuming desc
            let desc_is_data = desc.is_data_descriptor();
            let desc_is_accessor = desc.is_accessor_descriptor();
            let desc_has_get = desc.get.is_some();
            let desc_has_set = desc.set.is_some();
            let desc_writable = desc.writable;

            // Handle parameter map before consuming desc
            if let Some(ref mut map) = self.parameter_map
                && map.contains_key(&key)
            {
                if let Some(ref val) = desc.value
                    && let Some((env_ref, param_name)) = map.get(&key)
                {
                    let _ = env_ref.borrow_mut().set(param_name, val.clone());
                }
                if desc_has_get || desc_has_set {
                    map.remove(&key);
                } else if desc_writable == Some(false) {
                    if let Some(ref val) = desc.value
                        && let Some((env_ref, param_name)) = map.get(&key)
                    {
                        let _ = env_ref.borrow_mut().set(param_name, val.clone());
                    }
                    map.remove(&key);
                }
            }

            let current_is_data = current.is_data_descriptor();
            let current_is_accessor = current.is_accessor_descriptor();

            // Build merged descriptor
            let merged = if desc_is_data
                && !desc_is_accessor
                && current_is_accessor
                && !current_is_data
            {
                // Changing from accessor to data
                PropertyDescriptor {
                    value: desc.value.or(Some(JsValue::Undefined)),
                    writable: desc.writable.or(Some(false)),
                    get: None,
                    set: None,
                    enumerable: desc.enumerable.or(current.enumerable),
                    configurable: desc.configurable.or(current.configurable),
                }
            } else if desc_is_accessor && !desc_is_data && current_is_data && !current_is_accessor {
                // Changing from data to accessor
                PropertyDescriptor {
                    value: None,
                    writable: None,
                    get: desc.get.or(Some(JsValue::Undefined)),
                    set: desc.set.or(Some(JsValue::Undefined)),
                    enumerable: desc.enumerable.or(current.enumerable),
                    configurable: desc.configurable.or(current.configurable),
                }
            } else {
                // Normal merge: unspecified fields retain current values
                // but don't leak accessor fields into data or vice versa
                let result_is_accessor = if desc_is_accessor {
                    true
                } else if desc_is_data {
                    false
                } else {
                    current_is_accessor
                };
                if result_is_accessor {
                    PropertyDescriptor {
                        value: None,
                        writable: None,
                        get: desc.get.or(current.get),
                        set: desc.set.or(current.set),
                        enumerable: desc.enumerable.or(current.enumerable),
                        configurable: desc.configurable.or(current.configurable),
                    }
                } else {
                    PropertyDescriptor {
                        value: desc.value.or(current.value),
                        writable: desc.writable.or(current.writable),
                        get: None,
                        set: None,
                        enumerable: desc.enumerable.or(current.enumerable),
                        configurable: desc.configurable.or(current.configurable),
                    }
                }
            };

            if !self.property_order.contains(&key) {
                self.property_order.push(key.clone());
            }
            self.properties.insert(key, merged);
        } else {
            if !self.extensible {
                return false;
            }
            // Handle parameter map for new properties
            if let Some(ref mut map) = self.parameter_map
                && map.contains_key(&key)
                && let Some(ref val) = desc.value
                && let Some((env_ref, param_name)) = map.get(&key)
            {
                let _ = env_ref.borrow_mut().set(param_name, val.clone());
            }
            self.property_order.push(key.clone());
            // For new property, fill in defaults per spec
            let is_accessor = desc.is_accessor_descriptor();
            let new_desc = PropertyDescriptor {
                value: desc.value.or(if !is_accessor {
                    Some(JsValue::Undefined)
                } else {
                    None
                }),
                writable: desc
                    .writable
                    .or(if !is_accessor { Some(false) } else { None }),
                get: desc.get,
                set: desc.set,
                enumerable: desc.enumerable.or(Some(false)),
                configurable: desc.configurable.or(Some(false)),
            };
            self.properties.insert(key, new_desc);
        }
        true
    }

    pub fn set_property_value(&mut self, key: &str, value: JsValue) -> bool {
        if let Some(ref ta) = self.typed_array_info {
            if let Some(index) = canonical_numeric_index_string(key) {
                if is_valid_integer_index(ta, index) {
                    let ta_clone = ta.clone();
                    return typed_array_set_index(&ta_clone, index as usize, &value);
                }
                return false;
            }
        }
        // ArraySetLength (spec §10.4.2.1): reducing length deletes configurable properties
        if self.class_name == "Array" && key == "length" {
            if let JsValue::Number(new_len_f) = &value {
                let new_len_u32 = *new_len_f as u32;
                if (new_len_u32 as f64) == *new_len_f {
                    let mut actual_new_len = new_len_u32;
                    let old_len = self
                        .properties
                        .get("length")
                        .and_then(|d| d.value.as_ref())
                        .and_then(|v| {
                            if let JsValue::Number(n) = v {
                                Some(*n as u32)
                            } else {
                                None
                            }
                        });
                    if let Some(old_len) = old_len {
                        if new_len_u32 < old_len {
                            let mut idx_keys: Vec<(u32, String)> = self
                                .properties
                                .keys()
                                .filter_map(|k| {
                                    k.parse::<u32>()
                                        .ok()
                                        .filter(|&idx| idx >= new_len_u32)
                                        .map(|idx| (idx, k.clone()))
                                })
                                .collect();
                            idx_keys.sort_by(|a, b| b.0.cmp(&a.0));

                            for (idx, k) in &idx_keys {
                                let is_non_configurable = self
                                    .properties
                                    .get(k.as_str())
                                    .is_some_and(|d| d.configurable == Some(false));
                                if is_non_configurable {
                                    actual_new_len = idx + 1;
                                    break;
                                } else {
                                    self.properties.remove(k.as_str());
                                    self.property_order.retain(|p| p != k);
                                }
                            }
                            if let Some(ref mut elements) = self.array_elements {
                                elements.truncate(actual_new_len as usize);
                            }
                        }
                    }
                    if let Some(desc) = self.properties.get_mut("length") {
                        desc.value = Some(JsValue::Number(actual_new_len as f64));
                        return true;
                    }
                }
            }
        }
        if let Some(ref map) = self.parameter_map
            && let Some((env_ref, param_name)) = map.get(key)
        {
            let _ = env_ref.borrow_mut().set(param_name, value.clone());
        }
        // Keep array_elements in sync with properties for numeric indices
        if let Some(ref mut elements) = self.array_elements {
            if let Ok(idx) = key.parse::<usize>() {
                // Valid array indices are 0 to 2^32-2 (spec §6.1.7)
                if idx < elements.len() {
                    elements[idx] = value.clone();
                } else if idx < 0xFFFF_FFFF && idx <= elements.len() + 1024 {
                    // Extend for small gaps and valid array indices only
                    while elements.len() < idx {
                        elements.push(JsValue::Undefined);
                    }
                    elements.push(value.clone());
                    let new_len = elements.len();
                    if let Some(len_desc) = self.properties.get_mut("length") {
                        len_desc.value = Some(JsValue::Number(new_len as f64));
                    }
                } else if idx < 0xFFFF_FFFF {
                    // Valid array index but too sparse for array_elements — update length
                    let new_len = (idx + 1) as f64;
                    if let Some(len_desc) = self.properties.get_mut("length") {
                        if let Some(JsValue::Number(cur_len)) = &len_desc.value {
                            if new_len > *cur_len {
                                len_desc.value = Some(JsValue::Number(new_len));
                            }
                        }
                    }
                }
                // idx >= 0xFFFFFFFF: not a valid array index, stored as named property
            }
        }
        if let Some(desc) = self.properties.get_mut(key) {
            if desc.writable == Some(false) {
                return false;
            }
            desc.value = Some(value);
            true
        } else {
            if !self.extensible {
                return false;
            }
            self.property_order.push(key.to_string());
            self.properties
                .insert(key.to_string(), PropertyDescriptor::data_default(value));
            true
        }
    }

    pub fn insert_value(&mut self, key: String, value: JsValue) {
        if !self.properties.contains_key(&key) {
            self.property_order.push(key.clone());
        }
        self.properties
            .insert(key, PropertyDescriptor::data_default(value));
    }

    pub fn insert_builtin(&mut self, key: String, value: JsValue) {
        if !self.properties.contains_key(&key) {
            self.property_order.push(key.clone());
        }
        self.properties
            .insert(key, PropertyDescriptor::data(value, true, false, true));
    }

    pub fn insert_property(&mut self, key: String, desc: PropertyDescriptor) {
        if !self.properties.contains_key(&key) {
            self.property_order.push(key.clone());
        }
        self.properties.insert(key, desc);
    }

    pub fn get_property_value(&self, key: &str) -> Option<JsValue> {
        self.properties.get(key).and_then(|d| d.value.clone())
    }
}

// §7.1.4.1 CanonicalNumericIndexString
/// Check if a string is an array index (non-negative integer < 2^32-1).
fn parse_array_index(key: &str) -> Option<u32> {
    if key.is_empty() {
        return None;
    }
    // No leading zeros (except "0" itself)
    if key.len() > 1 && key.starts_with('0') {
        return None;
    }
    let n: u32 = key.parse().ok()?;
    // Must be < 2^32 - 1 (0xFFFFFFFF is not an array index)
    if n == u32::MAX {
        return None;
    }
    Some(n)
}

pub(crate) fn canonical_numeric_index_string(key: &str) -> Option<f64> {
    if key == "-0" {
        return Some(-0.0_f64);
    }
    let n: f64 = key.parse().ok()?;
    if crate::types::number_ops::to_string(n) == key {
        Some(n)
    } else {
        None
    }
}

pub(crate) fn typed_array_length(ta: &TypedArrayInfo) -> usize {
    if ta.is_detached.get() {
        return 0;
    }
    if ta.is_length_tracking {
        let buf_len = ta.buffer.borrow().len();
        let remaining = buf_len.saturating_sub(ta.byte_offset);
        remaining / ta.kind.bytes_per_element()
    } else {
        let buf_len = ta.buffer.borrow().len();
        if ta.byte_offset + ta.byte_length > buf_len {
            0
        } else {
            ta.array_length
        }
    }
}

pub(crate) fn is_typed_array_out_of_bounds(ta: &TypedArrayInfo) -> bool {
    if ta.is_detached.get() {
        return true;
    }
    let buf_len = ta.buffer.borrow().len();
    if ta.is_length_tracking {
        ta.byte_offset > buf_len
    } else {
        ta.byte_offset + ta.byte_length > buf_len
    }
}

pub(crate) fn typed_array_byte_length(ta: &TypedArrayInfo) -> usize {
    typed_array_length(ta) * ta.kind.bytes_per_element()
}

// §10.4.5.14 IsValidIntegerIndex
pub(crate) fn is_valid_integer_index(ta: &TypedArrayInfo, index: f64) -> bool {
    if ta.is_detached.get() {
        return false;
    }
    if is_typed_array_out_of_bounds(ta) {
        return false;
    }
    if index.is_nan() || index.is_infinite() {
        return false;
    }
    if index != index.trunc() {
        return false;
    }
    if index.is_sign_negative() && index == 0.0 {
        return false; // -0
    }
    let len = typed_array_length(ta) as f64;
    if index < 0.0 || index >= len {
        return false;
    }
    true
}

pub(crate) fn typed_array_get_index(ta: &TypedArrayInfo, idx: usize) -> JsValue {
    let buf = ta.buffer.borrow();
    let offset = ta.byte_offset + idx * ta.kind.bytes_per_element();
    if offset + ta.kind.bytes_per_element() > buf.len() {
        return JsValue::Undefined;
    }
    match ta.kind {
        TypedArrayKind::Int8 => JsValue::Number(buf[offset] as i8 as f64),
        TypedArrayKind::Uint8 | TypedArrayKind::Uint8Clamped => JsValue::Number(buf[offset] as f64),
        TypedArrayKind::Int16 => {
            let v = i16::from_ne_bytes([buf[offset], buf[offset + 1]]);
            JsValue::Number(v as f64)
        }
        TypedArrayKind::Uint16 => {
            let v = u16::from_ne_bytes([buf[offset], buf[offset + 1]]);
            JsValue::Number(v as f64)
        }
        TypedArrayKind::Int32 => {
            let v = i32::from_ne_bytes([
                buf[offset],
                buf[offset + 1],
                buf[offset + 2],
                buf[offset + 3],
            ]);
            JsValue::Number(v as f64)
        }
        TypedArrayKind::Uint32 => {
            let v = u32::from_ne_bytes([
                buf[offset],
                buf[offset + 1],
                buf[offset + 2],
                buf[offset + 3],
            ]);
            JsValue::Number(v as f64)
        }
        TypedArrayKind::Float32 => {
            let v = f32::from_ne_bytes([
                buf[offset],
                buf[offset + 1],
                buf[offset + 2],
                buf[offset + 3],
            ]);
            JsValue::Number(v as f64)
        }
        TypedArrayKind::Float64 => {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&buf[offset..offset + 8]);
            JsValue::Number(f64::from_ne_bytes(bytes))
        }
        TypedArrayKind::BigInt64 => {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&buf[offset..offset + 8]);
            JsValue::BigInt(crate::types::JsBigInt {
                value: num_bigint::BigInt::from(i64::from_ne_bytes(bytes)),
            })
        }
        TypedArrayKind::BigUint64 => {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&buf[offset..offset + 8]);
            JsValue::BigInt(crate::types::JsBigInt {
                value: num_bigint::BigInt::from(u64::from_ne_bytes(bytes)),
            })
        }
    }
}

pub(crate) fn typed_array_set_index(ta: &TypedArrayInfo, idx: usize, value: &JsValue) -> bool {
    let mut buf = ta.buffer.borrow_mut();
    let offset = ta.byte_offset + idx * ta.kind.bytes_per_element();
    if offset + ta.kind.bytes_per_element() > buf.len() {
        return false;
    }
    match ta.kind {
        TypedArrayKind::Int8 => {
            let v = to_int8(value);
            buf[offset] = v as u8;
        }
        TypedArrayKind::Uint8 => {
            let v = to_uint8(value);
            buf[offset] = v;
        }
        TypedArrayKind::Uint8Clamped => {
            let v = to_uint8_clamped(value);
            buf[offset] = v;
        }
        TypedArrayKind::Int16 => {
            let v = to_int16(value);
            buf[offset..offset + 2].copy_from_slice(&v.to_ne_bytes());
        }
        TypedArrayKind::Uint16 => {
            let v = to_uint16(value);
            buf[offset..offset + 2].copy_from_slice(&v.to_ne_bytes());
        }
        TypedArrayKind::Int32 => {
            let v = to_int32(value);
            buf[offset..offset + 4].copy_from_slice(&v.to_ne_bytes());
        }
        TypedArrayKind::Uint32 => {
            let v = to_uint32(value);
            buf[offset..offset + 4].copy_from_slice(&v.to_ne_bytes());
        }
        TypedArrayKind::Float32 => {
            let n = to_number(value);
            buf[offset..offset + 4].copy_from_slice(&(n as f32).to_ne_bytes());
        }
        TypedArrayKind::Float64 => {
            let n = to_number(value);
            buf[offset..offset + 8].copy_from_slice(&n.to_ne_bytes());
        }
        TypedArrayKind::BigInt64 => {
            let v = to_bigint64(value);
            buf[offset..offset + 8].copy_from_slice(&v.to_ne_bytes());
        }
        TypedArrayKind::BigUint64 => {
            let v = to_biguint64(value);
            buf[offset..offset + 8].copy_from_slice(&v.to_ne_bytes());
        }
    }
    true
}

fn to_number(v: &JsValue) -> f64 {
    match v {
        JsValue::Number(n) => *n,
        JsValue::Boolean(true) => 1.0,
        JsValue::Boolean(false) | JsValue::Null => 0.0,
        JsValue::Undefined => f64::NAN,
        JsValue::String(s) => s.to_string().parse::<f64>().unwrap_or(f64::NAN),
        _ => f64::NAN,
    }
}

fn to_int8(v: &JsValue) -> i8 {
    to_number(v) as i32 as i8
}
fn to_uint8(v: &JsValue) -> u8 {
    to_number(v) as i32 as u8
}
fn to_uint8_clamped(v: &JsValue) -> u8 {
    let n = to_number(v);
    if n.is_nan() {
        0
    } else if n <= 0.0 {
        0
    } else if n >= 255.0 {
        255
    } else {
        (n + 0.5).floor() as u8
    }
}
fn to_int16(v: &JsValue) -> i16 {
    to_number(v) as i32 as i16
}
fn to_uint16(v: &JsValue) -> u16 {
    to_number(v) as i32 as u16
}
fn to_int32(v: &JsValue) -> i32 {
    to_number(v) as i32
}
fn to_uint32(v: &JsValue) -> u32 {
    to_number(v) as u32
}
fn to_bigint64(v: &JsValue) -> i64 {
    match v {
        JsValue::BigInt(b) => {
            i64::try_from(&b.value).unwrap_or_else(|_| {
                // Truncate to 64 bits
                let bytes = b.value.to_signed_bytes_le();
                let mut result = [0u8; 8];
                let len = bytes.len().min(8);
                result[..len].copy_from_slice(&bytes[..len]);
                if bytes.len() < 8 && !bytes.is_empty() && (bytes[bytes.len() - 1] & 0x80) != 0 {
                    for byte in result.iter_mut().skip(len) {
                        *byte = 0xFF;
                    }
                }
                i64::from_le_bytes(result)
            })
        }
        _ => 0,
    }
}
fn to_biguint64(v: &JsValue) -> u64 {
    match v {
        JsValue::BigInt(b) => u64::try_from(&b.value).unwrap_or_else(|_| {
            let bytes = b.value.to_signed_bytes_le();
            let mut result = [0u8; 8];
            let len = bytes.len().min(8);
            result[..len].copy_from_slice(&bytes[..len]);
            u64::from_le_bytes(result)
        }),
        _ => 0,
    }
}

#[derive(Debug, Clone)]
pub enum PromiseState {
    Pending,
    Fulfilled(JsValue),
    Rejected(JsValue),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PromiseReactionType {
    Fulfill,
    Reject,
}

#[derive(Debug, Clone)]
pub struct PromiseReaction {
    pub handler: Option<JsValue>,
    pub promise_id: Option<u64>,
    pub resolve: JsValue,
    pub reject: JsValue,
    pub reaction_type: PromiseReactionType,
}

#[derive(Debug, Clone)]
pub struct PromiseData {
    pub state: PromiseState,
    pub fulfill_reactions: Vec<PromiseReaction>,
    pub reject_reactions: Vec<PromiseReaction>,
    pub is_handled: bool,
}

impl PromiseData {
    pub fn new() -> Self {
        Self {
            state: PromiseState::Pending,
            fulfill_reactions: Vec::new(),
            reject_reactions: Vec::new(),
            is_handled: false,
        }
    }
}

pub(crate) const GC_THRESHOLD: usize = 4096;

pub(crate) struct SetRecord {
    pub(crate) has: JsValue,
    pub(crate) keys: JsValue,
    pub(crate) size: f64,
}

#[derive(Debug, Clone)]
pub enum ModuleStatus {
    Unlinked,
    Linking,
    Linked,
    Evaluating,
    Evaluated,
    Error(JsValue),
}

impl PartialEq for ModuleStatus {
    fn eq(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (ModuleStatus::Unlinked, ModuleStatus::Unlinked)
                | (ModuleStatus::Linking, ModuleStatus::Linking)
                | (ModuleStatus::Linked, ModuleStatus::Linked)
                | (ModuleStatus::Evaluating, ModuleStatus::Evaluating)
                | (ModuleStatus::Evaluated, ModuleStatus::Evaluated)
        )
    }
}

#[derive(Debug, Clone)]
pub struct ImportEntry {
    pub module_request: String,
    pub import_name: ImportName,
    pub local_name: String,
}

#[derive(Debug, Clone)]
pub enum ImportName {
    Star,
    Default,
    Named(String),
}

#[derive(Debug, Clone)]
pub struct ExportEntry {
    pub export_name: Option<String>,
    pub module_request: Option<String>,
    pub import_name: Option<ExportImportName>,
    pub local_name: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ExportImportName {
    Star,
    Named(String),
}

pub struct ModuleRecord {
    pub specifier: String,
    pub status: ModuleStatus,
    pub environment: Option<EnvRef>,
    pub namespace: Option<JsValue>,
    pub import_entries: Vec<ImportEntry>,
    pub export_entries: Vec<ExportEntry>,
    pub local_export_entries: Vec<ExportEntry>,
    pub indirect_export_entries: Vec<ExportEntry>,
    pub star_export_entries: Vec<ExportEntry>,
}
