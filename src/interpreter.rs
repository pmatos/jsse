use crate::ast::*;
use crate::types::{JsString, JsValue, number_ops};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug)]
pub enum Completion {
    Normal(JsValue),
    Return(JsValue),
    Throw(JsValue),
    Break(Option<String>),
    Continue(Option<String>),
}

impl Completion {
    fn is_abrupt(&self) -> bool {
        !matches!(self, Completion::Normal(_))
    }
}

type EnvRef = Rc<RefCell<Environment>>;

#[derive(Debug)]
pub struct Environment {
    bindings: HashMap<String, Binding>,
    parent: Option<EnvRef>,
}

#[derive(Debug, Clone)]
struct Binding {
    value: JsValue,
    kind: BindingKind,
    initialized: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum BindingKind {
    Var,
    Let,
    Const,
}

impl Environment {
    fn new(parent: Option<EnvRef>) -> EnvRef {
        Rc::new(RefCell::new(Environment {
            bindings: HashMap::new(),
            parent,
        }))
    }

    fn declare(&mut self, name: &str, kind: BindingKind) {
        self.bindings.insert(
            name.to_string(),
            Binding {
                value: JsValue::Undefined,
                kind,
                initialized: kind == BindingKind::Var,
            },
        );
    }

    fn set(&mut self, name: &str, value: JsValue) -> Result<(), JsValue> {
        if let Some(binding) = self.bindings.get_mut(name) {
            if binding.kind == BindingKind::Const && binding.initialized {
                return Err(JsValue::String(JsString::from_str(
                    "Assignment to constant variable.",
                )));
            }
            binding.value = value;
            binding.initialized = true;
            Ok(())
        } else if let Some(parent) = &self.parent {
            parent.borrow_mut().set(name, value)
        } else {
            // Global implicit declaration (sloppy mode)
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

    fn get(&self, name: &str) -> Option<JsValue> {
        if let Some(binding) = self.bindings.get(name) {
            if !binding.initialized {
                return None; // TDZ
            }
            Some(binding.value.clone())
        } else if let Some(parent) = &self.parent {
            parent.borrow().get(name)
        } else {
            None
        }
    }

    fn has(&self, name: &str) -> bool {
        if self.bindings.contains_key(name) {
            true
        } else if let Some(parent) = &self.parent {
            parent.borrow().has(name)
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
    },
    Native(
        String,
        Rc<dyn Fn(&mut Interpreter, &[JsValue]) -> Completion>,
    ),
}

impl Clone for JsFunction {
    fn clone(&self) -> Self {
        match self {
            JsFunction::User {
                name,
                params,
                body,
                closure,
            } => JsFunction::User {
                name: name.clone(),
                params: params.clone(),
                body: body.clone(),
                closure: closure.clone(),
            },
            JsFunction::Native(name, f) => JsFunction::Native(name.clone(), f.clone()),
        }
    }
}

impl std::fmt::Debug for JsFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsFunction::User { name, .. } => write!(f, "JsFunction::User({name:?})"),
            JsFunction::Native(name, _) => write!(f, "JsFunction::Native({name:?})"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct JsObjectData {
    pub properties: HashMap<String, JsValue>,
    pub prototype: Option<Rc<RefCell<JsObjectData>>>,
    pub callable: Option<JsFunction>,
    pub array_elements: Option<Vec<JsValue>>,
    pub class_name: String,
}

impl JsObjectData {
    fn new() -> Self {
        Self {
            properties: HashMap::new(),
            prototype: None,
            callable: None,
            array_elements: None,
            class_name: "Object".to_string(),
        }
    }

    fn get_property(&self, key: &str) -> JsValue {
        if let Some(val) = self.properties.get(key) {
            return val.clone();
        }
        if let Some(proto) = &self.prototype {
            return proto.borrow().get_property(key);
        }
        JsValue::Undefined
    }
}

pub struct Interpreter {
    global_env: EnvRef,
    objects: Vec<Rc<RefCell<JsObjectData>>>,
}

impl Interpreter {
    pub fn new() -> Self {
        let global = Environment::new(None);

        {
            let mut env = global.borrow_mut();
            for (name, value) in [
                ("undefined", JsValue::Undefined),
                ("NaN", JsValue::Number(f64::NAN)),
                ("Infinity", JsValue::Number(f64::INFINITY)),
            ] {
                env.bindings.insert(
                    name.to_string(),
                    Binding {
                        value,
                        kind: BindingKind::Const,
                        initialized: true,
                    },
                );
            }
        }

        let mut interp = Self {
            global_env: global,
            objects: Vec::new(),
        };
        interp.setup_globals();
        interp
    }

    fn register_global_fn(&mut self, name: &str, kind: BindingKind, func: JsFunction) {
        let val = self.create_function(func);
        self.global_env.borrow_mut().declare(name, kind);
        let _ = self.global_env.borrow_mut().set(name, val);
    }

    fn setup_globals(&mut self) {
        let console = self.create_object();
        {
            let log_fn = self.create_function(JsFunction::Native(
                "log".to_string(),
                Rc::new(|_interp, args| {
                    let parts: Vec<String> = args.iter().map(|v| format!("{v}")).collect();
                    println!("{}", parts.join(" "));
                    Completion::Normal(JsValue::Undefined)
                }),
            ));
            console
                .borrow_mut()
                .properties
                .insert("log".to_string(), log_fn);
        }
        let console_val = JsValue::Object(crate::types::JsObject {
            id: self.objects.len() as u64 - 1,
        });
        self.global_env
            .borrow_mut()
            .declare("console", BindingKind::Const);
        let _ = self.global_env.borrow_mut().set("console", console_val);

        self.register_global_fn(
            "Error",
            BindingKind::Var,
            JsFunction::Native(
                "Error".to_string(),
                Rc::new(|_interp, args| {
                    let msg = args.first().cloned().unwrap_or(JsValue::Undefined);
                    Completion::Normal(msg)
                }),
            ),
        );

        self.register_global_fn(
            "Test262Error",
            BindingKind::Var,
            JsFunction::Native(
                "Test262Error".to_string(),
                Rc::new(|_interp, args| {
                    let msg = args.first().cloned().unwrap_or(JsValue::Undefined);
                    Completion::Normal(msg)
                }),
            ),
        );

        // Error constructors
        for name in [
            "SyntaxError",
            "TypeError",
            "ReferenceError",
            "RangeError",
            "URIError",
            "EvalError",
        ] {
            let error_name = name.to_string();
            self.register_global_fn(
                name,
                BindingKind::Var,
                JsFunction::Native(
                    error_name.clone(),
                    Rc::new(move |interp, args| {
                        let msg = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let obj = interp.create_object();
                        {
                            let mut o = obj.borrow_mut();
                            o.class_name = error_name.clone();
                            o.properties.insert("message".to_string(), msg);
                            o.properties.insert(
                                "name".to_string(),
                                JsValue::String(JsString::from_str(&error_name)),
                            );
                        }
                        Completion::Normal(JsValue::Object(crate::types::JsObject {
                            id: interp.objects.len() as u64 - 1,
                        }))
                    }),
                ),
            );
        }

        // Object constructor (minimal)
        self.register_global_fn(
            "Object",
            BindingKind::Var,
            JsFunction::Native(
                "Object".to_string(),
                Rc::new(|interp, args| {
                    if let Some(val) = args.first() {
                        if matches!(val, JsValue::Object(_)) {
                            return Completion::Normal(val.clone());
                        }
                    }
                    let obj = interp.create_object();
                    Completion::Normal(JsValue::Object(crate::types::JsObject {
                        id: interp.objects.len() as u64 - 1,
                    }))
                }),
            ),
        );

        // String constructor/converter
        self.register_global_fn(
            "String",
            BindingKind::Var,
            JsFunction::Native(
                "String".to_string(),
                Rc::new(|_interp, args| {
                    let val = args
                        .first()
                        .cloned()
                        .unwrap_or(JsValue::String(JsString::from_str("")));
                    Completion::Normal(JsValue::String(JsString::from_str(&to_js_string(&val))))
                }),
            ),
        );

        // Number constructor/converter
        self.register_global_fn(
            "Number",
            BindingKind::Var,
            JsFunction::Native(
                "Number".to_string(),
                Rc::new(|_interp, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::Number(0.0));
                    Completion::Normal(JsValue::Number(to_number(&val)))
                }),
            ),
        );

        // Boolean constructor/converter
        self.register_global_fn(
            "Boolean",
            BindingKind::Var,
            JsFunction::Native(
                "Boolean".to_string(),
                Rc::new(|_interp, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    Completion::Normal(JsValue::Boolean(to_boolean(&val)))
                }),
            ),
        );

        // Array constructor
        self.register_global_fn(
            "Array",
            BindingKind::Var,
            JsFunction::Native(
                "Array".to_string(),
                Rc::new(|interp, args| {
                    let obj = interp.create_object();
                    {
                        let mut o = obj.borrow_mut();
                        o.class_name = "Array".to_string();
                        if args.len() == 1 {
                            if let JsValue::Number(n) = &args[0] {
                                o.array_elements = Some(vec![JsValue::Undefined; *n as usize]);
                                o.properties.insert("length".to_string(), args[0].clone());
                            } else {
                                o.array_elements = Some(vec![args[0].clone()]);
                                o.properties
                                    .insert("length".to_string(), JsValue::Number(1.0));
                            }
                        } else {
                            o.array_elements = Some(args.to_vec());
                            o.properties
                                .insert("length".to_string(), JsValue::Number(args.len() as f64));
                        }
                    }
                    Completion::Normal(JsValue::Object(crate::types::JsObject {
                        id: interp.objects.len() as u64 - 1,
                    }))
                }),
            ),
        );

        // Global functions
        self.register_global_fn(
            "parseInt",
            BindingKind::Var,
            JsFunction::Native(
                "parseInt".to_string(),
                Rc::new(|_interp, args| {
                    let s = args.first().map(|v| to_js_string(v)).unwrap_or_default();
                    let radix = args.get(1).map(|v| to_number(v) as i32).unwrap_or(10);
                    let s = s.trim();
                    let (negative, s) = if let Some(rest) = s.strip_prefix('-') {
                        (true, rest)
                    } else if let Some(rest) = s.strip_prefix('+') {
                        (false, rest)
                    } else {
                        (false, s)
                    };
                    let radix = if radix == 0 {
                        if s.starts_with("0x") || s.starts_with("0X") {
                            16
                        } else {
                            10
                        }
                    } else {
                        radix
                    };
                    let s = if radix == 16 {
                        s.strip_prefix("0x")
                            .or_else(|| s.strip_prefix("0X"))
                            .unwrap_or(s)
                    } else {
                        s
                    };
                    match i64::from_str_radix(s, radix as u32) {
                        Ok(n) => {
                            let n = if negative { -n } else { n };
                            Completion::Normal(JsValue::Number(n as f64))
                        }
                        Err(_) => Completion::Normal(JsValue::Number(f64::NAN)),
                    }
                }),
            ),
        );

        self.register_global_fn(
            "parseFloat",
            BindingKind::Var,
            JsFunction::Native(
                "parseFloat".to_string(),
                Rc::new(|_interp, args| {
                    let s = args.first().map(|v| to_js_string(v)).unwrap_or_default();
                    let s = s.trim();
                    match s.parse::<f64>() {
                        Ok(n) => Completion::Normal(JsValue::Number(n)),
                        Err(_) => Completion::Normal(JsValue::Number(f64::NAN)),
                    }
                }),
            ),
        );

        self.register_global_fn(
            "isNaN",
            BindingKind::Var,
            JsFunction::Native(
                "isNaN".to_string(),
                Rc::new(|_interp, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let n = to_number(&val);
                    Completion::Normal(JsValue::Boolean(n.is_nan()))
                }),
            ),
        );

        self.register_global_fn(
            "isFinite",
            BindingKind::Var,
            JsFunction::Native(
                "isFinite".to_string(),
                Rc::new(|_interp, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                    let n = to_number(&val);
                    Completion::Normal(JsValue::Boolean(n.is_finite()))
                }),
            ),
        );

        // Math object
        let math_obj = self.create_object();
        {
            let mut m = math_obj.borrow_mut();
            m.class_name = "Math".to_string();
            m.properties
                .insert("PI".to_string(), JsValue::Number(std::f64::consts::PI));
            m.properties
                .insert("E".to_string(), JsValue::Number(std::f64::consts::E));
            m.properties
                .insert("LN2".to_string(), JsValue::Number(std::f64::consts::LN_2));
            m.properties
                .insert("LN10".to_string(), JsValue::Number(std::f64::consts::LN_10));
            m.properties.insert(
                "LOG2E".to_string(),
                JsValue::Number(std::f64::consts::LOG2_E),
            );
            m.properties.insert(
                "LOG10E".to_string(),
                JsValue::Number(std::f64::consts::LOG10_E),
            );
            m.properties.insert(
                "SQRT2".to_string(),
                JsValue::Number(std::f64::consts::SQRT_2),
            );
            m.properties.insert(
                "SQRT1_2".to_string(),
                JsValue::Number(std::f64::consts::FRAC_1_SQRT_2),
            );
        }
        // Add Math methods
        let math_fns: Vec<(&str, fn(f64) -> f64)> = vec![
            ("abs", f64::abs),
            ("ceil", f64::ceil),
            ("floor", f64::floor),
            ("round", f64::round),
            ("sqrt", f64::sqrt),
            ("sin", f64::sin),
            ("cos", f64::cos),
            ("tan", f64::tan),
            ("log", f64::ln),
            ("exp", f64::exp),
            ("asin", f64::asin),
            ("acos", f64::acos),
            ("atan", f64::atan),
            ("trunc", f64::trunc),
            ("sign", f64::signum),
            ("cbrt", f64::cbrt),
        ];
        for (name, op) in math_fns {
            let fn_val = self.create_function(JsFunction::Native(
                name.to_string(),
                Rc::new(move |_interp, args| {
                    let x = args.first().map(|v| to_number(v)).unwrap_or(f64::NAN);
                    Completion::Normal(JsValue::Number(op(x)))
                }),
            ));
            math_obj
                .borrow_mut()
                .properties
                .insert(name.to_string(), fn_val);
        }
        // Math.max, Math.min, Math.pow, Math.random, Math.atan2
        let max_fn = self.create_function(JsFunction::Native(
            "max".to_string(),
            Rc::new(|_interp, args| {
                if args.is_empty() {
                    return Completion::Normal(JsValue::Number(f64::NEG_INFINITY));
                }
                let mut result = f64::NEG_INFINITY;
                for a in args {
                    let n = to_number(a);
                    if n.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    if n > result {
                        result = n;
                    }
                }
                Completion::Normal(JsValue::Number(result))
            }),
        ));
        math_obj
            .borrow_mut()
            .properties
            .insert("max".to_string(), max_fn);
        let min_fn = self.create_function(JsFunction::Native(
            "min".to_string(),
            Rc::new(|_interp, args| {
                if args.is_empty() {
                    return Completion::Normal(JsValue::Number(f64::INFINITY));
                }
                let mut result = f64::INFINITY;
                for a in args {
                    let n = to_number(a);
                    if n.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    if n < result {
                        result = n;
                    }
                }
                Completion::Normal(JsValue::Number(result))
            }),
        ));
        math_obj
            .borrow_mut()
            .properties
            .insert("min".to_string(), min_fn);
        let pow_fn = self.create_function(JsFunction::Native(
            "pow".to_string(),
            Rc::new(|_interp, args| {
                let base = args.first().map(|v| to_number(v)).unwrap_or(f64::NAN);
                let exp = args.get(1).map(|v| to_number(v)).unwrap_or(f64::NAN);
                Completion::Normal(JsValue::Number(base.powf(exp)))
            }),
        ));
        math_obj
            .borrow_mut()
            .properties
            .insert("pow".to_string(), pow_fn);
        let random_fn = self.create_function(JsFunction::Native(
            "random".to_string(),
            Rc::new(|_interp, _args| {
                Completion::Normal(JsValue::Number(0.5)) // deterministic for testing
            }),
        ));
        math_obj
            .borrow_mut()
            .properties
            .insert("random".to_string(), random_fn);

        let math_val = JsValue::Object(crate::types::JsObject {
            id: self.objects.len() as u64 - 1,
        });
        self.global_env
            .borrow_mut()
            .declare("Math", BindingKind::Const);
        let _ = self.global_env.borrow_mut().set("Math", math_val);

        // eval (stub that throws)
        self.register_global_fn(
            "eval",
            BindingKind::Var,
            JsFunction::Native(
                "eval".to_string(),
                Rc::new(|_interp, _args| {
                    Completion::Throw(JsValue::String(JsString::from_str("eval is not supported")))
                }),
            ),
        );

        self.register_global_fn(
            "$DONOTEVALUATE",
            BindingKind::Var,
            JsFunction::Native(
                "$DONOTEVALUATE".to_string(),
                Rc::new(|_interp, _args| {
                    Completion::Throw(JsValue::String(JsString::from_str(
                        "Test262: $DONOTEVALUATE was called",
                    )))
                }),
            ),
        );
    }

    fn create_object(&mut self) -> Rc<RefCell<JsObjectData>> {
        let obj = Rc::new(RefCell::new(JsObjectData::new()));
        self.objects.push(obj.clone());
        obj
    }

    fn create_function(&mut self, func: JsFunction) -> JsValue {
        let mut obj_data = JsObjectData::new();
        obj_data.callable = Some(func);
        obj_data.class_name = "Function".to_string();
        let obj = Rc::new(RefCell::new(obj_data));
        self.objects.push(obj);
        JsValue::Object(crate::types::JsObject {
            id: self.objects.len() as u64 - 1,
        })
    }

    fn get_object(&self, id: u64) -> Option<Rc<RefCell<JsObjectData>>> {
        self.objects.get(id as usize).cloned()
    }

    fn create_arguments_object(&mut self, args: &[JsValue]) -> JsValue {
        let obj = self.create_object();
        {
            let mut o = obj.borrow_mut();
            o.class_name = "Arguments".to_string();
            o.properties
                .insert("length".to_string(), JsValue::Number(args.len() as f64));
            for (i, val) in args.iter().enumerate() {
                o.properties.insert(i.to_string(), val.clone());
            }
        }
        JsValue::Object(crate::types::JsObject {
            id: self.objects.len() as u64 - 1,
        })
    }

    pub fn run(&mut self, program: &Program) -> Completion {
        self.exec_statements(&program.body, &self.global_env.clone())
    }

    fn exec_statements(&mut self, stmts: &[Statement], env: &EnvRef) -> Completion {
        // Hoist var and function declarations
        for stmt in stmts {
            match stmt {
                Statement::Variable(decl) if decl.kind == VarKind::Var => {
                    for d in &decl.declarations {
                        self.hoist_pattern(&d.pattern, env);
                    }
                }
                Statement::FunctionDeclaration(f) => {
                    env.borrow_mut().declare(&f.name, BindingKind::Var);
                    let func = JsFunction::User {
                        name: Some(f.name.clone()),
                        params: f.params.clone(),
                        body: f.body.clone(),
                        closure: env.clone(),
                    };
                    let val = self.create_function(func);
                    let _ = env.borrow_mut().set(&f.name, val);
                }
                _ => {}
            }
        }

        let mut result = JsValue::Undefined;
        for stmt in stmts {
            let comp = self.exec_statement(stmt, env);
            match comp {
                Completion::Normal(val) => result = val,
                other => return other,
            }
        }
        Completion::Normal(result)
    }

    fn hoist_pattern(&self, pat: &Pattern, env: &EnvRef) {
        match pat {
            Pattern::Identifier(name) => {
                if !env.borrow().bindings.contains_key(name) {
                    env.borrow_mut().declare(name, BindingKind::Var);
                }
            }
            Pattern::Array(elems) => {
                for elem in elems.iter().flatten() {
                    match elem {
                        ArrayPatternElement::Pattern(p) | ArrayPatternElement::Rest(p) => {
                            self.hoist_pattern(p, env);
                        }
                    }
                }
            }
            Pattern::Object(props) => {
                for prop in props {
                    match prop {
                        ObjectPatternProperty::KeyValue(_, p) | ObjectPatternProperty::Rest(p) => {
                            self.hoist_pattern(p, env);
                        }
                        ObjectPatternProperty::Shorthand(name) => {
                            if !env.borrow().bindings.contains_key(name) {
                                env.borrow_mut().declare(name, BindingKind::Var);
                            }
                        }
                    }
                }
            }
            Pattern::Assign(inner, _) | Pattern::Rest(inner) => {
                self.hoist_pattern(inner, env);
            }
        }
    }

    fn exec_statement(&mut self, stmt: &Statement, env: &EnvRef) -> Completion {
        match stmt {
            Statement::Empty => Completion::Normal(JsValue::Undefined),
            Statement::Expression(expr) => self.eval_expr(expr, env),
            Statement::Block(stmts) => {
                let block_env = Environment::new(Some(env.clone()));
                self.exec_statements(stmts, &block_env)
            }
            Statement::Variable(decl) => self.exec_variable_declaration(decl, env),
            Statement::If(if_stmt) => {
                let test = self.eval_expr(&if_stmt.test, env);
                let test = match test {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if to_boolean(&test) {
                    self.exec_statement(&if_stmt.consequent, env)
                } else if let Some(alt) = &if_stmt.alternate {
                    self.exec_statement(alt, env)
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            Statement::While(w) => self.exec_while(w, env),
            Statement::DoWhile(dw) => self.exec_do_while(dw, env),
            Statement::For(f) => self.exec_for(f, env),
            Statement::ForIn(fi) => self.exec_for_in(fi, env),
            Statement::ForOf(_) => Completion::Normal(JsValue::Undefined), // TODO
            Statement::Return(expr) => {
                let val = if let Some(e) = expr {
                    match self.eval_expr(e, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    }
                } else {
                    JsValue::Undefined
                };
                Completion::Return(val)
            }
            Statement::Break(label) => Completion::Break(label.clone()),
            Statement::Continue(label) => Completion::Continue(label.clone()),
            Statement::Throw(expr) => {
                let val = match self.eval_expr(expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                Completion::Throw(val)
            }
            Statement::Try(t) => self.exec_try(t, env),
            Statement::Switch(s) => self.exec_switch(s, env),
            Statement::Labeled(label, stmt) => {
                let comp = self.exec_statement(stmt, env);
                match &comp {
                    Completion::Break(Some(l)) if l == label => {
                        Completion::Normal(JsValue::Undefined)
                    }
                    Completion::Continue(Some(l)) if l == label => {
                        Completion::Normal(JsValue::Undefined)
                    }
                    _ => comp,
                }
            }
            Statement::With(_, _) => Completion::Normal(JsValue::Undefined), // TODO
            Statement::Debugger => Completion::Normal(JsValue::Undefined),
            Statement::FunctionDeclaration(_) => Completion::Normal(JsValue::Undefined), // hoisted
            Statement::ClassDeclaration(_) => Completion::Normal(JsValue::Undefined),    // TODO
        }
    }

    fn exec_variable_declaration(
        &mut self,
        decl: &VariableDeclaration,
        env: &EnvRef,
    ) -> Completion {
        let kind = match decl.kind {
            VarKind::Var => BindingKind::Var,
            VarKind::Let => BindingKind::Let,
            VarKind::Const => BindingKind::Const,
        };
        for d in &decl.declarations {
            let val = if let Some(init) = &d.init {
                match self.eval_expr(init, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                }
            } else {
                JsValue::Undefined
            };
            if let Err(e) = self.bind_pattern(&d.pattern, val, kind, env) {
                return Completion::Throw(e);
            }
        }
        Completion::Normal(JsValue::Undefined)
    }

    fn bind_pattern(
        &mut self,
        pat: &Pattern,
        val: JsValue,
        kind: BindingKind,
        env: &EnvRef,
    ) -> Result<(), JsValue> {
        match pat {
            Pattern::Identifier(name) => {
                if kind != BindingKind::Var || !env.borrow().bindings.contains_key(name) {
                    env.borrow_mut().declare(name, kind);
                }
                env.borrow_mut().set(name, val)
            }
            Pattern::Assign(inner, default) => {
                let v = if val.is_undefined() {
                    match self.eval_expr(default, env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Err(e),
                        _ => JsValue::Undefined,
                    }
                } else {
                    val
                };
                self.bind_pattern(inner, v, kind, env)
            }
            _ => {
                // TODO: array/object destructuring
                Ok(())
            }
        }
    }

    fn exec_while(&mut self, w: &WhileStatement, env: &EnvRef) -> Completion {
        loop {
            let test = match self.eval_expr(&w.test, env) {
                Completion::Normal(v) => v,
                other => return other,
            };
            if !to_boolean(&test) {
                break;
            }
            match self.exec_statement(&w.body, env) {
                Completion::Normal(_) | Completion::Continue(None) => {}
                Completion::Break(None) => break,
                other => return other,
            }
        }
        Completion::Normal(JsValue::Undefined)
    }

    fn exec_do_while(&mut self, dw: &DoWhileStatement, env: &EnvRef) -> Completion {
        loop {
            match self.exec_statement(&dw.body, env) {
                Completion::Normal(_) | Completion::Continue(None) => {}
                Completion::Break(None) => break,
                other => return other,
            }
            let test = match self.eval_expr(&dw.test, env) {
                Completion::Normal(v) => v,
                other => return other,
            };
            if !to_boolean(&test) {
                break;
            }
        }
        Completion::Normal(JsValue::Undefined)
    }

    fn exec_for(&mut self, f: &ForStatement, env: &EnvRef) -> Completion {
        let for_env = Environment::new(Some(env.clone()));
        if let Some(init) = &f.init {
            match init {
                ForInit::Variable(decl) => {
                    let comp = self.exec_variable_declaration(decl, &for_env);
                    if comp.is_abrupt() {
                        return comp;
                    }
                }
                ForInit::Expression(expr) => {
                    let comp = self.eval_expr(expr, &for_env);
                    if comp.is_abrupt() {
                        return comp;
                    }
                }
            }
        }
        loop {
            if let Some(test) = &f.test {
                let val = match self.eval_expr(test, &for_env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if !to_boolean(&val) {
                    break;
                }
            }
            match self.exec_statement(&f.body, &for_env) {
                Completion::Normal(_) | Completion::Continue(None) => {}
                Completion::Break(None) => break,
                other => return other,
            }
            if let Some(update) = &f.update {
                let comp = self.eval_expr(update, &for_env);
                if comp.is_abrupt() {
                    return comp;
                }
            }
        }
        Completion::Normal(JsValue::Undefined)
    }

    fn exec_for_in(&mut self, fi: &ForInStatement, env: &EnvRef) -> Completion {
        let obj_val = match self.eval_expr(&fi.right, env) {
            Completion::Normal(v) => v,
            other => return other,
        };
        if obj_val.is_nullish() {
            return Completion::Normal(JsValue::Undefined);
        }
        if let JsValue::Object(ref o) = obj_val {
            if let Some(obj) = self.get_object(o.id) {
                let keys: Vec<String> = obj.borrow().properties.keys().cloned().collect();
                for key in keys {
                    let key_val = JsValue::String(JsString::from_str(&key));
                    let for_env = Environment::new(Some(env.clone()));
                    match &fi.left {
                        ForInOfLeft::Variable(decl) => {
                            let kind = match decl.kind {
                                VarKind::Var => BindingKind::Var,
                                VarKind::Let => BindingKind::Let,
                                VarKind::Const => BindingKind::Const,
                            };
                            if let Some(d) = decl.declarations.first() {
                                if let Err(e) =
                                    self.bind_pattern(&d.pattern, key_val, kind, &for_env)
                                {
                                    return Completion::Throw(e);
                                }
                            }
                        }
                        ForInOfLeft::Pattern(pat) => {
                            if let Pattern::Identifier(name) = pat {
                                let _ = env.borrow_mut().set(name, key_val);
                            }
                        }
                    }
                    match self.exec_statement(&fi.body, &for_env) {
                        Completion::Normal(_) | Completion::Continue(None) => {}
                        Completion::Break(None) => break,
                        other => return other,
                    }
                }
            }
        }
        Completion::Normal(JsValue::Undefined)
    }

    fn exec_try(&mut self, t: &TryStatement, env: &EnvRef) -> Completion {
        let block_env = Environment::new(Some(env.clone()));
        let result = self.exec_statements(&t.block, &block_env);
        let result = match result {
            Completion::Throw(val) => {
                if let Some(handler) = &t.handler {
                    let catch_env = Environment::new(Some(env.clone()));
                    if let Some(param) = &handler.param {
                        if let Err(e) = self.bind_pattern(param, val, BindingKind::Let, &catch_env)
                        {
                            return Completion::Throw(e);
                        }
                    }
                    self.exec_statements(&handler.body, &catch_env)
                } else {
                    Completion::Throw(val)
                }
            }
            other => other,
        };
        if let Some(finalizer) = &t.finalizer {
            let fin_env = Environment::new(Some(env.clone()));
            let fin_result = self.exec_statements(finalizer, &fin_env);
            if fin_result.is_abrupt() {
                return fin_result;
            }
        }
        result
    }

    fn exec_switch(&mut self, s: &SwitchStatement, env: &EnvRef) -> Completion {
        let disc = match self.eval_expr(&s.discriminant, env) {
            Completion::Normal(v) => v,
            other => return other,
        };
        let switch_env = Environment::new(Some(env.clone()));
        let mut found = false;
        let mut default_idx = None;
        for (i, case) in s.cases.iter().enumerate() {
            if case.test.is_none() {
                default_idx = Some(i);
                continue;
            }
            if !found {
                let test = match self.eval_expr(case.test.as_ref().unwrap(), &switch_env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if strict_equality(&disc, &test) {
                    found = true;
                }
            }
            if found {
                for stmt in &case.consequent {
                    match self.exec_statement(stmt, &switch_env) {
                        Completion::Normal(_) => {}
                        Completion::Break(None) => return Completion::Normal(JsValue::Undefined),
                        other => return other,
                    }
                }
            }
        }
        if !found {
            if let Some(idx) = default_idx {
                for case in &s.cases[idx..] {
                    for stmt in &case.consequent {
                        match self.exec_statement(stmt, &switch_env) {
                            Completion::Normal(_) => {}
                            Completion::Break(None) => {
                                return Completion::Normal(JsValue::Undefined);
                            }
                            other => return other,
                        }
                    }
                }
            }
        }
        Completion::Normal(JsValue::Undefined)
    }

    fn eval_expr(&mut self, expr: &Expression, env: &EnvRef) -> Completion {
        match expr {
            Expression::Literal(lit) => Completion::Normal(self.eval_literal(lit)),
            Expression::Identifier(name) => {
                match env.borrow().get(name) {
                    Some(val) => Completion::Normal(val),
                    None => {
                        // Check if it's a well-known global that might not be declared
                        Completion::Throw(JsValue::String(JsString::from_str(&format!(
                            "{name} is not defined"
                        ))))
                    }
                }
            }
            Expression::This => Completion::Normal(JsValue::Undefined), // TODO
            Expression::Unary(op, operand) => {
                let val = match self.eval_expr(operand, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                Completion::Normal(self.eval_unary(*op, &val))
            }
            Expression::Binary(op, left, right) => {
                let lval = match self.eval_expr(left, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let rval = match self.eval_expr(right, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                Completion::Normal(self.eval_binary(*op, &lval, &rval))
            }
            Expression::Logical(op, left, right) => self.eval_logical(*op, left, right, env),
            Expression::Update(op, prefix, arg) => self.eval_update(*op, *prefix, arg, env),
            Expression::Assign(op, left, right) => self.eval_assign(*op, left, right, env),
            Expression::Conditional(test, cons, alt) => {
                let test_val = match self.eval_expr(test, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if to_boolean(&test_val) {
                    self.eval_expr(cons, env)
                } else {
                    self.eval_expr(alt, env)
                }
            }
            Expression::Call(callee, args) => self.eval_call(callee, args, env),
            Expression::New(callee, args) => self.eval_new(callee, args, env),
            Expression::Member(obj, prop) => self.eval_member(obj, prop, env),
            Expression::Array(elements) => self.eval_array_literal(elements, env),
            Expression::Object(props) => self.eval_object_literal(props, env),
            Expression::Function(f) => {
                let func = JsFunction::User {
                    name: f.name.clone(),
                    params: f.params.clone(),
                    body: f.body.clone(),
                    closure: env.clone(),
                };
                Completion::Normal(self.create_function(func))
            }
            Expression::ArrowFunction(af) => {
                let body_stmts = match &af.body {
                    ArrowBody::Block(stmts) => stmts.clone(),
                    ArrowBody::Expression(expr) => {
                        vec![Statement::Return(Some(*expr.clone()))]
                    }
                };
                let func = JsFunction::User {
                    name: None,
                    params: af.params.clone(),
                    body: body_stmts,
                    closure: env.clone(),
                };
                Completion::Normal(self.create_function(func))
            }
            Expression::Typeof(operand) => {
                // typeof on unresolvable reference returns "undefined"
                if let Expression::Identifier(name) = operand.as_ref() {
                    let val = env.borrow().get(name).unwrap_or(JsValue::Undefined);
                    return Completion::Normal(JsValue::String(JsString::from_str(typeof_val(
                        &val,
                        &self.objects,
                    ))));
                }
                let val = match self.eval_expr(operand, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                Completion::Normal(JsValue::String(JsString::from_str(typeof_val(
                    &val,
                    &self.objects,
                ))))
            }
            Expression::Void(operand) => {
                match self.eval_expr(operand, env) {
                    Completion::Normal(_) => {}
                    other => return other,
                }
                Completion::Normal(JsValue::Undefined)
            }
            Expression::Delete(_) => Completion::Normal(JsValue::Boolean(true)), // TODO
            Expression::Sequence(exprs) | Expression::Comma(exprs) => {
                let mut result = JsValue::Undefined;
                for e in exprs {
                    result = match self.eval_expr(e, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                }
                Completion::Normal(result)
            }
            Expression::Spread(_) => Completion::Normal(JsValue::Undefined), // handled by caller
            Expression::Template(tmpl) => {
                let mut s = String::new();
                for (i, quasi) in tmpl.quasis.iter().enumerate() {
                    s.push_str(quasi);
                    if i < tmpl.expressions.len() {
                        let val = match self.eval_expr(&tmpl.expressions[i], env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        s.push_str(&format!("{val}"));
                    }
                }
                Completion::Normal(JsValue::String(JsString::from_str(&s)))
            }
            _ => Completion::Normal(JsValue::Undefined),
        }
    }

    fn eval_literal(&self, lit: &Literal) -> JsValue {
        match lit {
            Literal::Null => JsValue::Null,
            Literal::Boolean(b) => JsValue::Boolean(*b),
            Literal::Number(n) => JsValue::Number(*n),
            Literal::String(s) => JsValue::String(JsString::from_str(s)),
            Literal::BigInt(_) => JsValue::Undefined, // TODO
            Literal::RegExp(_, _) => JsValue::Undefined, // TODO
        }
    }

    fn eval_unary(&self, op: UnaryOp, val: &JsValue) -> JsValue {
        match op {
            UnaryOp::Minus => JsValue::Number(number_ops::unary_minus(to_number(val))),
            UnaryOp::Plus => JsValue::Number(to_number(val)),
            UnaryOp::Not => JsValue::Boolean(!to_boolean(val)),
            UnaryOp::BitNot => JsValue::Number(number_ops::bitwise_not(to_number(val))),
        }
    }

    fn eval_binary(&self, op: BinaryOp, left: &JsValue, right: &JsValue) -> JsValue {
        match op {
            BinaryOp::Add => {
                // String concatenation or numeric addition
                if is_string(left) || is_string(right) {
                    let ls = to_js_string(left);
                    let rs = to_js_string(right);
                    JsValue::String(JsString::from_str(&format!("{ls}{rs}")))
                } else {
                    JsValue::Number(number_ops::add(to_number(left), to_number(right)))
                }
            }
            BinaryOp::Sub => {
                JsValue::Number(number_ops::subtract(to_number(left), to_number(right)))
            }
            BinaryOp::Mul => {
                JsValue::Number(number_ops::multiply(to_number(left), to_number(right)))
            }
            BinaryOp::Div => JsValue::Number(number_ops::divide(to_number(left), to_number(right))),
            BinaryOp::Mod => {
                JsValue::Number(number_ops::remainder(to_number(left), to_number(right)))
            }
            BinaryOp::Exp => {
                JsValue::Number(number_ops::exponentiate(to_number(left), to_number(right)))
            }
            BinaryOp::Eq => JsValue::Boolean(abstract_equality(left, right)),
            BinaryOp::NotEq => JsValue::Boolean(!abstract_equality(left, right)),
            BinaryOp::StrictEq => JsValue::Boolean(strict_equality(left, right)),
            BinaryOp::StrictNotEq => JsValue::Boolean(!strict_equality(left, right)),
            BinaryOp::Lt => JsValue::Boolean(abstract_relational(left, right) == Some(true)),
            BinaryOp::Gt => JsValue::Boolean(abstract_relational(right, left) == Some(true)),
            BinaryOp::LtEq => JsValue::Boolean(abstract_relational(right, left) == Some(false)),
            BinaryOp::GtEq => JsValue::Boolean(abstract_relational(left, right) == Some(false)),
            BinaryOp::LShift => {
                JsValue::Number(number_ops::left_shift(to_number(left), to_number(right)))
            }
            BinaryOp::RShift => JsValue::Number(number_ops::signed_right_shift(
                to_number(left),
                to_number(right),
            )),
            BinaryOp::URShift => JsValue::Number(number_ops::unsigned_right_shift(
                to_number(left),
                to_number(right),
            )),
            BinaryOp::BitAnd => {
                JsValue::Number(number_ops::bitwise_and(to_number(left), to_number(right)))
            }
            BinaryOp::BitOr => {
                JsValue::Number(number_ops::bitwise_or(to_number(left), to_number(right)))
            }
            BinaryOp::BitXor => {
                JsValue::Number(number_ops::bitwise_xor(to_number(left), to_number(right)))
            }
            BinaryOp::In => JsValue::Boolean(false), // TODO
            BinaryOp::Instanceof => JsValue::Boolean(false), // TODO
        }
    }

    fn eval_logical(
        &mut self,
        op: LogicalOp,
        left: &Expression,
        right: &Expression,
        env: &EnvRef,
    ) -> Completion {
        let lval = match self.eval_expr(left, env) {
            Completion::Normal(v) => v,
            other => return other,
        };
        match op {
            LogicalOp::And => {
                if !to_boolean(&lval) {
                    Completion::Normal(lval)
                } else {
                    self.eval_expr(right, env)
                }
            }
            LogicalOp::Or => {
                if to_boolean(&lval) {
                    Completion::Normal(lval)
                } else {
                    self.eval_expr(right, env)
                }
            }
            LogicalOp::NullishCoalescing => {
                if lval.is_nullish() {
                    self.eval_expr(right, env)
                } else {
                    Completion::Normal(lval)
                }
            }
        }
    }

    fn eval_update(
        &mut self,
        op: UpdateOp,
        prefix: bool,
        arg: &Expression,
        env: &EnvRef,
    ) -> Completion {
        if let Expression::Identifier(name) = arg {
            let old_val = match env.borrow().get(name) {
                Some(v) => to_number(&v),
                None => {
                    return Completion::Throw(JsValue::String(JsString::from_str(&format!(
                        "{name} is not defined"
                    ))));
                }
            };
            let new_val = match op {
                UpdateOp::Increment => old_val + 1.0,
                UpdateOp::Decrement => old_val - 1.0,
            };
            if let Err(e) = env.borrow_mut().set(name, JsValue::Number(new_val)) {
                return Completion::Throw(e);
            }
            Completion::Normal(JsValue::Number(if prefix { new_val } else { old_val }))
        } else {
            // TODO: member expression update
            Completion::Normal(JsValue::Number(f64::NAN))
        }
    }

    fn eval_assign(
        &mut self,
        op: AssignOp,
        left: &Expression,
        right: &Expression,
        env: &EnvRef,
    ) -> Completion {
        let rval = match self.eval_expr(right, env) {
            Completion::Normal(v) => v,
            other => return other,
        };

        match left {
            Expression::Identifier(name) => {
                let final_val = if op == AssignOp::Assign {
                    rval
                } else {
                    let lval = env.borrow().get(name).unwrap_or(JsValue::Undefined);
                    self.apply_compound_assign(op, &lval, &rval)
                };
                if !env.borrow().has(name) {
                    env.borrow_mut().declare(name, BindingKind::Var);
                }
                if let Err(e) = env.borrow_mut().set(name, final_val.clone()) {
                    return Completion::Throw(e);
                }
                Completion::Normal(final_val)
            }
            Expression::Member(obj_expr, prop) => {
                let obj_val = match self.eval_expr(obj_expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let key = match prop {
                    MemberProperty::Dot(name) => name.clone(),
                    MemberProperty::Computed(expr) => {
                        let v = match self.eval_expr(expr, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        to_js_string(&v)
                    }
                };
                if let JsValue::Object(ref o) = obj_val {
                    if let Some(obj) = self.get_object(o.id) {
                        let final_val = if op == AssignOp::Assign {
                            rval
                        } else {
                            let lval = obj.borrow().get_property(&key);
                            self.apply_compound_assign(op, &lval, &rval)
                        };
                        obj.borrow_mut().properties.insert(key, final_val.clone());
                        return Completion::Normal(final_val);
                    }
                }
                Completion::Normal(rval)
            }
            _ => Completion::Normal(rval),
        }
    }

    fn apply_compound_assign(&self, op: AssignOp, lval: &JsValue, rval: &JsValue) -> JsValue {
        match op {
            AssignOp::AddAssign => self.eval_binary(BinaryOp::Add, lval, rval),
            AssignOp::SubAssign => self.eval_binary(BinaryOp::Sub, lval, rval),
            AssignOp::MulAssign => self.eval_binary(BinaryOp::Mul, lval, rval),
            AssignOp::DivAssign => self.eval_binary(BinaryOp::Div, lval, rval),
            AssignOp::ModAssign => self.eval_binary(BinaryOp::Mod, lval, rval),
            AssignOp::ExpAssign => self.eval_binary(BinaryOp::Exp, lval, rval),
            AssignOp::LShiftAssign => self.eval_binary(BinaryOp::LShift, lval, rval),
            AssignOp::RShiftAssign => self.eval_binary(BinaryOp::RShift, lval, rval),
            AssignOp::URShiftAssign => self.eval_binary(BinaryOp::URShift, lval, rval),
            AssignOp::BitAndAssign => self.eval_binary(BinaryOp::BitAnd, lval, rval),
            AssignOp::BitOrAssign => self.eval_binary(BinaryOp::BitOr, lval, rval),
            AssignOp::BitXorAssign => self.eval_binary(BinaryOp::BitXor, lval, rval),
            _ => rval.clone(),
        }
    }

    fn eval_call(&mut self, callee: &Expression, args: &[Expression], env: &EnvRef) -> Completion {
        // Handle member calls: obj.method()
        let (func_val, this_val) = match callee {
            Expression::Member(obj_expr, prop) => {
                let obj_val = match self.eval_expr(obj_expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let key = match prop {
                    MemberProperty::Dot(name) => name.clone(),
                    MemberProperty::Computed(expr) => {
                        let v = match self.eval_expr(expr, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        to_js_string(&v)
                    }
                };
                if let JsValue::Object(ref o) = obj_val {
                    if let Some(obj) = self.get_object(o.id) {
                        let method = obj.borrow().get_property(&key);
                        (method, obj_val)
                    } else {
                        return Completion::Throw(JsValue::String(JsString::from_str(
                            "Cannot read property of undefined",
                        )));
                    }
                } else {
                    return Completion::Throw(JsValue::String(JsString::from_str(
                        "Cannot read property of non-object",
                    )));
                }
            }
            _ => {
                let val = match self.eval_expr(callee, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                (val, JsValue::Undefined)
            }
        };

        let mut evaluated_args = Vec::new();
        for arg in args {
            let val = match self.eval_expr(arg, env) {
                Completion::Normal(v) => v,
                other => return other,
            };
            evaluated_args.push(val);
        }

        self.call_function(&func_val, &this_val, &evaluated_args)
    }

    fn call_function(
        &mut self,
        func_val: &JsValue,
        _this_val: &JsValue,
        args: &[JsValue],
    ) -> Completion {
        if let JsValue::Object(o) = func_val {
            if let Some(obj) = self.get_object(o.id) {
                let callable = obj.borrow().callable.clone();
                if let Some(func) = callable {
                    return match func {
                        JsFunction::Native(_, f) => f(self, args),
                        JsFunction::User {
                            params,
                            body,
                            closure,
                            ..
                        } => {
                            let func_env = Environment::new(Some(closure));
                            // Bind parameters
                            for (i, param) in params.iter().enumerate() {
                                let val = args.get(i).cloned().unwrap_or(JsValue::Undefined);
                                let _ = self.bind_pattern(param, val, BindingKind::Var, &func_env);
                            }
                            // Create arguments object
                            let arguments_obj = self.create_arguments_object(args);
                            func_env.borrow_mut().declare("arguments", BindingKind::Var);
                            let _ = func_env.borrow_mut().set("arguments", arguments_obj);
                            let result = self.exec_statements(&body, &func_env);
                            match result {
                                Completion::Return(v) | Completion::Normal(v) => {
                                    Completion::Normal(v)
                                }
                                other => other,
                            }
                        }
                    };
                }
            }
        }
        Completion::Throw(JsValue::String(JsString::from_str("is not a function")))
    }

    fn eval_new(&mut self, callee: &Expression, args: &[Expression], env: &EnvRef) -> Completion {
        let _callee_val = match self.eval_expr(callee, env) {
            Completion::Normal(v) => v,
            other => return other,
        };
        let mut evaluated_args = Vec::new();
        for arg in args {
            let val = match self.eval_expr(arg, env) {
                Completion::Normal(v) => v,
                other => return other,
            };
            evaluated_args.push(val);
        }
        // TODO: proper new semantics (create object, call constructor, return)
        // For now, just call the function
        self.call_function(&_callee_val, &JsValue::Undefined, &evaluated_args)
    }

    fn eval_member(&mut self, obj: &Expression, prop: &MemberProperty, env: &EnvRef) -> Completion {
        let obj_val = match self.eval_expr(obj, env) {
            Completion::Normal(v) => v,
            other => return other,
        };
        let key = match prop {
            MemberProperty::Dot(name) => name.clone(),
            MemberProperty::Computed(expr) => {
                let v = match self.eval_expr(expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                to_js_string(&v)
            }
        };
        match &obj_val {
            JsValue::Object(o) => {
                if let Some(obj) = self.get_object(o.id) {
                    Completion::Normal(obj.borrow().get_property(&key))
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            JsValue::String(s) => {
                if key == "length" {
                    Completion::Normal(JsValue::Number(s.len() as f64))
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            _ => Completion::Normal(JsValue::Undefined),
        }
    }

    fn eval_array_literal(&mut self, elements: &[Option<Expression>], env: &EnvRef) -> Completion {
        let mut values = Vec::new();
        for elem in elements {
            match elem {
                Some(expr) => {
                    let val = match self.eval_expr(expr, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    values.push(val);
                }
                None => values.push(JsValue::Undefined),
            }
        }
        let mut obj_data = JsObjectData::new();
        obj_data.class_name = "Array".to_string();
        for (i, v) in values.iter().enumerate() {
            obj_data.properties.insert(i.to_string(), v.clone());
        }
        obj_data
            .properties
            .insert("length".to_string(), JsValue::Number(values.len() as f64));
        obj_data.array_elements = Some(values);
        let obj = Rc::new(RefCell::new(obj_data));
        self.objects.push(obj);
        Completion::Normal(JsValue::Object(crate::types::JsObject {
            id: self.objects.len() as u64 - 1,
        }))
    }

    fn eval_object_literal(&mut self, props: &[Property], env: &EnvRef) -> Completion {
        let mut obj_data = JsObjectData::new();
        for prop in props {
            let key = match &prop.key {
                PropertyKey::Identifier(n) => n.clone(),
                PropertyKey::String(s) => s.clone(),
                PropertyKey::Number(n) => number_ops::to_string(*n),
                PropertyKey::Computed(expr) => {
                    let v = match self.eval_expr(expr, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    to_js_string(&v)
                }
            };
            let value = match self.eval_expr(&prop.value, env) {
                Completion::Normal(v) => v,
                other => return other,
            };
            // Handle spread
            if let Expression::Spread(inner) = &prop.value {
                let spread_val = match self.eval_expr(inner, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if let JsValue::Object(ref o) = spread_val {
                    if let Some(src) = self.get_object(o.id) {
                        for (k, v) in &src.borrow().properties {
                            obj_data.properties.insert(k.clone(), v.clone());
                        }
                    }
                }
                continue;
            }
            obj_data.properties.insert(key, value);
        }
        let obj = Rc::new(RefCell::new(obj_data));
        self.objects.push(obj);
        Completion::Normal(JsValue::Object(crate::types::JsObject {
            id: self.objects.len() as u64 - 1,
        }))
    }
}

// Type conversion helpers

pub fn to_boolean(val: &JsValue) -> bool {
    match val {
        JsValue::Undefined | JsValue::Null => false,
        JsValue::Boolean(b) => *b,
        JsValue::Number(n) => *n != 0.0 && !n.is_nan(),
        JsValue::String(s) => !s.is_empty(),
        JsValue::BigInt(_) => true, // BigInt(0n) is falsy, but simplified
        JsValue::Symbol(_) | JsValue::Object(_) => true,
    }
}

pub fn to_number(val: &JsValue) -> f64 {
    match val {
        JsValue::Undefined => f64::NAN,
        JsValue::Null => 0.0,
        JsValue::Boolean(b) => *b as u8 as f64,
        JsValue::Number(n) => *n,
        JsValue::String(s) => {
            let rust_str = s.to_rust_string();
            let trimmed = rust_str.trim();
            if trimmed.is_empty() {
                return 0.0;
            }
            trimmed.parse::<f64>().unwrap_or(f64::NAN)
        }
        _ => f64::NAN,
    }
}

pub fn to_js_string(val: &JsValue) -> String {
    format!("{val}")
}

fn is_string(val: &JsValue) -> bool {
    matches!(val, JsValue::String(_))
}

fn strict_equality(left: &JsValue, right: &JsValue) -> bool {
    match (left, right) {
        (JsValue::Undefined, JsValue::Undefined) => true,
        (JsValue::Null, JsValue::Null) => true,
        (JsValue::Boolean(a), JsValue::Boolean(b)) => a == b,
        (JsValue::Number(a), JsValue::Number(b)) => number_ops::equal(*a, *b),
        (JsValue::String(a), JsValue::String(b)) => a == b,
        (JsValue::Object(a), JsValue::Object(b)) => a.id == b.id,
        _ => false,
    }
}

fn abstract_equality(left: &JsValue, right: &JsValue) -> bool {
    // Same type
    if std::mem::discriminant(left) == std::mem::discriminant(right) {
        return strict_equality(left, right);
    }
    // null == undefined
    if (left.is_null() && right.is_undefined()) || (left.is_undefined() && right.is_null()) {
        return true;
    }
    // Number vs String
    if left.is_number() && right.is_string() {
        return abstract_equality(left, &JsValue::Number(to_number(right)));
    }
    if left.is_string() && right.is_number() {
        return abstract_equality(&JsValue::Number(to_number(left)), right);
    }
    // Boolean coercion
    if left.is_boolean() {
        return abstract_equality(&JsValue::Number(to_number(left)), right);
    }
    if right.is_boolean() {
        return abstract_equality(left, &JsValue::Number(to_number(right)));
    }
    false
}

fn abstract_relational(left: &JsValue, right: &JsValue) -> Option<bool> {
    if is_string(left) && is_string(right) {
        let ls = to_js_string(left);
        let rs = to_js_string(right);
        return Some(ls < rs);
    }
    let ln = to_number(left);
    let rn = to_number(right);
    number_ops::less_than(ln, rn)
}

fn typeof_val<'a>(val: &JsValue, objects: &[Rc<RefCell<JsObjectData>>]) -> &'a str {
    match val {
        JsValue::Undefined => "undefined",
        JsValue::Null => "object",
        JsValue::Boolean(_) => "boolean",
        JsValue::Number(_) => "number",
        JsValue::String(_) => "string",
        JsValue::Symbol(_) => "symbol",
        JsValue::BigInt(_) => "bigint",
        JsValue::Object(o) => {
            if let Some(obj) = objects.get(o.id as usize) {
                if obj.borrow().callable.is_some() {
                    return "function";
                }
            }
            "object"
        }
    }
}
