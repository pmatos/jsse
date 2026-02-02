use super::*;

impl Interpreter {
    pub(crate) fn exec_statements(&mut self, stmts: &[Statement], env: &EnvRef) -> Completion {
        if Self::is_strict_mode_body(stmts) {
            env.borrow_mut().strict = true;
        }
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
                        is_arrow: false,
                        is_strict: Self::is_strict_mode_body(&f.body),
                        is_generator: f.is_generator,
                        is_async: f.is_async,
                    };
                    let val = self.create_function(func);
                    let _ = env.borrow_mut().set(&f.name, val);
                }
                _ => {}
            }
        }

        let mut result = JsValue::Undefined;
        for stmt in stmts {
            self.maybe_gc();
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

    pub(crate) fn exec_statement(&mut self, stmt: &Statement, env: &EnvRef) -> Completion {
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
            Statement::ForOf(fo) => self.exec_for_of(fo, env),
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
            Statement::With(expr, body) => {
                let val = match self.eval_expr(expr, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let obj_val = match self.to_object(&val) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if let JsValue::Object(obj_ref) = &obj_val {
                    if let Some(obj_data) = self.get_object(obj_ref.id) {
                        let unscopables = {
                            let obj = obj_data.borrow();
                            let v = obj.get_property("Symbol(Symbol.unscopables)");
                            if matches!(v, JsValue::Undefined) {
                                obj.get_property("[Symbol.unscopables]")
                            } else {
                                v
                            }
                        };
                        let unscopables_data = if let JsValue::Object(u) = &unscopables {
                            self.get_object(u.id)
                        } else {
                            None
                        };
                        let with_env = Rc::new(RefCell::new(Environment {
                            bindings: HashMap::new(),
                            parent: Some(env.clone()),
                            strict: env.borrow().strict,
                            with_object: Some(WithObject {
                                object: obj_data,
                                unscopables: unscopables_data,
                            }),
                        }));
                        self.exec_statement(body, &with_env)
                    } else {
                        Completion::Normal(JsValue::Undefined)
                    }
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            Statement::Debugger => Completion::Normal(JsValue::Undefined),
            Statement::FunctionDeclaration(_) => Completion::Normal(JsValue::Undefined), // hoisted
            Statement::ClassDeclaration(cd) => {
                let class_val = self.eval_class(&cd.name, &cd.super_class, &cd.body, env);
                match class_val {
                    Completion::Normal(val) => {
                        env.borrow_mut().declare(&cd.name, BindingKind::Let);
                        let _ = env.borrow_mut().set(&cd.name, val);
                        Completion::Normal(JsValue::Undefined)
                    }
                    other => other,
                }
            }
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
            if d.init.is_none()
                && decl.kind == VarKind::Var
                && let Pattern::Identifier(ref name) = d.pattern
            {
                if !env.borrow().bindings.contains_key(name) {
                    env.borrow_mut().declare(name, kind);
                }
                continue;
            }
            let val = if let Some(init) = &d.init {
                match self.eval_expr(init, env) {
                    Completion::Normal(v) => v,
                    other => return other,
                }
            } else {
                JsValue::Undefined
            };
            if let Pattern::Identifier(ref name) = d.pattern {
                self.set_function_name(&val, name);
            }
            if let Err(e) = self.bind_pattern(&d.pattern, val, kind, env) {
                return Completion::Throw(e);
            }
        }
        Completion::Normal(JsValue::Undefined)
    }

    pub(crate) fn bind_pattern(
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
                if let Pattern::Identifier(ref name) = **inner {
                    self.set_function_name(&v, name);
                }
                self.bind_pattern(inner, v, kind, env)
            }
            Pattern::Array(elements) => {
                let iterator = self.get_iterator(&val)?;
                for elem in elements {
                    if let Some(elem) = elem {
                        match elem {
                            ArrayPatternElement::Pattern(p) => {
                                let item = match self.iterator_step(&iterator) {
                                    Ok(Some(result)) => self.iterator_value(&result),
                                    Ok(None) => JsValue::Undefined,
                                    Err(e) => return Err(e),
                                };
                                self.bind_pattern(p, item, kind, env)?;
                            }
                            ArrayPatternElement::Rest(p) => {
                                let mut rest = Vec::new();
                                loop {
                                    match self.iterator_step(&iterator) {
                                        Ok(Some(result)) => {
                                            rest.push(self.iterator_value(&result));
                                        }
                                        Ok(None) => break,
                                        Err(e) => return Err(e),
                                    }
                                }
                                let arr = self.create_array(rest);
                                self.bind_pattern(p, arr, kind, env)?;
                                break;
                            }
                        }
                    } else {
                        // Elision — skip one iterator step
                        let _ = self.iterator_step(&iterator);
                    }
                }
                Ok(())
            }
            Pattern::Object(props) => {
                let mut excluded_keys = Vec::new();
                for prop in props {
                    match prop {
                        ObjectPatternProperty::Shorthand(name) => {
                            excluded_keys.push(name.clone());
                            let v = if let JsValue::Object(o) = &val {
                                if let Some(obj) = self.get_object(o.id) {
                                    obj.borrow().get_property(name)
                                } else {
                                    JsValue::Undefined
                                }
                            } else {
                                JsValue::Undefined
                            };
                            if kind != BindingKind::Var || !env.borrow().bindings.contains_key(name)
                            {
                                env.borrow_mut().declare(name, kind);
                            }
                            env.borrow_mut().set(name, v)?;
                        }
                        ObjectPatternProperty::KeyValue(key, pat) => {
                            let key_str = match key {
                                PropertyKey::Identifier(s) | PropertyKey::String(s) => s.clone(),
                                PropertyKey::Number(n) => {
                                    crate::interpreter::to_js_string(&JsValue::Number(*n))
                                }
                                PropertyKey::Computed(expr) => match self.eval_expr(expr, env) {
                                    Completion::Normal(v) => self.to_property_key(&v)?,
                                    Completion::Throw(e) => return Err(e),
                                    _ => String::new(),
                                },
                                PropertyKey::Private(_) => {
                                    return Err(self.create_type_error(
                                        "Private names are not valid in object patterns",
                                    ));
                                }
                            };
                            excluded_keys.push(key_str.clone());
                            let v = if let JsValue::Object(o) = &val {
                                if let Some(obj) = self.get_object(o.id) {
                                    obj.borrow().get_property(&key_str)
                                } else {
                                    JsValue::Undefined
                                }
                            } else {
                                JsValue::Undefined
                            };
                            self.bind_pattern(pat, v, kind, env)?;
                        }
                        ObjectPatternProperty::Rest(pat) => {
                            let rest_obj = self.create_object();
                            if let JsValue::Object(o) = &val
                                && let Some(src) = self.get_object(o.id)
                            {
                                let src = src.borrow();
                                for key in &src.property_order {
                                    if !excluded_keys.contains(key)
                                        && let Some(desc) = src.properties.get(key)
                                        && desc.enumerable.unwrap_or(true)
                                    {
                                        rest_obj.borrow_mut().insert_value(
                                            key.clone(),
                                            desc.value.clone().unwrap_or(JsValue::Undefined),
                                        );
                                    }
                                }
                            }
                            let rest_id = rest_obj.borrow().id.unwrap();
                            let rest_val = JsValue::Object(crate::types::JsObject { id: rest_id });
                            self.bind_pattern(pat, rest_val, kind, env)?;
                        }
                    }
                }
                Ok(())
            }
            Pattern::Rest(inner) => self.bind_pattern(inner, val, kind, env),
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
                    // var declarations should go in the parent scope (hoisting)
                    let decl_env = if decl.kind == VarKind::Var {
                        env
                    } else {
                        &for_env
                    };
                    let comp = self.exec_variable_declaration(decl, decl_env);
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
        if let JsValue::Object(ref o) = obj_val
            && let Some(obj) = self.get_object(o.id)
        {
            let keys = obj.borrow().enumerable_keys_with_proto();
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
                        let bind_env = if decl.kind == VarKind::Var {
                            env
                        } else {
                            &for_env
                        };
                        if let Some(d) = decl.declarations.first()
                            && let Err(e) = self.bind_pattern(&d.pattern, key_val, kind, bind_env)
                        {
                            return Completion::Throw(e);
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
        Completion::Normal(JsValue::Undefined)
    }

    fn exec_for_of(&mut self, fo: &ForOfStatement, env: &EnvRef) -> Completion {
        let iterable = match self.eval_expr(&fo.right, env) {
            Completion::Normal(v) => v,
            other => return other,
        };

        let iterator = if fo.is_await {
            match self.get_async_iterator(&iterable) {
                Ok(iter) => iter,
                Err(e) => return Completion::Throw(e),
            }
        } else {
            match self.get_iterator(&iterable) {
                Ok(iter) => iter,
                Err(e) => return Completion::Throw(e),
            }
        };

        loop {
            let step_result = match self.iterator_next(&iterator) {
                Ok(v) => v,
                Err(e) => return Completion::Throw(e),
            };
            let step_result = if fo.is_await {
                match self.await_value(&step_result) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => {
                        self.iterator_close(&iterator, e.clone());
                        return Completion::Throw(e);
                    }
                    other => return other,
                }
            } else {
                step_result
            };
            if self.iterator_complete(&step_result) {
                break;
            }
            let val = self.iterator_value(&step_result);

            let for_env = Environment::new(Some(env.clone()));
            match &fo.left {
                ForInOfLeft::Variable(decl) => {
                    let kind = match decl.kind {
                        VarKind::Var => BindingKind::Var,
                        VarKind::Let => BindingKind::Let,
                        VarKind::Const => BindingKind::Const,
                    };
                    let bind_env = if decl.kind == VarKind::Var {
                        env
                    } else {
                        &for_env
                    };
                    if let Some(d) = decl.declarations.first()
                        && let Err(e) = self.bind_pattern(&d.pattern, val, kind, bind_env)
                    {
                        self.iterator_close(&iterator, e.clone());
                        return Completion::Throw(e);
                    }
                }
                ForInOfLeft::Pattern(pat) => {
                    if let Pattern::Identifier(name) = pat {
                        let _ = env.borrow_mut().set(name, val);
                    } else if let Err(e) = self.bind_pattern(pat, val, BindingKind::Let, &for_env) {
                        self.iterator_close(&iterator, e.clone());
                        return Completion::Throw(e);
                    }
                }
            }
            match self.exec_statement(&fo.body, &for_env) {
                Completion::Normal(_) | Completion::Continue(None) => {}
                Completion::Break(None) => {
                    self.iterator_close(&iterator, JsValue::Undefined);
                    break;
                }
                Completion::Return(v) => {
                    self.iterator_close(&iterator, JsValue::Undefined);
                    return Completion::Return(v);
                }
                other => return other,
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
                    if let Some(param) = &handler.param
                        && let Err(e) = self.bind_pattern(param, val, BindingKind::Let, &catch_env)
                    {
                        return Completion::Throw(e);
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
        if !found && let Some(idx) = default_idx {
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
        Completion::Normal(JsValue::Undefined)
    }
}
