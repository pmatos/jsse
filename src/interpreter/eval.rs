use super::*;

impl Interpreter {
    pub(crate) fn eval_expr(&mut self, expr: &Expression, env: &EnvRef) -> Completion {
        match expr {
            Expression::Literal(lit) => Completion::Normal(self.eval_literal(lit)),
            Expression::Identifier(name) => match env.borrow().get(name) {
                Some(val) => Completion::Normal(val),
                None => {
                    let err = self.create_reference_error(&format!("{name} is not defined"));
                    Completion::Throw(err)
                }
            },
            Expression::This => {
                Completion::Normal(env.borrow().get("this").unwrap_or(JsValue::Undefined))
            }
            Expression::Super => {
                Completion::Normal(env.borrow().get("__super__").unwrap_or(JsValue::Undefined))
            }
            Expression::NewTarget => {
                Completion::Normal(self.new_target.clone().unwrap_or(JsValue::Undefined))
            }
            Expression::PrivateIdentifier(_) => Completion::Throw(
                self.create_type_error("Private identifier can only be used with 'in' operator"),
            ),
            Expression::Unary(op, operand) => {
                let val = match self.eval_expr(operand, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                Completion::Normal(self.eval_unary(*op, &val))
            }
            Expression::Binary(op, left, right) => {
                if *op == BinaryOp::In
                    && let Expression::PrivateIdentifier(name) = left.as_ref() {
                        let rval = match self.eval_expr(right, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        return match &rval {
                            JsValue::Object(o) => {
                                if let Some(obj) = self.get_object(o.id) {
                                    Completion::Normal(JsValue::Boolean(
                                        obj.borrow().private_fields.contains_key(name),
                                    ))
                                } else {
                                    Completion::Normal(JsValue::Boolean(false))
                                }
                            }
                            _ => Completion::Throw(self.create_type_error(
                                "Cannot use 'in' operator to search for a private field without an object",
                            )),
                        };
                    }
                let lval = match self.eval_expr(left, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let rval = match self.eval_expr(right, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                // Proxy has trap for `in` operator
                if *op == BinaryOp::In
                    && let JsValue::Object(ref o) = rval
                        && self.get_proxy_info(o.id).is_some() {
                            let key = to_js_string(&lval);
                            let target_val = self.get_proxy_target_val(o.id);
                            let key_val = JsValue::String(JsString::from_str(&key));
                            match self.invoke_proxy_trap(
                                o.id,
                                "has",
                                vec![target_val.clone(), key_val],
                            ) {
                                Ok(Some(v)) => {
                                    return Completion::Normal(JsValue::Boolean(to_boolean(&v)));
                                }
                                Ok(None) => {
                                    // No trap, fall through to target
                                    if let JsValue::Object(ref t) = target_val
                                        && let Some(tobj) = self.get_object(t.id)
                                    {
                                        return Completion::Normal(JsValue::Boolean(
                                            tobj.borrow().has_property(&key),
                                        ));
                                    }
                                    return Completion::Normal(JsValue::Boolean(false));
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        }
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
                    is_arrow: false,
                    is_strict: Self::is_strict_mode_body(&f.body),
                    is_generator: f.is_generator,
                    is_async: f.is_async,
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
                    body: body_stmts.clone(),
                    closure: env.clone(),
                    is_arrow: true,
                    is_strict: Self::is_strict_mode_body(&body_stmts),
                    is_generator: false,
                    is_async: af.is_async,
                };
                Completion::Normal(self.create_function(func))
            }
            Expression::Class(ce) => {
                let name = ce.name.clone().unwrap_or_default();
                self.eval_class(&name, &ce.super_class, &ce.body, env)
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
            Expression::Delete(expr) => match expr.as_ref() {
                Expression::Member(obj_expr, prop) => {
                    let obj_val = match self.eval_expr(obj_expr, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let key = match prop {
                        MemberProperty::Dot(name) => name.clone(),
                        MemberProperty::Computed(expr) => match self.eval_expr(expr, env) {
                            Completion::Normal(v) => to_js_string(&v),
                            other => return other,
                        },
                        MemberProperty::Private(_) => {
                            return Completion::Throw(
                                self.create_type_error("Private fields cannot be deleted"),
                            );
                        }
                    };
                    if let JsValue::Object(o) = &obj_val
                        && let Some(obj) = self.get_object(o.id)
                    {
                        // Proxy deleteProperty trap
                        if obj.borrow().is_proxy() {
                            let target_val = self.get_proxy_target_val(o.id);
                            let key_val = JsValue::String(JsString::from_str(&key));
                            match self.invoke_proxy_trap(
                                o.id,
                                "deleteProperty",
                                vec![target_val.clone(), key_val],
                            ) {
                                Ok(Some(v)) => {
                                    return Completion::Normal(JsValue::Boolean(to_boolean(&v)));
                                }
                                Ok(None) => {
                                    // No trap, fall through to target
                                    if let JsValue::Object(ref t) = target_val
                                        && let Some(tobj) = self.get_object(t.id)
                                    {
                                        let mut tm = tobj.borrow_mut();
                                        if let Some(desc) = tm.properties.get(&key)
                                            && desc.configurable == Some(false)
                                        {
                                            return Completion::Normal(JsValue::Boolean(false));
                                        }
                                        tm.properties.remove(&key);
                                        tm.property_order.retain(|k| k != &key);
                                    }
                                    return Completion::Normal(JsValue::Boolean(true));
                                }
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                        let mut obj_mut = obj.borrow_mut();
                        if let Some(desc) = obj_mut.properties.get(&key)
                            && desc.configurable == Some(false)
                        {
                            return Completion::Normal(JsValue::Boolean(false));
                        }
                        obj_mut.properties.remove(&key);
                        obj_mut.property_order.retain(|k| k != &key);
                        if let Some(ref mut map) = obj_mut.parameter_map {
                            map.remove(&key);
                        }
                    }
                    Completion::Normal(JsValue::Boolean(true))
                }
                _ => Completion::Normal(JsValue::Boolean(true)),
            },
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
            Expression::Yield(expr, delegate) => {
                if *delegate {
                    let iterable = if let Some(e) = expr {
                        match self.eval_expr(e, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        }
                    } else {
                        JsValue::Undefined
                    };
                    let iterator = match self.get_iterator(&iterable) {
                        Ok(it) => it,
                        Err(e) => return Completion::Throw(e),
                    };
                    loop {
                        let next_result = match self.iterator_next(&iterator) {
                            Ok(v) => v,
                            Err(e) => return Completion::Throw(e),
                        };
                        let (done_val, value) = if let JsValue::Object(ref ro) = next_result {
                            if let Some(robj) = self.get_object(ro.id) {
                                let d = robj.borrow().get_property("done");
                                let v = robj.borrow().get_property("value");
                                (d, v)
                            } else {
                                (JsValue::Undefined, JsValue::Undefined)
                            }
                        } else {
                            (JsValue::Undefined, JsValue::Undefined)
                        };
                        if to_boolean(&done_val) {
                            break Completion::Normal(value);
                        }
                        if let Some(ref mut ctx) = self.generator_context {
                            let current = ctx.current_yield;
                            ctx.current_yield += 1;
                            if current < ctx.target_yield {
                                continue;
                            }
                            if current == ctx.target_yield {
                                return Completion::Yield(value);
                            }
                        }
                        return Completion::Yield(value);
                    }
                } else {
                    if let Some(ref mut ctx) = self.generator_context {
                        let current = ctx.current_yield;
                        ctx.current_yield += 1;
                        if current < ctx.target_yield {
                            return Completion::Normal(ctx.sent_value.clone());
                        }
                        let value = if let Some(e) = expr {
                            match self.eval_expr(e, env) {
                                Completion::Normal(v) => v,
                                other => return other,
                            }
                        } else {
                            JsValue::Undefined
                        };
                        return Completion::Yield(value);
                    }
                    if let Some(e) = expr {
                        self.eval_expr(e, env)
                    } else {
                        Completion::Normal(JsValue::Undefined)
                    }
                }
            }
            Expression::Await(expr) => {
                let val = match self.eval_expr(expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                self.await_value(&val)
            }
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
            Expression::OptionalChain(base, prop) => {
                let base_val = match self.eval_expr(base, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if matches!(base_val, JsValue::Null | JsValue::Undefined) {
                    return Completion::Normal(JsValue::Undefined);
                }
                match prop.as_ref() {
                    Expression::Identifier(name) => match &base_val {
                        JsValue::Object(o) => self.get_object_property(o.id, name, &base_val),
                        JsValue::String(s) => {
                            if name == "length" {
                                Completion::Normal(JsValue::Number(s.len() as f64))
                            } else if let Some(ref sp) = self.string_prototype {
                                Completion::Normal(sp.borrow().get_property(name))
                            } else {
                                Completion::Normal(JsValue::Undefined)
                            }
                        }
                        _ => Completion::Normal(JsValue::Undefined),
                    },
                    Expression::Call(callee, args) => {
                        if let Expression::Identifier(method_name) = callee.as_ref()
                            && method_name.is_empty() {
                                let mut evaluated_args = Vec::new();
                                for arg in args {
                                    let val = match self.eval_expr(arg, env) {
                                        Completion::Normal(v) => v,
                                        other => return other,
                                    };
                                    evaluated_args.push(val);
                                }
                                return self.call_function(
                                    &base_val,
                                    &JsValue::Undefined,
                                    &evaluated_args,
                                );
                            }
                        Completion::Normal(JsValue::Undefined)
                    }
                    Expression::Member(_, mp) => {
                        if let MemberProperty::Private(name) = mp
                            && let JsValue::Object(o) = &base_val
                                && let Some(obj) = self.get_object(o.id) {
                                    let elem = obj.borrow().private_fields.get(name).cloned();
                                    return match elem {
                                        Some(PrivateElement::Field(v)) | Some(PrivateElement::Method(v)) => {
                                            Completion::Normal(v)
                                        }
                                        Some(PrivateElement::Accessor { get, .. }) => {
                                            if let Some(getter) = get {
                                                self.call_function(&getter, &base_val, &[])
                                            } else {
                                                Completion::Throw(self.create_type_error(&format!(
                                                    "Cannot read private member #{name} which has no getter"
                                                )))
                                            }
                                        }
                                        None => Completion::Throw(self.create_type_error(&format!(
                                            "Cannot read private member #{name} from an object whose class did not declare it"
                                        ))),
                                    };
                                }
                        Completion::Normal(JsValue::Undefined)
                    }
                    other => {
                        let key_val = match self.eval_expr(other, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let key = to_js_string(&key_val);
                        match &base_val {
                            JsValue::Object(o) => self.get_object_property(o.id, &key, &base_val),
                            _ => Completion::Normal(JsValue::Undefined),
                        }
                    }
                }
            }
            _ => Completion::Normal(JsValue::Undefined),
        }
    }

    fn eval_literal(&mut self, lit: &Literal) -> JsValue {
        match lit {
            Literal::Null => JsValue::Null,
            Literal::Boolean(b) => JsValue::Boolean(*b),
            Literal::Number(n) => JsValue::Number(*n),
            Literal::String(s) => JsValue::String(JsString::from_str(s)),
            Literal::BigInt(_) => JsValue::Undefined, // TODO
            Literal::RegExp(pattern, flags) => {
                let mut obj = JsObjectData::new();
                obj.prototype = self
                    .regexp_prototype
                    .clone()
                    .or(self.object_prototype.clone());
                obj.class_name = "RegExp".to_string();
                obj.insert_value(
                    "source".to_string(),
                    JsValue::String(JsString::from_str(pattern)),
                );
                obj.insert_value(
                    "flags".to_string(),
                    JsValue::String(JsString::from_str(flags)),
                );
                obj.insert_value("global".to_string(), JsValue::Boolean(flags.contains('g')));
                obj.insert_value(
                    "ignoreCase".to_string(),
                    JsValue::Boolean(flags.contains('i')),
                );
                obj.insert_value(
                    "multiline".to_string(),
                    JsValue::Boolean(flags.contains('m')),
                );
                obj.insert_value("dotAll".to_string(), JsValue::Boolean(flags.contains('s')));
                obj.insert_value("unicode".to_string(), JsValue::Boolean(flags.contains('u')));
                obj.insert_value("sticky".to_string(), JsValue::Boolean(flags.contains('y')));
                obj.insert_value("lastIndex".to_string(), JsValue::Number(0.0));
                let rc = Rc::new(RefCell::new(obj));
                let id = self.allocate_object_slot(rc);
                JsValue::Object(crate::types::JsObject { id })
            }
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

    fn require_object_coercible(&mut self, val: &JsValue) -> Completion {
        match val {
            JsValue::Undefined | JsValue::Null => {
                let err = self.create_type_error("Cannot convert undefined or null to object");
                Completion::Throw(err)
            }
            _ => Completion::Normal(val.clone()),
        }
    }

    fn is_regexp(&self, val: &JsValue) -> bool {
        if let JsValue::Object(o) = val
            && let Some(obj) = self.get_object(o.id)
        {
            return obj.borrow().class_name == "RegExp";
        }
        false
    }

    fn canonical_numeric_index_string(s: &str) -> Option<f64> {
        if s == "-0" {
            return Some(-0.0_f64);
        }
        let n: f64 = s.parse().ok()?;
        if format!("{n}") == s { Some(n) } else { None }
    }

    fn to_index(&mut self, val: &JsValue) -> Completion {
        if val.is_undefined() {
            return Completion::Normal(JsValue::Number(0.0));
        }
        let integer_index = to_number(val);
        let integer_index = if integer_index.is_nan() {
            0.0
        } else {
            integer_index.trunc()
        };
        if !(0.0..=9007199254740991.0).contains(&integer_index) {
            let err = self.create_error("RangeError", "Invalid index");
            return Completion::Throw(err);
        }
        Completion::Normal(JsValue::Number(integer_index))
    }

    fn to_length(val: &JsValue) -> f64 {
        let len = to_number(val);
        if len.is_nan() || len <= 0.0 {
            return 0.0;
        }
        len.min(9007199254740991.0).floor() // 2^53 - 1
    }

    pub(crate) fn to_object(&mut self, val: &JsValue) -> Completion {
        match val {
            JsValue::Undefined | JsValue::Null => {
                let err = self.create_type_error("Cannot convert undefined or null to object");
                Completion::Throw(err)
            }
            JsValue::Boolean(_)
            | JsValue::Number(_)
            | JsValue::String(_)
            | JsValue::Symbol(_)
            | JsValue::BigInt(_) => {
                let mut obj_data = JsObjectData::new();
                obj_data.primitive_value = Some(val.clone());
                match val {
                    JsValue::String(_) => {
                        obj_data.class_name = "String".to_string();
                        if let Some(ref sp) = self.string_prototype {
                            obj_data.prototype = Some(sp.clone());
                        }
                    }
                    JsValue::Number(_) => {
                        obj_data.class_name = "Number".to_string();
                        if let Some(ref np) = self.number_prototype {
                            obj_data.prototype = Some(np.clone());
                        }
                    }
                    JsValue::Boolean(_) => {
                        obj_data.class_name = "Boolean".to_string();
                        if let Some(ref bp) = self.boolean_prototype {
                            obj_data.prototype = Some(bp.clone());
                        }
                    }
                    JsValue::Symbol(_) => {
                        obj_data.class_name = "Symbol".to_string();
                        if let Some(ref sp) = self.symbol_prototype {
                            obj_data.prototype = Some(sp.clone());
                        }
                    }
                    JsValue::BigInt(_) => obj_data.class_name = "BigInt".to_string(),
                    _ => unreachable!(),
                }
                if obj_data.prototype.is_none() {
                    obj_data.prototype = self.object_prototype.clone();
                }
                let obj = Rc::new(RefCell::new(obj_data));
                let id = self.allocate_object_slot(obj);
                Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
            }
            JsValue::Object(_) => Completion::Normal(val.clone()),
        }
    }

    fn to_primitive(&mut self, val: &JsValue, preferred_type: &str) -> JsValue {
        match val {
            JsValue::Object(o) => {
                let methods = if preferred_type == "string" {
                    ["toString", "valueOf"]
                } else {
                    ["valueOf", "toString"]
                };
                for method_name in &methods {
                    let method = if let Some(obj) = self.get_object(o.id) {
                        let desc = obj.borrow().get_property_descriptor(method_name);
                        desc.and_then(|d| d.value)
                    } else {
                        None
                    };
                    if let Some(func) = method
                        && let JsValue::Object(fo) = &func
                        && self
                            .get_object(fo.id)
                            .map(|o| o.borrow().callable.is_some())
                            .unwrap_or(false)
                    {
                        let result = self.call_function(&func, val, &[]);
                        match result {
                            Completion::Normal(v) if !matches!(v, JsValue::Object(_)) => {
                                return v;
                            }
                            _ => {}
                        }
                    }
                }
                // Fallback: check for primitive_value (wrapper objects)
                if let Some(obj) = self.get_object(o.id)
                    && let Some(pv) = obj.borrow().primitive_value.clone()
                {
                    return pv;
                }
                JsValue::String(JsString::from_str("[object Object]"))
            }
            _ => val.clone(),
        }
    }

    pub(crate) fn to_number_coerce(&mut self, val: &JsValue) -> f64 {
        let prim = self.to_primitive(val, "number");
        to_number(&prim)
    }

    fn abstract_equality(&mut self, left: &JsValue, right: &JsValue) -> bool {
        if std::mem::discriminant(left) == std::mem::discriminant(right) {
            return strict_equality(left, right);
        }
        if (left.is_null() && right.is_undefined()) || (left.is_undefined() && right.is_null()) {
            return true;
        }
        if left.is_number() && right.is_string() {
            return self.abstract_equality(left, &JsValue::Number(to_number(right)));
        }
        if left.is_string() && right.is_number() {
            return self.abstract_equality(&JsValue::Number(to_number(left)), right);
        }
        if left.is_boolean() {
            return self.abstract_equality(&JsValue::Number(to_number(left)), right);
        }
        if right.is_boolean() {
            return self.abstract_equality(left, &JsValue::Number(to_number(right)));
        }
        // Object vs primitive
        if matches!(left, JsValue::Object(_))
            && (right.is_string() || right.is_number() || right.is_symbol())
        {
            let lprim = self.to_primitive(left, "default");
            return self.abstract_equality(&lprim, right);
        }
        if matches!(right, JsValue::Object(_))
            && (left.is_string() || left.is_number() || left.is_symbol())
        {
            let rprim = self.to_primitive(right, "default");
            return self.abstract_equality(left, &rprim);
        }
        false
    }

    fn abstract_relational(&mut self, left: &JsValue, right: &JsValue) -> Option<bool> {
        let lprim = self.to_primitive(left, "number");
        let rprim = self.to_primitive(right, "number");
        if is_string(&lprim) && is_string(&rprim) {
            let ls = to_js_string(&lprim);
            let rs = to_js_string(&rprim);
            return Some(ls < rs);
        }
        let ln = to_number(&lprim);
        let rn = to_number(&rprim);
        number_ops::less_than(ln, rn)
    }

    fn eval_binary(&mut self, op: BinaryOp, left: &JsValue, right: &JsValue) -> JsValue {
        match op {
            BinaryOp::Add => {
                let lprim = self.to_primitive(left, "default");
                let rprim = self.to_primitive(right, "default");
                if is_string(&lprim) || is_string(&rprim) {
                    let ls = to_js_string(&lprim);
                    let rs = to_js_string(&rprim);
                    JsValue::String(JsString::from_str(&format!("{ls}{rs}")))
                } else {
                    JsValue::Number(number_ops::add(to_number(&lprim), to_number(&rprim)))
                }
            }
            BinaryOp::Sub => JsValue::Number(number_ops::subtract(
                self.to_number_coerce(left),
                self.to_number_coerce(right),
            )),
            BinaryOp::Mul => JsValue::Number(number_ops::multiply(
                self.to_number_coerce(left),
                self.to_number_coerce(right),
            )),
            BinaryOp::Div => JsValue::Number(number_ops::divide(
                self.to_number_coerce(left),
                self.to_number_coerce(right),
            )),
            BinaryOp::Mod => JsValue::Number(number_ops::remainder(
                self.to_number_coerce(left),
                self.to_number_coerce(right),
            )),
            BinaryOp::Exp => JsValue::Number(number_ops::exponentiate(
                self.to_number_coerce(left),
                self.to_number_coerce(right),
            )),
            BinaryOp::Eq => JsValue::Boolean(self.abstract_equality(left, right)),
            BinaryOp::NotEq => JsValue::Boolean(!self.abstract_equality(left, right)),
            BinaryOp::StrictEq => JsValue::Boolean(strict_equality(left, right)),
            BinaryOp::StrictNotEq => JsValue::Boolean(!strict_equality(left, right)),
            BinaryOp::Lt => JsValue::Boolean(self.abstract_relational(left, right) == Some(true)),
            BinaryOp::Gt => JsValue::Boolean(self.abstract_relational(right, left) == Some(true)),
            BinaryOp::LtEq => {
                JsValue::Boolean(self.abstract_relational(right, left) == Some(false))
            }
            BinaryOp::GtEq => {
                JsValue::Boolean(self.abstract_relational(left, right) == Some(false))
            }
            BinaryOp::LShift => JsValue::Number(number_ops::left_shift(
                self.to_number_coerce(left),
                self.to_number_coerce(right),
            )),
            BinaryOp::RShift => JsValue::Number(number_ops::signed_right_shift(
                self.to_number_coerce(left),
                self.to_number_coerce(right),
            )),
            BinaryOp::URShift => JsValue::Number(number_ops::unsigned_right_shift(
                self.to_number_coerce(left),
                self.to_number_coerce(right),
            )),
            BinaryOp::BitAnd => JsValue::Number(number_ops::bitwise_and(
                self.to_number_coerce(left),
                self.to_number_coerce(right),
            )),
            BinaryOp::BitOr => JsValue::Number(number_ops::bitwise_or(
                self.to_number_coerce(left),
                self.to_number_coerce(right),
            )),
            BinaryOp::BitXor => JsValue::Number(number_ops::bitwise_xor(
                self.to_number_coerce(left),
                self.to_number_coerce(right),
            )),
            BinaryOp::In => {
                if let JsValue::Object(o) = &right {
                    if let Some(obj) = self.get_object(o.id) {
                        let key = to_js_string(left);
                        let obj_ref = obj.borrow();
                        JsValue::Boolean(obj_ref.has_property(&key))
                    } else {
                        JsValue::Boolean(false)
                    }
                } else {
                    JsValue::Boolean(false)
                }
            }
            BinaryOp::Instanceof => {
                if let JsValue::Object(rhs) = &right {
                    if let Some(ctor_obj) = self.get_object(rhs.id) {
                        let proto_val = ctor_obj.borrow().get_property("prototype");
                        if let JsValue::Object(proto) = &proto_val {
                            if let Some(proto_data) = self.get_object(proto.id) {
                                if let JsValue::Object(lhs) = &left {
                                    if let Some(inst_obj) = self.get_object(lhs.id) {
                                        let mut current = inst_obj.borrow().prototype.clone();
                                        let mut result = false;
                                        while let Some(p) = current {
                                            if Rc::ptr_eq(&p, &proto_data) {
                                                result = true;
                                                break;
                                            }
                                            current = p.borrow().prototype.clone();
                                        }
                                        JsValue::Boolean(result)
                                    } else {
                                        JsValue::Boolean(false)
                                    }
                                } else {
                                    JsValue::Boolean(false)
                                }
                            } else {
                                JsValue::Boolean(false)
                            }
                        } else {
                            JsValue::Boolean(false)
                        }
                    } else {
                        JsValue::Boolean(false)
                    }
                } else {
                    JsValue::Boolean(false)
                }
            }
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
                    let err = self.create_reference_error(&format!("{name} is not defined"));
                    return Completion::Throw(err);
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
        // Logical assignments are short-circuit
        if matches!(
            op,
            AssignOp::LogicalAndAssign | AssignOp::LogicalOrAssign | AssignOp::NullishAssign
        ) {
            let lval = match self.eval_expr(left, env) {
                Completion::Normal(v) => v,
                other => return other,
            };
            let should_assign = match op {
                AssignOp::LogicalAndAssign => to_boolean(&lval),
                AssignOp::LogicalOrAssign => !to_boolean(&lval),
                AssignOp::NullishAssign => lval.is_null() || lval.is_undefined(),
                _ => unreachable!(),
            };
            if !should_assign {
                return Completion::Normal(lval);
            }
            let rval = match self.eval_expr(right, env) {
                Completion::Normal(v) => v,
                other => return other,
            };
            if let Expression::Identifier(name) = left {
                let _ = env.borrow_mut().set(name, rval.clone());
            }
            return Completion::Normal(rval);
        }

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
                if let MemberProperty::Private(name) = prop {
                    return match &obj_val {
                        JsValue::Object(o) => {
                            if let Some(obj) = self.get_object(o.id) {
                                let elem = obj.borrow().private_fields.get(name).cloned();
                                match elem {
                                    Some(PrivateElement::Field(_)) => {
                                        let final_val = if op == AssignOp::Assign {
                                            rval
                                        } else {
                                            let lval = if let Some(PrivateElement::Field(v)) = obj.borrow().private_fields.get(name) {
                                                v.clone()
                                            } else {
                                                JsValue::Undefined
                                            };
                                            self.apply_compound_assign(op, &lval, &rval)
                                        };
                                        obj.borrow_mut()
                                            .private_fields
                                            .insert(name.clone(), PrivateElement::Field(final_val.clone()));
                                        Completion::Normal(final_val)
                                    }
                                    Some(PrivateElement::Method(_)) => {
                                        Completion::Throw(self.create_type_error(&format!(
                                            "Cannot assign to private method #{name}"
                                        )))
                                    }
                                    Some(PrivateElement::Accessor { get, set }) => {
                                        if let Some(setter) = &set {
                                            let final_val = if op == AssignOp::Assign {
                                                rval
                                            } else {
                                                let lval = if let Some(ref getter) = get {
                                                    match self.call_function(getter, &obj_val, &[]) {
                                                        Completion::Normal(v) => v,
                                                        other => return other,
                                                    }
                                                } else {
                                                    JsValue::Undefined
                                                };
                                                self.apply_compound_assign(op, &lval, &rval)
                                            };
                                            let setter = setter.clone();
                                            self.call_function(&setter, &obj_val, &[final_val.clone()]);
                                            Completion::Normal(final_val)
                                        } else {
                                            Completion::Throw(self.create_type_error(&format!(
                                                "Cannot set private member #{name} which has no setter"
                                            )))
                                        }
                                    }
                                    None => {
                                        Completion::Throw(self.create_type_error(&format!(
                                            "Cannot write private member #{name} to an object whose class did not declare it"
                                        )))
                                    }
                                }
                            } else {
                                Completion::Normal(JsValue::Undefined)
                            }
                        }
                        _ => Completion::Throw(self.create_type_error(&format!(
                            "Cannot write private member #{name} to a non-object"
                        ))),
                    };
                }
                let key = match prop {
                    MemberProperty::Dot(name) => name.clone(),
                    MemberProperty::Computed(expr) => {
                        let v = match self.eval_expr(expr, env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        to_js_string(&v)
                    }
                    MemberProperty::Private(_) => unreachable!(),
                };
                if let JsValue::Object(ref o) = obj_val
                    && let Some(obj) = self.get_object(o.id)
                {
                    let final_val = if op == AssignOp::Assign {
                        rval
                    } else {
                        let lval = obj.borrow().get_property(&key);
                        self.apply_compound_assign(op, &lval, &rval)
                    };
                    // Proxy set trap
                    if obj.borrow().is_proxy() {
                        let target_val = self.get_proxy_target_val(o.id);
                        let key_val = JsValue::String(JsString::from_str(&key));
                        let receiver = obj_val.clone();
                        match self.invoke_proxy_trap(
                            o.id,
                            "set",
                            vec![target_val.clone(), key_val, final_val.clone(), receiver],
                        ) {
                            Ok(Some(v)) => {
                                if to_boolean(&v) {
                                    return Completion::Normal(final_val);
                                }
                                return Completion::Normal(final_val);
                            }
                            Ok(None) => {
                                // No trap, fall through to target
                                if let JsValue::Object(ref t) = target_val
                                    && let Some(tobj) = self.get_object(t.id)
                                {
                                    let success = tobj
                                        .borrow_mut()
                                        .set_property_value(&key, final_val.clone());
                                    if !success && env.borrow().strict {
                                        return Completion::Throw(self.create_type_error(
                                            &format!("Cannot assign to read only property '{key}'"),
                                        ));
                                    }
                                }
                                return Completion::Normal(final_val);
                            }
                            Err(e) => return Completion::Throw(e),
                        }
                    }
                    // Check for setter
                    let desc = obj.borrow().get_property_descriptor(&key);
                    if let Some(ref d) = desc
                        && let Some(ref setter) = d.set
                    {
                        let setter = setter.clone();
                        let this = obj_val.clone();
                        return match self.call_function(&setter, &this, &[final_val.clone()]) {
                            Completion::Normal(_) => Completion::Normal(final_val),
                            other => other,
                        };
                    }
                    let success = obj.borrow_mut().set_property_value(&key, final_val.clone());
                    if !success && env.borrow().strict {
                        return Completion::Throw(self.create_type_error(&format!(
                            "Cannot assign to read only property '{key}'"
                        )));
                    }
                    return Completion::Normal(final_val);
                }
                Completion::Normal(rval)
            }
            Expression::Array(elements) if op == AssignOp::Assign => {
                // Destructuring array assignment
                for (i, elem) in elements.iter().enumerate() {
                    if let Some(expr) = elem {
                        if let Expression::Spread(inner) = expr {
                            let rest: Vec<JsValue> = if let JsValue::Object(o) = &rval {
                                if let Some(obj) = self.get_object(o.id) {
                                    obj.borrow()
                                        .array_elements
                                        .as_ref()
                                        .map(|e| e.get(i..).unwrap_or(&[]).to_vec())
                                        .unwrap_or_default()
                                } else {
                                    vec![]
                                }
                            } else {
                                vec![]
                            };
                            let arr = self.create_array(rest);
                            let _result = self.eval_assign(
                                AssignOp::Assign,
                                inner,
                                &Expression::Literal(Literal::Null),
                                env,
                            );
                            // Assign directly
                            if let Expression::Identifier(name) = inner.as_ref() {
                                if !env.borrow().has(name) {
                                    env.borrow_mut().declare(name, BindingKind::Var);
                                }
                                let _ = env.borrow_mut().set(name, arr);
                            }
                            break;
                        }
                        let item = if let JsValue::Object(o) = &rval {
                            if let Some(obj) = self.get_object(o.id) {
                                obj.borrow()
                                    .array_elements
                                    .as_ref()
                                    .and_then(|e| e.get(i).cloned())
                                    .unwrap_or(JsValue::Undefined)
                            } else {
                                JsValue::Undefined
                            }
                        } else {
                            JsValue::Undefined
                        };
                        // Check for default value: `[a = defaultVal] = arr`
                        let (target, val) =
                            if let Expression::Assign(AssignOp::Assign, target, default) = expr {
                                let v = if item.is_undefined() {
                                    match self.eval_expr(default, env) {
                                        Completion::Normal(v) => v,
                                        other => return other,
                                    }
                                } else {
                                    item
                                };
                                (target.as_ref(), v)
                            } else {
                                (expr, item)
                            };
                        match target {
                            Expression::Identifier(name) => {
                                if !env.borrow().has(name) {
                                    env.borrow_mut().declare(name, BindingKind::Var);
                                }
                                let _ = env.borrow_mut().set(name, val);
                            }
                            Expression::Member(..) => {
                                // Create a temp to hold the val, assign to member
                                let _temp_lit = Expression::Literal(Literal::Null);
                                // We'd need to manually do the member assign here
                                // For now, skip complex member destructuring
                            }
                            _ => {}
                        }
                    }
                }
                Completion::Normal(rval)
            }
            Expression::Object(props) if op == AssignOp::Assign => {
                // Destructuring object assignment
                for prop in props {
                    let (key, target, default_val) = match &prop.kind {
                        PropertyKind::Init => {
                            let key = match &prop.key {
                                PropertyKey::Identifier(s) | PropertyKey::String(s) => s.clone(),
                                PropertyKey::Number(n) => to_js_string(&JsValue::Number(*n)),
                                PropertyKey::Computed(expr) => match self.eval_expr(expr, env) {
                                    Completion::Normal(v) => to_js_string(&v),
                                    other => return other,
                                },
                                PropertyKey::Private(_) => {
                                    return Completion::Throw(self.create_type_error(
                                        "Private names are not valid in object patterns",
                                    ));
                                }
                            };
                            // Check if shorthand ({a} = obj) or key-value ({a: b} = obj)
                            if let Expression::Identifier(name) = &prop.value {
                                if name == &key {
                                    (key, prop.value.clone(), None)
                                } else {
                                    (key, prop.value.clone(), None)
                                }
                            } else if let Expression::Assign(AssignOp::Assign, target, default) =
                                &prop.value
                            {
                                (key, *target.clone(), Some(*default.clone()))
                            } else {
                                (key, prop.value.clone(), None)
                            }
                        }
                        _ => continue,
                    };
                    let val = if let JsValue::Object(o) = &rval {
                        if let Some(obj) = self.get_object(o.id) {
                            obj.borrow().get_property(&key)
                        } else {
                            JsValue::Undefined
                        }
                    } else {
                        JsValue::Undefined
                    };
                    let val = if val.is_undefined() {
                        if let Some(default) = default_val {
                            match self.eval_expr(&default, env) {
                                Completion::Normal(v) => v,
                                other => return other,
                            }
                        } else {
                            val
                        }
                    } else {
                        val
                    };
                    if let Expression::Identifier(name) = &target {
                        if !env.borrow().has(name) {
                            env.borrow_mut().declare(name, BindingKind::Var);
                        }
                        let _ = env.borrow_mut().set(name, val);
                    }
                }
                Completion::Normal(rval)
            }
            _ => Completion::Normal(rval),
        }
    }

    fn apply_compound_assign(&mut self, op: AssignOp, lval: &JsValue, rval: &JsValue) -> JsValue {
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
        // Handle super() calls - call parent constructor with current this
        if matches!(callee, Expression::Super) {
            let super_ctor = env.borrow().get("__super__").unwrap_or(JsValue::Undefined);
            let this_val = env.borrow().get("this").unwrap_or(JsValue::Undefined);
            let mut arg_vals = Vec::new();
            for arg in args {
                match self.eval_expr(arg, env) {
                    Completion::Normal(v) => arg_vals.push(v),
                    other => return other,
                }
            }
            return self.call_function(&super_ctor, &this_val, &arg_vals);
        }

        // Handle member calls: obj.method()
        let (func_val, this_val) = match callee {
            Expression::Member(obj_expr, prop) => {
                let is_super_call = matches!(obj_expr.as_ref(), Expression::Super);
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
                    MemberProperty::Private(name) => {
                        if let JsValue::Object(ref o) = obj_val
                            && let Some(obj) = self.get_object(o.id) {
                                let elem = obj.borrow().private_fields.get(name).cloned();
                                let func_val = match elem {
                                    Some(PrivateElement::Field(v))
                                    | Some(PrivateElement::Method(v)) => v,
                                    Some(PrivateElement::Accessor { get, .. }) => {
                                        if let Some(getter) = get {
                                            match self.call_function(&getter, &obj_val, &[]) {
                                                Completion::Normal(v) => v,
                                                other => return other,
                                            }
                                        } else {
                                            return Completion::Throw(self.create_type_error(&format!(
                                                "Cannot read private member #{name} which has no getter"
                                            )));
                                        }
                                    }
                                    None => {
                                        return Completion::Throw(self.create_type_error(&format!(
                                            "Cannot read private member #{name} from an object whose class did not declare it"
                                        )));
                                    }
                                };
                                let mut evaluated_args = Vec::new();
                                for arg in args {
                                    match arg {
                                        Expression::Spread(inner) => {
                                            let spread_val = match self.eval_expr(inner, env) {
                                                Completion::Normal(v) => v,
                                                other => return other,
                                            };
                                            if let Ok(items) = self.iterate_to_vec(&spread_val) {
                                                evaluated_args.extend(items);
                                            }
                                        }
                                        _ => match self.eval_expr(arg, env) {
                                            Completion::Normal(v) => evaluated_args.push(v),
                                            other => return other,
                                        },
                                    }
                                }
                                return self.call_function(&func_val, &obj_val, &evaluated_args);
                            }
                        return Completion::Throw(self.create_type_error(&format!(
                            "Cannot read private member #{name} from a non-object"
                        )));
                    }
                };
                // super.method() - look up on super constructor's prototype, bind this
                if is_super_call {
                    if let JsValue::Object(ref o) = obj_val {
                        if let Some(obj) = self.get_object(o.id) {
                            let proto_val = obj.borrow().get_property("prototype");
                            if let JsValue::Object(ref p) = proto_val {
                                if let Some(proto) = self.get_object(p.id) {
                                    let method = proto.borrow().get_property(&key);
                                    let this_val =
                                        env.borrow().get("this").unwrap_or(JsValue::Undefined);
                                    (method, this_val)
                                } else {
                                    (JsValue::Undefined, JsValue::Undefined)
                                }
                            } else {
                                (JsValue::Undefined, JsValue::Undefined)
                            }
                        } else {
                            (JsValue::Undefined, JsValue::Undefined)
                        }
                    } else {
                        (JsValue::Undefined, JsValue::Undefined)
                    }
                } else if let JsValue::Object(ref o) = obj_val {
                    let oid = o.id;
                    let ov = obj_val.clone();
                    match self.get_object_property(oid, &key, &ov) {
                        Completion::Normal(method) => (method, obj_val),
                        other => return other,
                    }
                } else if let JsValue::String(_) = &obj_val {
                    if let Some(ref sp) = self.string_prototype {
                        let method = sp.borrow().get_property(&key);
                        (method, obj_val)
                    } else {
                        (JsValue::Undefined, obj_val)
                    }
                } else if matches!(&obj_val, JsValue::Number(_)) {
                    let proto = self
                        .number_prototype
                        .clone()
                        .or(self.object_prototype.clone());
                    if let Some(ref p) = proto {
                        let method = p.borrow().get_property(&key);
                        (method, obj_val)
                    } else {
                        (JsValue::Undefined, obj_val)
                    }
                } else if matches!(&obj_val, JsValue::Boolean(_)) {
                    let proto = self
                        .boolean_prototype
                        .clone()
                        .or(self.object_prototype.clone());
                    if let Some(ref p) = proto {
                        let method = p.borrow().get_property(&key);
                        (method, obj_val)
                    } else {
                        (JsValue::Undefined, obj_val)
                    }
                } else if matches!(&obj_val, JsValue::Symbol(_)) {
                    if let Some(ref p) = self.symbol_prototype {
                        let desc = p.borrow().get_property_descriptor(&key);
                        let method = match desc {
                            Some(ref d) if d.get.is_some() => {
                                let getter = d.get.clone().unwrap();
                                match self.call_function(&getter, &obj_val, &[]) {
                                    Completion::Normal(v) => v,
                                    other => return other,
                                }
                            }
                            Some(ref d) => d.value.clone().unwrap_or(JsValue::Undefined),
                            None => JsValue::Undefined,
                        };
                        (method, obj_val)
                    } else {
                        (JsValue::Undefined, obj_val)
                    }
                } else if matches!(&obj_val, JsValue::Undefined | JsValue::Null) {
                    let err = self.create_type_error(&format!(
                        "Cannot read properties of {obj_val} (reading '{key}')"
                    ));
                    return Completion::Throw(err);
                } else {
                    (JsValue::Undefined, obj_val)
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

        let evaluated_args = match self.eval_spread_args(args, env) {
            Ok(args) => args,
            Err(e) => return Completion::Throw(e),
        };

        self.call_function(&func_val, &this_val, &evaluated_args)
    }

    pub(crate) fn generator_next(&mut self, this: &JsValue, sent_value: JsValue) -> Completion {
        let JsValue::Object(o) = this else {
            let err = self.create_type_error("Generator.prototype.next called on non-object");
            return Completion::Throw(err);
        };
        let Some(obj_rc) = self.get_object(o.id) else {
            let err = self.create_type_error("Generator.prototype.next called on non-object");
            return Completion::Throw(err);
        };

        // Extract state (must release borrow before executing body)
        let state = obj_rc.borrow().iterator_state.clone();
        let Some(IteratorState::Generator {
            body,
            params,
            closure,
            is_strict,
            args,
            this_val,
            target_yield,
            done,
        }) = state
        else {
            let err = self.create_type_error("not a generator object");
            return Completion::Throw(err);
        };

        if done {
            return Completion::Normal(self.create_iter_result_object(JsValue::Undefined, true));
        }

        // Create fresh function environment and bind params
        let func_env = Environment::new(Some(closure.clone()));
        for (i, param) in params.iter().enumerate() {
            if let Pattern::Rest(inner) = param {
                let rest: Vec<JsValue> = args.get(i..).unwrap_or(&[]).to_vec();
                let rest_arr = self.create_array(rest);
                let _ = self.bind_pattern(inner, rest_arr, BindingKind::Var, &func_env);
                break;
            }
            let val = args.get(i).cloned().unwrap_or(JsValue::Undefined);
            let _ = self.bind_pattern(param, val, BindingKind::Var, &func_env);
        }
        func_env.borrow_mut().bindings.insert(
            "this".to_string(),
            Binding {
                value: this_val.clone(),
                kind: BindingKind::Const,
                initialized: true,
            },
        );
        // arguments object
        let arguments_obj =
            self.create_arguments_object(&args, JsValue::Undefined, is_strict, None, &[]);
        func_env.borrow_mut().declare("arguments", BindingKind::Var);
        let _ = func_env.borrow_mut().set("arguments", arguments_obj);

        // Set generator context for replay
        self.generator_context = Some(GeneratorContext {
            target_yield,
            current_yield: 0,
            sent_value,
        });

        let result = self.exec_statements(&body, &func_env);
        let _ctx = self.generator_context.take();

        match result {
            Completion::Yield(v) => {
                // Advance target_yield for next call
                obj_rc.borrow_mut().iterator_state = Some(IteratorState::Generator {
                    body: body.clone(),
                    params: params.clone(),
                    closure: closure.clone(),
                    is_strict,
                    args: args.clone(),
                    this_val: this_val.clone(),
                    target_yield: target_yield + 1,
                    done: false,
                });
                Completion::Normal(self.create_iter_result_object(v, false))
            }
            Completion::Return(v) => {
                obj_rc.borrow_mut().iterator_state = Some(IteratorState::Generator {
                    body,
                    params,
                    closure,
                    is_strict,
                    args,
                    this_val,
                    target_yield,
                    done: true,
                });
                Completion::Normal(self.create_iter_result_object(v, true))
            }
            Completion::Normal(_) => {
                obj_rc.borrow_mut().iterator_state = Some(IteratorState::Generator {
                    body,
                    params,
                    closure,
                    is_strict,
                    args,
                    this_val,
                    target_yield,
                    done: true,
                });
                Completion::Normal(self.create_iter_result_object(JsValue::Undefined, true))
            }
            Completion::Throw(e) => {
                obj_rc.borrow_mut().iterator_state = Some(IteratorState::Generator {
                    body,
                    params,
                    closure,
                    is_strict,
                    args,
                    this_val,
                    target_yield,
                    done: true,
                });
                Completion::Throw(e)
            }
            other => other,
        }
    }

    pub(crate) fn generator_return(&mut self, this: &JsValue, value: JsValue) -> Completion {
        let JsValue::Object(o) = this else {
            let err = self.create_type_error("Generator.prototype.return called on non-object");
            return Completion::Throw(err);
        };
        if let Some(obj_rc) = self.get_object(o.id)
            && let Some(IteratorState::Generator {
                body,
                params,
                closure,
                is_strict,
                args,
                this_val,
                target_yield,
                ..
            }) = obj_rc.borrow().iterator_state.clone()
        {
            obj_rc.borrow_mut().iterator_state = Some(IteratorState::Generator {
                body,
                params,
                closure,
                is_strict,
                args,
                this_val,
                target_yield,
                done: true,
            });
        }
        Completion::Normal(self.create_iter_result_object(value, true))
    }

    pub(crate) fn generator_throw(&mut self, this: &JsValue, exception: JsValue) -> Completion {
        let JsValue::Object(o) = this else {
            let err = self.create_type_error("Generator.prototype.throw called on non-object");
            return Completion::Throw(err);
        };
        if let Some(obj_rc) = self.get_object(o.id)
            && let Some(IteratorState::Generator {
                body,
                params,
                closure,
                is_strict,
                args,
                this_val,
                target_yield,
                ..
            }) = obj_rc.borrow().iterator_state.clone()
        {
            obj_rc.borrow_mut().iterator_state = Some(IteratorState::Generator {
                body,
                params,
                closure,
                is_strict,
                args,
                this_val,
                target_yield,
                done: true,
            });
        }
        Completion::Throw(exception)
    }

    pub(crate) fn call_function(
        &mut self,
        func_val: &JsValue,
        _this_val: &JsValue,
        args: &[JsValue],
    ) -> Completion {
        if let JsValue::Object(o) = func_val
            && let Some(obj) = self.get_object(o.id)
        {
            // Proxy apply trap
            if obj.borrow().is_proxy() {
                let target_val = self.get_proxy_target_val(o.id);
                let args_array = self.create_array(args.to_vec());
                match self.invoke_proxy_trap(
                    o.id,
                    "apply",
                    vec![target_val.clone(), _this_val.clone(), args_array],
                ) {
                    Ok(Some(v)) => return Completion::Normal(v),
                    Ok(None) => {
                        // No trap, call target directly
                        return self.call_function(&target_val, _this_val, args);
                    }
                    Err(e) => return Completion::Throw(e),
                }
            }
            let callable = obj.borrow().callable.clone();
            if let Some(func) = callable {
                return match func {
                    JsFunction::Native(_, _, f) => f(self, _this_val, args),
                    JsFunction::User {
                        params,
                        body,
                        closure,
                        is_arrow,
                        is_strict,
                        is_generator,
                        is_async,
                        ..
                    } => {
                        if is_async && !is_generator {
                            return self.call_async_function(
                                &params,
                                &body,
                                closure.clone(),
                                is_arrow,
                                is_strict,
                                _this_val,
                                args,
                                func_val,
                            );
                        }
                        if is_generator {
                            // Create a generator object instead of executing
                            let gen_obj = self.create_object();
                            // Set prototype from the function's .prototype property
                            if let Some(func_obj_rc) = self.get_object(o.id) {
                                let proto_val =
                                    func_obj_rc.borrow().get_property_value("prototype");
                                if let Some(JsValue::Object(ref p)) = proto_val
                                    && let Some(proto_rc) = self.get_object(p.id) {
                                        gen_obj.borrow_mut().prototype = Some(proto_rc);
                                    }
                            }
                            gen_obj.borrow_mut().class_name = "Generator".to_string();
                            gen_obj.borrow_mut().iterator_state = Some(IteratorState::Generator {
                                body: body.clone(),
                                params: params.clone(),
                                closure: closure.clone(),
                                is_strict,
                                args: args.to_vec(),
                                this_val: _this_val.clone(),
                                target_yield: 0,
                                done: false,
                            });
                            let gen_id = gen_obj.borrow().id.unwrap();
                            return Completion::Normal(JsValue::Object(crate::types::JsObject {
                                id: gen_id,
                            }));
                        }
                        let closure_strict = closure.borrow().strict;
                        let func_env = Environment::new(Some(closure));
                        // Bind parameters
                        for (i, param) in params.iter().enumerate() {
                            if let Pattern::Rest(inner) = param {
                                let rest: Vec<JsValue> = args.get(i..).unwrap_or(&[]).to_vec();
                                let rest_arr = self.create_array(rest);
                                let _ =
                                    self.bind_pattern(inner, rest_arr, BindingKind::Var, &func_env);
                                break;
                            }
                            let val = args.get(i).cloned().unwrap_or(JsValue::Undefined);
                            let _ = self.bind_pattern(param, val, BindingKind::Var, &func_env);
                        }
                        // Arrow functions inherit `this` and `arguments` from closure
                        if !is_arrow {
                            let effective_this = if !is_strict
                                && !closure_strict
                                && matches!(_this_val, JsValue::Undefined | JsValue::Null)
                            {
                                self.global_env
                                    .borrow()
                                    .get("this")
                                    .unwrap_or(_this_val.clone())
                            } else {
                                _this_val.clone()
                            };
                            func_env.borrow_mut().bindings.insert(
                                "this".to_string(),
                                Binding {
                                    value: effective_this,
                                    kind: BindingKind::Const,
                                    initialized: true,
                                },
                            );
                            let is_simple =
                                params.iter().all(|p| matches!(p, Pattern::Identifier(_)));
                            let env_strict = func_env.borrow().strict;
                            let use_mapped = is_simple && !is_strict && !env_strict;
                            let param_names: Vec<String> = if use_mapped {
                                params
                                    .iter()
                                    .filter_map(|p| {
                                        if let Pattern::Identifier(name) = p {
                                            Some(name.clone())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect()
                            } else {
                                Vec::new()
                            };
                            let mapped_env = if use_mapped { Some(&func_env) } else { None };
                            let arguments_obj = self.create_arguments_object(
                                args,
                                func_val.clone(),
                                is_strict,
                                mapped_env,
                                &param_names,
                            );
                            func_env.borrow_mut().declare("arguments", BindingKind::Var);
                            let _ = func_env.borrow_mut().set("arguments", arguments_obj);
                        }
                        let result = self.exec_statements(&body, &func_env);
                        match result {
                            Completion::Return(v) | Completion::Normal(v) => Completion::Normal(v),
                            Completion::Yield(_) => Completion::Normal(JsValue::Undefined),
                            other => other,
                        }
                    }
                };
            }
        }
        let err = self.create_type_error("is not a function");
        Completion::Throw(err)
    }

    fn eval_spread_args(
        &mut self,
        args: &[Expression],
        env: &EnvRef,
    ) -> Result<Vec<JsValue>, JsValue> {
        let mut evaluated = Vec::new();
        for arg in args {
            if let Expression::Spread(inner) = arg {
                let val = match self.eval_expr(inner, env) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::Undefined,
                };
                let items = self.iterate_to_vec(&val)?;
                evaluated.extend(items);
            } else {
                let val = match self.eval_expr(arg, env) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::Undefined,
                };
                evaluated.push(val);
            }
        }
        Ok(evaluated)
    }

    fn eval_new(&mut self, callee: &Expression, args: &[Expression], env: &EnvRef) -> Completion {
        let callee_val = match self.eval_expr(callee, env) {
            Completion::Normal(v) => v,
            other => return other,
        };
        let evaluated_args = match self.eval_spread_args(args, env) {
            Ok(args) => args,
            Err(e) => return Completion::Throw(e),
        };
        // Proxy construct trap
        if let JsValue::Object(ref co) = callee_val
            && self.get_proxy_info(co.id).is_some()
        {
            let target_val = self.get_proxy_target_val(co.id);
            let args_array = self.create_array(evaluated_args.clone());
            let new_target = callee_val.clone();
            match self.invoke_proxy_trap(
                co.id,
                "construct",
                vec![target_val.clone(), args_array, new_target],
            ) {
                Ok(Some(v)) => {
                    if matches!(v, JsValue::Object(_)) {
                        return Completion::Normal(v);
                    }
                    return Completion::Throw(
                        self.create_type_error("'construct' on proxy: trap returned non-Object"),
                    );
                }
                Ok(None) => {
                    // No trap, forward to target constructor
                    // Temporarily replace callee with target for normal eval_new path
                    let prev_new_target = self.new_target.take();
                    self.new_target = Some(callee_val.clone());
                    let new_obj = self.create_object();
                    if let JsValue::Object(ref t) = target_val
                        && let Some(func_obj) = self.get_object(t.id)
                    {
                        let proto = func_obj.borrow().get_property_value("prototype");
                        if let Some(JsValue::Object(proto_obj)) = proto
                            && let Some(proto_rc) = self.get_object(proto_obj.id)
                        {
                            new_obj.borrow_mut().prototype = Some(proto_rc);
                        }
                    }
                    let new_obj_id = new_obj.borrow().id.unwrap();
                    let this_val = JsValue::Object(crate::types::JsObject { id: new_obj_id });
                    let result = self.call_function(&target_val, &this_val, &evaluated_args);
                    self.new_target = prev_new_target;
                    return match result {
                        Completion::Normal(v) if matches!(v, JsValue::Object(_)) => {
                            Completion::Normal(v)
                        }
                        Completion::Normal(_) => Completion::Normal(this_val),
                        other => other,
                    };
                }
                Err(e) => return Completion::Throw(e),
            }
        }
        // Create new object for 'this'
        let new_obj = self.create_object();
        // Set prototype from constructor.prototype if available
        let (private_field_defs, public_field_defs) = if let JsValue::Object(o) = &callee_val
            && let Some(func_obj) = self.get_object(o.id)
        {
            let proto = func_obj.borrow().get_property_value("prototype");
            if let Some(JsValue::Object(proto_obj)) = proto
                && let Some(proto_rc) = self.get_object(proto_obj.id)
            {
                new_obj.borrow_mut().prototype = Some(proto_rc);
            }
            // Store constructor reference
            new_obj
                .borrow_mut()
                .insert_builtin("constructor".to_string(), callee_val.clone());
            let borrowed = func_obj.borrow();
            (
                borrowed.class_private_field_defs.clone(),
                borrowed.class_public_field_defs.clone(),
            )
        } else {
            (Vec::new(), Vec::new())
        };
        // Initialize private fields on the new instance
        let new_obj_id = new_obj.borrow().id.unwrap();
        let this_val = JsValue::Object(crate::types::JsObject { id: new_obj_id });
        // Create a temporary env for evaluating initializers with `this` bound
        let init_env = Environment::new(Some(env.clone()));
        init_env.borrow_mut().declare("this", BindingKind::Const);
        let _ = init_env.borrow_mut().set("this", this_val.clone());
        for def in &private_field_defs {
            match def {
                PrivateFieldDef::Field { name, initializer } => {
                    let val = if let Some(init) = initializer {
                        match self.eval_expr(init, &init_env) {
                            Completion::Normal(v) => v,
                            other => return other,
                        }
                    } else {
                        JsValue::Undefined
                    };
                    if let Some(obj) = self.get_object(new_obj_id) {
                        obj.borrow_mut()
                            .private_fields
                            .insert(name.clone(), PrivateElement::Field(val));
                    }
                }
                PrivateFieldDef::Method { name, value } => {
                    if let Some(obj) = self.get_object(new_obj_id) {
                        obj.borrow_mut()
                            .private_fields
                            .insert(name.clone(), PrivateElement::Method(value.clone()));
                    }
                }
                PrivateFieldDef::Accessor { name, get, set } => {
                    if let Some(obj) = self.get_object(new_obj_id) {
                        obj.borrow_mut().private_fields.insert(
                            name.clone(),
                            PrivateElement::Accessor {
                                get: get.clone(),
                                set: set.clone(),
                            },
                        );
                    }
                }
            }
        }
        for (key, initializer) in &public_field_defs {
            let val = if let Some(init) = initializer {
                match self.eval_expr(init, &init_env) {
                    Completion::Normal(v) => v,
                    other => return other,
                }
            } else {
                JsValue::Undefined
            };
            if let Some(obj) = self.get_object(new_obj_id) {
                obj.borrow_mut().insert_value(key.clone(), val);
            }
        }
        let prev_new_target = self.new_target.take();
        self.new_target = Some(callee_val.clone());
        let result = self.call_function(&callee_val, &this_val, &evaluated_args);
        self.new_target = prev_new_target;
        match result {
            Completion::Normal(v) => {
                if matches!(v, JsValue::Object(_)) {
                    Completion::Normal(v)
                } else {
                    Completion::Normal(this_val)
                }
            }
            other => other,
        }
    }

    fn get_proxy_info(&self, obj_id: u64) -> Option<(bool, Option<u64>, Option<u64>)> {
        if let Some(obj) = self.get_object(obj_id) {
            let b = obj.borrow();
            if b.is_proxy() || b.proxy_revoked {
                let target_id = b.proxy_target.as_ref().and_then(|t| t.borrow().id);
                let handler_id = b.proxy_handler.as_ref().and_then(|h| h.borrow().id);
                return Some((b.proxy_revoked, target_id, handler_id));
            }
        }
        None
    }

    pub(crate) fn invoke_proxy_trap(
        &mut self,
        proxy_id: u64,
        trap_name: &str,
        args: Vec<JsValue>,
    ) -> Result<Option<JsValue>, JsValue> {
        let info = self.get_proxy_info(proxy_id);
        match info {
            Some((true, _, _)) => Err(self.create_type_error(&format!(
                "Cannot perform '{}' on a proxy that has been revoked",
                trap_name
            ))),
            Some((false, Some(_target_id), Some(handler_id))) => {
                let trap_val = if let Some(handler) = self.get_object(handler_id) {
                    handler.borrow().get_property(trap_name)
                } else {
                    JsValue::Undefined
                };
                if matches!(trap_val, JsValue::Undefined | JsValue::Null) {
                    return Ok(None); // No trap, fall through to target
                }
                let handler_val = JsValue::Object(crate::types::JsObject { id: handler_id });
                match self.call_function(&trap_val, &handler_val, &args) {
                    Completion::Normal(v) => Ok(Some(v)),
                    Completion::Throw(e) => Err(e),
                    _ => Ok(Some(JsValue::Undefined)),
                }
            }
            Some((false, _, _)) => Err(self.create_type_error(&format!(
                "Cannot perform '{}' on a proxy that has been revoked",
                trap_name
            ))),
            None => Ok(None),
        }
    }

    pub(crate) fn get_proxy_target_val(&self, proxy_id: u64) -> JsValue {
        if let Some(obj) = self.get_object(proxy_id) {
            let b = obj.borrow();
            if let Some(ref target) = b.proxy_target
                && let Some(tid) = target.borrow().id {
                    return JsValue::Object(crate::types::JsObject { id: tid });
                }
        }
        JsValue::Undefined
    }

    pub(crate) fn get_object_property(
        &mut self,
        obj_id: u64,
        key: &str,
        this_val: &JsValue,
    ) -> Completion {
        // Check if object is a proxy
        if self.get_proxy_info(obj_id).is_some() {
            let target_val = self.get_proxy_target_val(obj_id);
            let key_val = JsValue::String(JsString::from_str(key));
            let receiver = this_val.clone();
            match self.invoke_proxy_trap(obj_id, "get", vec![target_val.clone(), key_val, receiver])
            {
                Ok(Some(v)) => return Completion::Normal(v),
                Ok(None) => {
                    // No trap, fall through to target
                    if let JsValue::Object(ref t) = target_val {
                        return self.get_object_property(t.id, key, this_val);
                    }
                    return Completion::Normal(JsValue::Undefined);
                }
                Err(e) => return Completion::Throw(e),
            }
        }

        let desc = if let Some(obj) = self.get_object(obj_id) {
            obj.borrow().get_property_descriptor(key)
        } else {
            None
        };
        match desc {
            Some(ref d) if d.get.is_some() => {
                let getter = d.get.clone().unwrap();
                self.call_function(&getter, this_val, &[])
            }
            Some(ref d) => Completion::Normal(d.value.clone().unwrap_or(JsValue::Undefined)),
            None => Completion::Normal(JsValue::Undefined),
        }
    }

    fn eval_member(&mut self, obj: &Expression, prop: &MemberProperty, env: &EnvRef) -> Completion {
        let obj_val = match self.eval_expr(obj, env) {
            Completion::Normal(v) => v,
            other => return other,
        };
        if let MemberProperty::Private(name) = prop {
            return match &obj_val {
                JsValue::Object(o) => {
                    if let Some(obj) = self.get_object(o.id) {
                        let elem = obj.borrow().private_fields.get(name).cloned();
                        match elem {
                            Some(PrivateElement::Field(v)) | Some(PrivateElement::Method(v)) => {
                                Completion::Normal(v)
                            }
                            Some(PrivateElement::Accessor { get, .. }) => {
                                if let Some(getter) = get {
                                    self.call_function(&getter, &obj_val, &[])
                                } else {
                                    Completion::Throw(self.create_type_error(&format!(
                                        "Cannot read private member #{name} which has no getter"
                                    )))
                                }
                            }
                            None => Completion::Throw(self.create_type_error(&format!(
                                "Cannot read private member #{name} from an object whose class did not declare it"
                            ))),
                        }
                    } else {
                        Completion::Normal(JsValue::Undefined)
                    }
                }
                _ => Completion::Throw(self.create_type_error(&format!(
                    "Cannot read private member #{name} from a non-object"
                ))),
            };
        }
        let key = match prop {
            MemberProperty::Dot(name) => name.clone(),
            MemberProperty::Computed(expr) => {
                let v = match self.eval_expr(expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                to_js_string(&v)
            }
            MemberProperty::Private(_) => unreachable!(),
        };
        match &obj_val {
            JsValue::Object(o) => self.get_object_property(o.id, &key, &obj_val.clone()),
            JsValue::String(s) => {
                if key == "length" {
                    Completion::Normal(JsValue::Number(s.len() as f64))
                } else if let Ok(idx) = key.parse::<usize>() {
                    let ch = s.to_rust_string().chars().nth(idx);
                    match ch {
                        Some(c) => {
                            Completion::Normal(JsValue::String(JsString::from_str(&c.to_string())))
                        }
                        None => Completion::Normal(JsValue::Undefined),
                    }
                } else if let Some(ref sp) = self.string_prototype {
                    Completion::Normal(sp.borrow().get_property(&key))
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            JsValue::Symbol(_) => {
                if let Some(ref sp) = self.symbol_prototype {
                    let desc = sp.borrow().get_property_descriptor(&key);
                    match desc {
                        Some(ref d) if d.get.is_some() => {
                            let getter = d.get.clone().unwrap();
                            self.call_function(&getter, &obj_val, &[])
                        }
                        Some(ref d) => {
                            Completion::Normal(d.value.clone().unwrap_or(JsValue::Undefined))
                        }
                        None => Completion::Normal(JsValue::Undefined),
                    }
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            JsValue::Number(_) => {
                if let Some(ref np) = self.number_prototype {
                    Completion::Normal(np.borrow().get_property(&key))
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            JsValue::Boolean(_) => {
                if let Some(ref bp) = self.boolean_prototype {
                    Completion::Normal(bp.borrow().get_property(&key))
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            JsValue::Undefined | JsValue::Null => {
                let err = self.create_type_error(&format!(
                    "Cannot read properties of {obj_val} (reading '{key}')"
                ));
                Completion::Throw(err)
            }
            _ => Completion::Normal(JsValue::Undefined),
        }
    }

    fn eval_array_literal(&mut self, elements: &[Option<Expression>], env: &EnvRef) -> Completion {
        let mut values = Vec::new();
        for elem in elements {
            match elem {
                Some(Expression::Spread(inner)) => {
                    let val = match self.eval_expr(inner, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    match self.iterate_to_vec(&val) {
                        Ok(items) => values.extend(items),
                        Err(e) => return Completion::Throw(e),
                    }
                }
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
        Completion::Normal(self.create_array(values))
    }

    pub(crate) fn eval_class(
        &mut self,
        name: &str,
        super_class: &Option<Box<Expression>>,
        body: &[ClassElement],
        env: &EnvRef,
    ) -> Completion {
        // Find constructor method
        let ctor_method = body.iter().find_map(|elem| {
            if let ClassElement::Method(m) = elem
                && m.kind == ClassMethodKind::Constructor
            {
                return Some(m);
            }
            None
        });

        // Evaluate super class if present
        let super_val = if let Some(sc) = super_class {
            match self.eval_expr(sc, env) {
                Completion::Normal(v) => Some(v),
                other => return other,
            }
        } else {
            None
        };

        // Create class environment with __super__ binding
        let class_env = Environment::new(Some(env.clone()));
        if let Some(ref sv) = super_val {
            class_env
                .borrow_mut()
                .declare("__super__", BindingKind::Const);
            let _ = class_env.borrow_mut().set("__super__", sv.clone());
        }

        // Create constructor function (classes are always strict mode)
        let ctor_func = if let Some(cm) = ctor_method {
            JsFunction::User {
                name: Some(name.to_string()),
                params: cm.value.params.clone(),
                body: cm.value.body.clone(),
                closure: class_env.clone(),
                is_arrow: false,
                is_strict: true,
                is_generator: false,
                is_async: false,
            }
        } else if super_val.is_some() {
            JsFunction::User {
                name: Some(name.to_string()),
                params: vec![],
                body: vec![],
                closure: class_env.clone(),
                is_arrow: false,
                is_strict: true,
                is_generator: false,
                is_async: false,
            }
        } else {
            JsFunction::User {
                name: Some(name.to_string()),
                params: vec![],
                body: vec![],
                closure: class_env.clone(),
                is_arrow: false,
                is_strict: true,
                is_generator: false,
                is_async: false,
            }
        };

        let ctor_val = self.create_function(ctor_func);

        // Get the prototype object that was auto-created by create_function
        let proto_obj = if let JsValue::Object(ref o) = ctor_val {
            if let Some(func_obj) = self.get_object(o.id) {
                let proto_val = func_obj.borrow().get_property("prototype");
                if let JsValue::Object(ref p) = proto_val {
                    self.get_object(p.id)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        // Set up inheritance
        if let Some(ref sv) = super_val
            && let JsValue::Object(super_o) = sv
            && let Some(super_obj) = self.get_object(super_o.id)
        {
            let super_proto_val = super_obj.borrow().get_property("prototype");
            if let JsValue::Object(ref sp) = super_proto_val
                && let Some(super_proto) = self.get_object(sp.id)
                && let Some(ref proto) = proto_obj
            {
                proto.borrow_mut().prototype = Some(super_proto);
            }
        }

        // Add methods and properties to prototype/constructor
        for elem in body {
            match elem {
                ClassElement::Method(m) => {
                    if m.kind == ClassMethodKind::Constructor {
                        continue;
                    }
                    let key = match &m.key {
                        PropertyKey::Identifier(s) | PropertyKey::String(s) => s.clone(),
                        PropertyKey::Number(n) => to_js_string(&JsValue::Number(*n)),
                        PropertyKey::Computed(expr) => match self.eval_expr(expr, env) {
                            Completion::Normal(v) => to_js_string(&v),
                            other => return other,
                        },
                        PropertyKey::Private(name) => {
                            let method_func = JsFunction::User {
                                name: Some(format!("#{name}")),
                                params: m.value.params.clone(),
                                body: m.value.body.clone(),
                                closure: class_env.clone(),
                                is_arrow: false,
                                is_strict: true,
                                is_generator: m.value.is_generator,
                                is_async: m.value.is_async,
                            };
                            let method_val = self.create_function(method_func);

                            if m.is_static {
                                if let JsValue::Object(ref o) = ctor_val
                                    && let Some(func_obj) = self.get_object(o.id) {
                                        match m.kind {
                                            ClassMethodKind::Get => {
                                                let existing = func_obj
                                                    .borrow()
                                                    .private_fields
                                                    .get(name)
                                                    .cloned();
                                                let elem = if let Some(PrivateElement::Accessor {
                                                    get: _,
                                                    set,
                                                }) = existing
                                                {
                                                    PrivateElement::Accessor {
                                                        get: Some(method_val),
                                                        set,
                                                    }
                                                } else {
                                                    PrivateElement::Accessor {
                                                        get: Some(method_val),
                                                        set: None,
                                                    }
                                                };
                                                func_obj
                                                    .borrow_mut()
                                                    .private_fields
                                                    .insert(name.clone(), elem);
                                            }
                                            ClassMethodKind::Set => {
                                                let existing = func_obj
                                                    .borrow()
                                                    .private_fields
                                                    .get(name)
                                                    .cloned();
                                                let elem = if let Some(PrivateElement::Accessor {
                                                    get,
                                                    set: _,
                                                }) = existing
                                                {
                                                    PrivateElement::Accessor {
                                                        get,
                                                        set: Some(method_val),
                                                    }
                                                } else {
                                                    PrivateElement::Accessor {
                                                        get: None,
                                                        set: Some(method_val),
                                                    }
                                                };
                                                func_obj
                                                    .borrow_mut()
                                                    .private_fields
                                                    .insert(name.clone(), elem);
                                            }
                                            _ => {
                                                func_obj.borrow_mut().private_fields.insert(
                                                    name.clone(),
                                                    PrivateElement::Method(method_val),
                                                );
                                            }
                                        }
                                    }
                            } else if let JsValue::Object(ref o) = ctor_val
                            && let Some(func_obj) = self.get_object(o.id) {
                                match m.kind {
                                    ClassMethodKind::Get => {
                                        let mut b = func_obj.borrow_mut();
                                        let mut found = false;
                                        for def in b.class_private_field_defs.iter_mut() {
                                            if let PrivateFieldDef::Accessor {
                                                name: n,
                                                get: g,
                                                ..
                                            } = def
                                                && n == name {
                                                    *g = Some(method_val.clone());
                                                    found = true;
                                                    break;
                                                }
                                        }
                                        if !found {
                                            b.class_private_field_defs.push(
                                                PrivateFieldDef::Accessor {
                                                    name: name.clone(),
                                                    get: Some(method_val),
                                                    set: None,
                                                },
                                            );
                                        }
                                    }
                                    ClassMethodKind::Set => {
                                        let mut b = func_obj.borrow_mut();
                                        let mut found = false;
                                        for def in b.class_private_field_defs.iter_mut() {
                                            if let PrivateFieldDef::Accessor {
                                                name: n,
                                                set: s,
                                                ..
                                            } = def
                                                && n == name {
                                                    *s = Some(method_val.clone());
                                                    found = true;
                                                    break;
                                                }
                                        }
                                        if !found {
                                            b.class_private_field_defs.push(
                                                PrivateFieldDef::Accessor {
                                                    name: name.clone(),
                                                    get: None,
                                                    set: Some(method_val),
                                                },
                                            );
                                        }
                                    }
                                    _ => {
                                        func_obj
                                            .borrow_mut()
                                            .class_private_field_defs
                                            .push(PrivateFieldDef::Method {
                                                name: name.clone(),
                                                value: method_val,
                                            });
                                    }
                                }
                            }
                            continue;
                        }
                    };
                    let method_func = JsFunction::User {
                        name: Some(key.clone()),
                        params: m.value.params.clone(),
                        body: m.value.body.clone(),
                        closure: class_env.clone(),
                        is_arrow: false,
                        is_strict: true,
                        is_generator: m.value.is_generator,
                        is_async: m.value.is_async,
                    };
                    let method_val = self.create_function(method_func);

                    let target = if m.is_static {
                        if let JsValue::Object(ref o) = ctor_val {
                            self.get_object(o.id)
                        } else {
                            None
                        }
                    } else {
                        proto_obj.clone()
                    };
                    if let Some(ref t) = target {
                        match m.kind {
                            ClassMethodKind::Get => {
                                let mut desc = t.borrow().properties.get(&key).cloned().unwrap_or(
                                    PropertyDescriptor {
                                        value: None,
                                        writable: None,
                                        get: None,
                                        set: None,
                                        enumerable: Some(false),
                                        configurable: Some(true),
                                    },
                                );
                                desc.get = Some(method_val);
                                desc.value = None;
                                desc.writable = None;
                                t.borrow_mut().insert_property(key, desc);
                            }
                            ClassMethodKind::Set => {
                                let mut desc = t.borrow().properties.get(&key).cloned().unwrap_or(
                                    PropertyDescriptor {
                                        value: None,
                                        writable: None,
                                        get: None,
                                        set: None,
                                        enumerable: Some(false),
                                        configurable: Some(true),
                                    },
                                );
                                desc.set = Some(method_val);
                                desc.value = None;
                                desc.writable = None;
                                t.borrow_mut().insert_property(key, desc);
                            }
                            _ => {
                                t.borrow_mut().insert_builtin(key, method_val);
                            }
                        }
                    }
                }
                ClassElement::Property(p) => {
                    // Check if this is a private field
                    if let PropertyKey::Private(name) = &p.key {
                        if !p.is_static {
                            // Store instance private field definition
                            if let JsValue::Object(ref o) = ctor_val
                                && let Some(func_obj) = self.get_object(o.id)
                            {
                                func_obj.borrow_mut().class_private_field_defs.push(
                                    PrivateFieldDef::Field {
                                        name: name.clone(),
                                        initializer: p.value.clone(),
                                    },
                                );
                            }
                        } else {
                            // Static private field - evaluate now and store on constructor
                            let val = if let Some(ref expr) = p.value {
                                match self.eval_expr(expr, env) {
                                    Completion::Normal(v) => v,
                                    other => return other,
                                }
                            } else {
                                JsValue::Undefined
                            };
                            if let JsValue::Object(ref o) = ctor_val
                                && let Some(func_obj) = self.get_object(o.id)
                            {
                                func_obj
                                    .borrow_mut()
                                    .private_fields
                                    .insert(name.clone(), PrivateElement::Field(val));
                            }
                        }
                        continue;
                    }
                    // Static properties are set on the constructor
                    if p.is_static {
                        let key = match &p.key {
                            PropertyKey::Identifier(s) | PropertyKey::String(s) => s.clone(),
                            PropertyKey::Number(n) => to_js_string(&JsValue::Number(*n)),
                            PropertyKey::Computed(expr) => match self.eval_expr(expr, env) {
                                Completion::Normal(v) => to_js_string(&v),
                                other => return other,
                            },
                            PropertyKey::Private(_) => unreachable!(),
                        };
                        let val = if let Some(ref expr) = p.value {
                            match self.eval_expr(expr, env) {
                                Completion::Normal(v) => v,
                                other => return other,
                            }
                        } else {
                            JsValue::Undefined
                        };
                        if let JsValue::Object(ref o) = ctor_val
                            && let Some(func_obj) = self.get_object(o.id)
                        {
                            func_obj.borrow_mut().insert_value(key, val);
                        }
                    } else {
                        let key = match &p.key {
                            PropertyKey::Identifier(s) | PropertyKey::String(s) => s.clone(),
                            PropertyKey::Number(n) => to_js_string(&JsValue::Number(*n)),
                            PropertyKey::Computed(expr) => match self.eval_expr(expr, env) {
                                Completion::Normal(v) => to_js_string(&v),
                                other => return other,
                            },
                            PropertyKey::Private(_) => unreachable!(),
                        };
                        if let JsValue::Object(ref o) = ctor_val
                            && let Some(func_obj) = self.get_object(o.id)
                        {
                            func_obj
                                .borrow_mut()
                                .class_public_field_defs
                                .push((key, p.value.clone()));
                        }
                    }
                }
                ClassElement::StaticBlock(body) => {
                    let block_env = Environment::new(Some(env.clone()));
                    block_env.borrow_mut().bindings.insert(
                        "this".to_string(),
                        Binding {
                            value: ctor_val.clone(),
                            kind: BindingKind::Const,
                            initialized: true,
                        },
                    );
                    match self.exec_statements(body, &block_env) {
                        Completion::Normal(_) => {}
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => {}
                    }
                }
            }
        }

        Completion::Normal(ctor_val)
    }

    fn eval_object_literal(&mut self, props: &[Property], env: &EnvRef) -> Completion {
        let mut obj_data = JsObjectData::new();
        obj_data.prototype = self.object_prototype.clone();
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
                PropertyKey::Private(_) => {
                    return Completion::Throw(
                        self.create_type_error("Private names are not valid in object literals"),
                    );
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
                if let JsValue::Object(ref o) = spread_val
                    && let Some(src) = self.get_object(o.id)
                {
                    let src_ref = src.borrow();
                    for k in &src_ref.property_order {
                        if let Some(v) = src_ref.properties.get(k) {
                            obj_data.insert_property(k.clone(), v.clone());
                        }
                    }
                }
                continue;
            }
            match prop.kind {
                PropertyKind::Get => {
                    let mut desc =
                        obj_data
                            .properties
                            .get(&key)
                            .cloned()
                            .unwrap_or(PropertyDescriptor {
                                value: None,
                                writable: None,
                                get: None,
                                set: None,
                                enumerable: Some(true),
                                configurable: Some(true),
                            });
                    desc.get = Some(value);
                    desc.value = None;
                    desc.writable = None;
                    obj_data.insert_property(key, desc);
                }
                PropertyKind::Set => {
                    let mut desc =
                        obj_data
                            .properties
                            .get(&key)
                            .cloned()
                            .unwrap_or(PropertyDescriptor {
                                value: None,
                                writable: None,
                                get: None,
                                set: None,
                                enumerable: Some(true),
                                configurable: Some(true),
                            });
                    desc.set = Some(value);
                    desc.value = None;
                    desc.writable = None;
                    obj_data.insert_property(key, desc);
                }
                _ => {
                    obj_data.insert_value(key, value);
                }
            }
        }
        let obj = Rc::new(RefCell::new(obj_data));
        let id = self.allocate_object_slot(obj);
        Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
    }

    fn call_async_function(
        &mut self,
        params: &[Pattern],
        body: &[Statement],
        closure: EnvRef,
        is_arrow: bool,
        is_strict: bool,
        this_val: &JsValue,
        args: &[JsValue],
        func_val: &JsValue,
    ) -> Completion {
        let promise = self.create_promise_object();
        let promise_id = if let JsValue::Object(ref o) = promise {
            o.id
        } else {
            0
        };
        let (resolve_fn, reject_fn) = self.create_resolving_functions(promise_id);

        let closure_strict = closure.borrow().strict;
        let func_env = Environment::new(Some(closure));
        for (i, param) in params.iter().enumerate() {
            if let Pattern::Rest(inner) = param {
                let rest: Vec<JsValue> = args.get(i..).unwrap_or(&[]).to_vec();
                let rest_arr = self.create_array(rest);
                let _ = self.bind_pattern(inner, rest_arr, BindingKind::Var, &func_env);
                break;
            }
            let val = args.get(i).cloned().unwrap_or(JsValue::Undefined);
            let _ = self.bind_pattern(param, val, BindingKind::Var, &func_env);
        }
        if !is_arrow {
            let effective_this = if !is_strict
                && !closure_strict
                && matches!(this_val, JsValue::Undefined | JsValue::Null)
            {
                self.global_env
                    .borrow()
                    .get("this")
                    .unwrap_or(this_val.clone())
            } else {
                this_val.clone()
            };
            func_env.borrow_mut().bindings.insert(
                "this".to_string(),
                Binding {
                    value: effective_this,
                    kind: BindingKind::Const,
                    initialized: true,
                },
            );
            let is_simple = params.iter().all(|p| matches!(p, Pattern::Identifier(_)));
            let env_strict = func_env.borrow().strict;
            let use_mapped = is_simple && !is_strict && !env_strict;
            let param_names: Vec<String> = if use_mapped {
                params
                    .iter()
                    .filter_map(|p| {
                        if let Pattern::Identifier(name) = p {
                            Some(name.clone())
                        } else {
                            None
                        }
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let mapped_env = if use_mapped { Some(&func_env) } else { None };
            let arguments_obj = self.create_arguments_object(
                args,
                func_val.clone(),
                is_strict,
                mapped_env,
                &param_names,
            );
            func_env.borrow_mut().declare("arguments", BindingKind::Var);
            let _ = func_env.borrow_mut().set("arguments", arguments_obj);
        }

        let result = self.exec_statements(body, &func_env);
        match result {
            Completion::Return(v) | Completion::Normal(v) => {
                let _ = self.call_function(&resolve_fn, &JsValue::Undefined, &[v]);
            }
            Completion::Throw(e) => {
                let _ = self.call_function(&reject_fn, &JsValue::Undefined, &[e]);
            }
            _ => {}
        }
        self.drain_microtasks();
        Completion::Normal(promise)
    }

    pub(crate) fn await_value(&mut self, val: &JsValue) -> Completion {
        if self.is_promise(val) {
            let promise_id = if let JsValue::Object(o) = val {
                o.id
            } else {
                0
            };
            self.drain_microtasks();
            match self.get_promise_state(promise_id) {
                Some(PromiseState::Fulfilled(v)) => Completion::Normal(v),
                Some(PromiseState::Rejected(r)) => Completion::Throw(r),
                Some(PromiseState::Pending) => {
                    for _ in 0..1000 {
                        if self.microtask_queue.is_empty() {
                            break;
                        }
                        self.drain_microtasks();
                        match self.get_promise_state(promise_id) {
                            Some(PromiseState::Fulfilled(v)) => return Completion::Normal(v),
                            Some(PromiseState::Rejected(r)) => return Completion::Throw(r),
                            _ => {}
                        }
                    }
                    Completion::Normal(JsValue::Undefined)
                }
                None => Completion::Normal(val.clone()),
            }
        } else if let JsValue::Object(o) = val {
            // Check for thenable
            if let Some(obj) = self.get_object(o.id) {
                let then_val = obj.borrow().get_property("then");
                if self.is_callable(&then_val) {
                    let p = self.promise_resolve_value(val);
                    return self.await_value(&p);
                }
            }
            Completion::Normal(val.clone())
        } else {
            Completion::Normal(val.clone())
        }
    }
}
