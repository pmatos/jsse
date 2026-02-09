use super::*;

impl Interpreter {
    pub(crate) fn exec_statements(&mut self, stmts: &[Statement], env: &EnvRef) -> Completion {
        if Self::is_strict_mode_body(stmts) {
            env.borrow_mut().strict = true;
        }
        // Hoist var and function declarations
        let var_scope = Environment::find_var_scope(env);
        let is_global = var_scope.borrow().global_object.is_some();
        let is_block_scope = !Rc::ptr_eq(env, &var_scope);
        for stmt in stmts {
            // Recursively hoist var declarations from all sub-statements
            self.hoist_vars_from_stmt(stmt, &var_scope, is_global);
            // Check for function declarations (including inside labels)
            if let Some(f) = Self::unwrap_labeled_function(stmt) {
                self.hoist_function_decl(f, env, is_global);
            }
        }

        // Annex B.3.3: at function/global level in sloppy mode,
        // create var bindings for function declarations inside blocks.
        // Skip names that conflict with parameters or lexical bindings.
        if !is_block_scope && !env.borrow().strict {
            let mut all_annexb = Vec::new();
            let mut blocked = Vec::new();
            Self::collect_annexb_function_names(stmts, &mut all_annexb, &mut blocked);
            if !all_annexb.is_empty() {
                let mut registered = Vec::new();
                // Collect top-level var/function names from statements
                let mut top_level_var_names = Vec::new();
                // Collect top-level lexical names (let/const/class)
                let mut lexical_names = Vec::new();
                for stmt in stmts {
                    match stmt {
                        Statement::FunctionDeclaration(f) => {
                            top_level_var_names.push(f.name.clone());
                        }
                        Statement::Variable(decl) if decl.kind == VarKind::Var => {
                            for d in &decl.declarations {
                                Self::collect_pattern_names(&d.pattern, &mut top_level_var_names);
                            }
                        }
                        Statement::Variable(decl)
                            if matches!(decl.kind, VarKind::Let | VarKind::Const) =>
                        {
                            for d in &decl.declarations {
                                Self::collect_pattern_names(&d.pattern, &mut lexical_names);
                            }
                        }
                        Statement::ClassDeclaration(cls) => {
                            lexical_names.push(cls.name.clone());
                        }
                        _ => {}
                    }
                }
                for name in all_annexb {
                    // Skip if name conflicts with a lexical declaration
                    if lexical_names.contains(&name) {
                        continue;
                    }
                    // Skip if name conflicts with a parameter (binding exists but
                    // is NOT from a top-level var/function declaration)
                    let is_param = env.borrow().bindings.contains_key(&name)
                        && !top_level_var_names.contains(&name);
                    if is_param {
                        continue;
                    }
                    if !env.borrow().bindings.contains_key(&name) {
                        if is_global {
                            env.borrow_mut().declare_global_var(&name);
                        } else {
                            env.borrow_mut().declare(&name, BindingKind::Var);
                        }
                    }
                    registered.push(name);
                }
                if !registered.is_empty() {
                    var_scope.borrow_mut().annexb_function_names = Some(registered);
                }
            }
        }

        self.call_stack_envs.push(env.clone());
        let mut result = Completion::Empty;
        for stmt in stmts {
            self.maybe_gc();
            let comp = self.exec_statement(stmt, env);
            match comp {
                Completion::Normal(val) => result = Completion::Normal(val),
                Completion::Empty => {} // keep previous result (UpdateEmpty semantics)
                Completion::Break(label, break_val) => {
                    let val = match break_val {
                        None => Some(result.value_or(JsValue::Undefined)),
                        some => some,
                    };
                    self.call_stack_envs.pop();
                    return Completion::Break(label, val);
                }
                Completion::Continue(label, cont_val) => {
                    let val = match cont_val {
                        None => Some(result.value_or(JsValue::Undefined)),
                        some => some,
                    };
                    self.call_stack_envs.pop();
                    return Completion::Continue(label, val);
                }
                other => {
                    self.call_stack_envs.pop();
                    return other;
                }
            }
        }
        self.call_stack_envs.pop();
        result
    }

    fn unwrap_labeled_function(stmt: &Statement) -> Option<&FunctionDecl> {
        match stmt {
            Statement::FunctionDeclaration(f) => Some(f),
            Statement::Labeled(_, inner) => Self::unwrap_labeled_function(inner),
            _ => None,
        }
    }

    fn hoist_function_decl(&mut self, f: &FunctionDecl, env: &EnvRef, is_global: bool) {
        if is_global {
            env.borrow_mut().declare_global_var(&f.name);
        } else {
            env.borrow_mut().declare(&f.name, BindingKind::Var);
        }
        let func = JsFunction::User {
            name: Some(f.name.clone()),
            params: f.params.clone(),
            body: f.body.clone(),
            closure: env.clone(),
            is_arrow: false,
            is_strict: Self::is_strict_mode_body(&f.body) || env.borrow().strict,
            is_generator: f.is_generator,
            is_async: f.is_async,
            source_text: f.source_text.clone(),
        };
        let val = self.create_function(func);
        let _ = env.borrow_mut().set(&f.name, val);
    }

    fn collect_pattern_names(pat: &Pattern, names: &mut Vec<String>) {
        match pat {
            Pattern::Identifier(name) => names.push(name.clone()),
            Pattern::Array(elems) => {
                for elem in elems.iter().flatten() {
                    match elem {
                        ArrayPatternElement::Pattern(p) | ArrayPatternElement::Rest(p) => {
                            Self::collect_pattern_names(p, names);
                        }
                    }
                }
            }
            Pattern::Object(props) => {
                for prop in props {
                    match prop {
                        ObjectPatternProperty::KeyValue(_, p) | ObjectPatternProperty::Rest(p) => {
                            Self::collect_pattern_names(p, names);
                        }
                        ObjectPatternProperty::Shorthand(name) => names.push(name.clone()),
                    }
                }
            }
            Pattern::Assign(inner, _) | Pattern::Rest(inner) => {
                Self::collect_pattern_names(inner, names);
            }
            Pattern::MemberExpression(_) => {}
        }
    }

    pub(crate) fn hoist_pattern(&self, pat: &Pattern, env: &EnvRef, is_global: bool) {
        match pat {
            Pattern::Identifier(name) => {
                if !env.borrow().bindings.contains_key(name) {
                    if is_global {
                        env.borrow_mut().declare_global_var(name);
                    } else {
                        env.borrow_mut().declare(name, BindingKind::Var);
                    }
                }
            }
            Pattern::Array(elems) => {
                for elem in elems.iter().flatten() {
                    match elem {
                        ArrayPatternElement::Pattern(p) | ArrayPatternElement::Rest(p) => {
                            self.hoist_pattern(p, env, is_global);
                        }
                    }
                }
            }
            Pattern::Object(props) => {
                for prop in props {
                    match prop {
                        ObjectPatternProperty::KeyValue(_, p) | ObjectPatternProperty::Rest(p) => {
                            self.hoist_pattern(p, env, is_global);
                        }
                        ObjectPatternProperty::Shorthand(name) => {
                            if !env.borrow().bindings.contains_key(name) {
                                if is_global {
                                    env.borrow_mut().declare_global_var(name);
                                } else {
                                    env.borrow_mut().declare(name, BindingKind::Var);
                                }
                            }
                        }
                    }
                }
            }
            Pattern::Assign(inner, _) | Pattern::Rest(inner) => {
                self.hoist_pattern(inner, env, is_global);
            }
            Pattern::MemberExpression(_) => {}
        }
    }

    fn hoist_vars_from_stmt(&self, stmt: &Statement, var_scope: &EnvRef, is_global: bool) {
        match stmt {
            Statement::Variable(decl) if decl.kind == VarKind::Var => {
                for d in &decl.declarations {
                    self.hoist_pattern(&d.pattern, var_scope, is_global);
                }
            }
            Statement::Block(stmts) => {
                for s in stmts {
                    self.hoist_vars_from_stmt(s, var_scope, is_global);
                }
            }
            Statement::If(i) => {
                self.hoist_vars_from_stmt(&i.consequent, var_scope, is_global);
                if let Some(alt) = &i.alternate {
                    self.hoist_vars_from_stmt(alt, var_scope, is_global);
                }
            }
            Statement::While(w) => self.hoist_vars_from_stmt(&w.body, var_scope, is_global),
            Statement::DoWhile(d) => self.hoist_vars_from_stmt(&d.body, var_scope, is_global),
            Statement::For(f) => {
                if let Some(ForInit::Variable(decl)) = &f.init {
                    if decl.kind == VarKind::Var {
                        for d in &decl.declarations {
                            self.hoist_pattern(&d.pattern, var_scope, is_global);
                        }
                    }
                }
                self.hoist_vars_from_stmt(&f.body, var_scope, is_global);
            }
            Statement::ForIn(fi) => {
                if let ForInOfLeft::Variable(decl) = &fi.left {
                    if decl.kind == VarKind::Var {
                        for d in &decl.declarations {
                            self.hoist_pattern(&d.pattern, var_scope, is_global);
                        }
                    }
                }
                self.hoist_vars_from_stmt(&fi.body, var_scope, is_global);
            }
            Statement::ForOf(fo) => {
                if let ForInOfLeft::Variable(decl) = &fo.left {
                    if decl.kind == VarKind::Var {
                        for d in &decl.declarations {
                            self.hoist_pattern(&d.pattern, var_scope, is_global);
                        }
                    }
                }
                self.hoist_vars_from_stmt(&fo.body, var_scope, is_global);
            }
            Statement::Switch(sw) => {
                for case in &sw.cases {
                    for s in &case.consequent {
                        self.hoist_vars_from_stmt(s, var_scope, is_global);
                    }
                }
            }
            Statement::Try(t) => {
                for s in &t.block {
                    self.hoist_vars_from_stmt(s, var_scope, is_global);
                }
                if let Some(handler) = &t.handler {
                    for s in &handler.body {
                        self.hoist_vars_from_stmt(s, var_scope, is_global);
                    }
                }
                if let Some(finalizer) = &t.finalizer {
                    for s in finalizer {
                        self.hoist_vars_from_stmt(s, var_scope, is_global);
                    }
                }
            }
            Statement::Labeled(_, inner) => {
                self.hoist_vars_from_stmt(inner, var_scope, is_global);
            }
            Statement::With(_, inner) => {
                self.hoist_vars_from_stmt(inner, var_scope, is_global);
            }
            _ => {}
        }
    }

    // Annex B.3.3: recursively find function declarations inside blocks
    // for var-scope hoisting at the function/global level.
    // `blocked` tracks lexical names from enclosing scopes that would
    // cause an early error if a var with the same name were declared.
    fn collect_annexb_function_names(
        stmts: &[Statement],
        names: &mut Vec<String>,
        blocked: &mut Vec<String>,
    ) {
        for stmt in stmts {
            match stmt {
                Statement::Block(inner) => {
                    // Collect lexical names in this block
                    let mut block_lexicals = Vec::new();
                    for s in inner {
                        match s {
                            Statement::Variable(decl)
                                if matches!(decl.kind, VarKind::Let | VarKind::Const) =>
                            {
                                for d in &decl.declarations {
                                    Self::collect_pattern_names(&d.pattern, &mut block_lexicals);
                                }
                            }
                            Statement::ClassDeclaration(cls) => {
                                block_lexicals.push(cls.name.clone());
                            }
                            _ => {}
                        }
                    }
                    // Check function declarations in this block
                    // Only regular functions (not generators or async) per Annex B.3.3
                    for s in inner {
                        if let Statement::FunctionDeclaration(f) = s {
                            if !f.is_generator
                                && !f.is_async
                                && !names.contains(&f.name)
                                && !blocked.contains(&f.name)
                                && !block_lexicals.contains(&f.name)
                            {
                                names.push(f.name.clone());
                            }
                        }
                    }
                    // Recurse with block lexicals added to blocked set
                    let prev_len = blocked.len();
                    blocked.extend(block_lexicals);
                    Self::collect_annexb_function_names(inner, names, blocked);
                    blocked.truncate(prev_len);
                }
                Statement::If(if_stmt) => {
                    Self::collect_annexb_function_names(
                        std::slice::from_ref(&*if_stmt.consequent),
                        names,
                        blocked,
                    );
                    if let Some(ref alt) = if_stmt.alternate {
                        Self::collect_annexb_function_names(
                            std::slice::from_ref(&**alt),
                            names,
                            blocked,
                        );
                    }
                }
                Statement::While(w) => {
                    Self::collect_annexb_function_names(
                        std::slice::from_ref(&*w.body),
                        names,
                        blocked,
                    );
                }
                Statement::DoWhile(dw) => {
                    Self::collect_annexb_function_names(
                        std::slice::from_ref(&*dw.body),
                        names,
                        blocked,
                    );
                }
                Statement::For(f) => {
                    let prev_len = blocked.len();
                    if let Some(ForInit::Variable(decl)) = &f.init {
                        if matches!(decl.kind, VarKind::Let | VarKind::Const) {
                            for d in &decl.declarations {
                                Self::collect_pattern_names(&d.pattern, blocked);
                            }
                        }
                    }
                    Self::collect_annexb_function_names(
                        std::slice::from_ref(&*f.body),
                        names,
                        blocked,
                    );
                    blocked.truncate(prev_len);
                }
                Statement::ForIn(fi) => {
                    let prev_len = blocked.len();
                    if let ForInOfLeft::Variable(decl) = &fi.left {
                        if matches!(decl.kind, VarKind::Let | VarKind::Const) {
                            for d in &decl.declarations {
                                Self::collect_pattern_names(&d.pattern, blocked);
                            }
                        }
                    }
                    Self::collect_annexb_function_names(
                        std::slice::from_ref(&*fi.body),
                        names,
                        blocked,
                    );
                    blocked.truncate(prev_len);
                }
                Statement::ForOf(fo) => {
                    let prev_len = blocked.len();
                    if let ForInOfLeft::Variable(decl) = &fo.left {
                        if matches!(decl.kind, VarKind::Let | VarKind::Const) {
                            for d in &decl.declarations {
                                Self::collect_pattern_names(&d.pattern, blocked);
                            }
                        }
                    }
                    Self::collect_annexb_function_names(
                        std::slice::from_ref(&*fo.body),
                        names,
                        blocked,
                    );
                    blocked.truncate(prev_len);
                }
                Statement::Labeled(_, inner) => {
                    Self::collect_annexb_function_names(
                        std::slice::from_ref(&**inner),
                        names,
                        blocked,
                    );
                }
                Statement::Switch(s) => {
                    // Switch creates a single scope for all cases
                    let mut switch_lexicals = Vec::new();
                    for case in &s.cases {
                        for cs in &case.consequent {
                            match cs {
                                Statement::Variable(decl)
                                    if matches!(decl.kind, VarKind::Let | VarKind::Const) =>
                                {
                                    for d in &decl.declarations {
                                        Self::collect_pattern_names(
                                            &d.pattern,
                                            &mut switch_lexicals,
                                        );
                                    }
                                }
                                Statement::ClassDeclaration(cls) => {
                                    switch_lexicals.push(cls.name.clone());
                                }
                                _ => {}
                            }
                        }
                    }
                    for case in &s.cases {
                        for cs in &case.consequent {
                            if let Statement::FunctionDeclaration(f) = cs {
                                if !f.is_generator
                                    && !f.is_async
                                    && !names.contains(&f.name)
                                    && !blocked.contains(&f.name)
                                    && !switch_lexicals.contains(&f.name)
                                {
                                    names.push(f.name.clone());
                                }
                            }
                        }
                    }
                    let prev_len = blocked.len();
                    blocked.extend(switch_lexicals);
                    for case in &s.cases {
                        Self::collect_annexb_function_names(&case.consequent, names, blocked);
                    }
                    blocked.truncate(prev_len);
                }
                Statement::Try(t) => {
                    Self::collect_annexb_function_names(&t.block, names, blocked);
                    if let Some(ref h) = t.handler {
                        let prev_len = blocked.len();
                        if let Some(ref param) = h.param {
                            Self::collect_pattern_names(param, blocked);
                        }
                        Self::collect_annexb_function_names(&h.body, names, blocked);
                        blocked.truncate(prev_len);
                    }
                    if let Some(ref fin) = t.finalizer {
                        Self::collect_annexb_function_names(fin, names, blocked);
                    }
                }
                _ => {}
            }
        }
    }

    pub(crate) fn exec_statement(&mut self, stmt: &Statement, env: &EnvRef) -> Completion {
        match stmt {
            Statement::Empty => Completion::Empty,
            Statement::Expression(expr) => self.eval_expr(expr, env),
            Statement::Block(stmts) => {
                let block_env = Environment::new(Some(env.clone()));
                let result = self.exec_statements(stmts, &block_env);
                self.dispose_resources(&block_env, result)
            }
            Statement::Variable(decl) => {
                let r = self.exec_variable_declaration(decl, env);
                if r.is_abrupt() { r } else { Completion::Empty }
            }
            Statement::If(if_stmt) => {
                let test = self.eval_expr(&if_stmt.test, env);
                let test = match test {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if to_boolean(&test) {
                    self.exec_statement(&if_stmt.consequent, env)
                        .update_empty(JsValue::Undefined)
                } else if let Some(alt) = &if_stmt.alternate {
                    self.exec_statement(alt, env)
                        .update_empty(JsValue::Undefined)
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
            Statement::Break(label) => Completion::Break(label.clone(), None),
            Statement::Continue(label) => Completion::Continue(label.clone(), None),
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
                    Completion::Break(Some(l), val) if l == label => {
                        Completion::Normal(val.clone().unwrap_or(JsValue::Undefined))
                    }
                    Completion::Continue(Some(l), val) if l == label => {
                        Completion::Normal(val.clone().unwrap_or(JsValue::Undefined))
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
                            is_function_scope: false,
                            with_object: Some(WithObject {
                                object: obj_data,
                                obj_id: obj_ref.id,
                                unscopables: unscopables_data,
                            }),
                            dispose_stack: None,
                            global_object: None,
                            annexb_function_names: None,
                        }));
                        self.exec_statement(body, &with_env)
                    } else {
                        Completion::Normal(JsValue::Undefined)
                    }
                } else {
                    Completion::Normal(JsValue::Undefined)
                }
            }
            Statement::Debugger => Completion::Empty,
            Statement::FunctionDeclaration(f) => {
                // Annex B.3.3: in sloppy-mode blocks, copy block-scoped value to var scope
                // Only for regular functions (not generators or async)
                if !f.is_generator && !f.is_async && !env.borrow().strict {
                    let is_block =
                        !env.borrow().is_function_scope && env.borrow().global_object.is_none();
                    if is_block {
                        let var_scope = Environment::find_var_scope(env);
                        let is_registered = var_scope
                            .borrow()
                            .annexb_function_names
                            .as_ref()
                            .map(|names| names.contains(&f.name))
                            .unwrap_or(false);
                        if is_registered {
                            let val = env.borrow().get(&f.name).unwrap_or(JsValue::Undefined);
                            let _ = var_scope.borrow_mut().set(&f.name, val);
                        }
                    }
                }
                Completion::Empty
            }
            Statement::ClassDeclaration(cd) => {
                let class_val = self.eval_class(
                    &cd.name,
                    &cd.super_class,
                    &cd.body,
                    env,
                    cd.source_text.clone(),
                );
                match class_val {
                    Completion::Normal(val) => {
                        env.borrow_mut().declare(&cd.name, BindingKind::Let);
                        let _ = env.borrow_mut().set(&cd.name, val);
                        Completion::Empty
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
        let is_using = matches!(decl.kind, VarKind::Using | VarKind::AwaitUsing);
        let kind = match decl.kind {
            VarKind::Var => BindingKind::Var,
            VarKind::Let => BindingKind::Let,
            VarKind::Const | VarKind::Using | VarKind::AwaitUsing => BindingKind::Const,
        };
        for d in &decl.declarations {
            if d.init.is_none()
                && decl.kind == VarKind::Var
                && let Pattern::Identifier(ref name) = d.pattern
            {
                let var_scope = Environment::find_var_scope(env);
                if !var_scope.borrow().bindings.contains_key(name) {
                    var_scope.borrow_mut().declare(name, kind);
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
                if d.init
                    .as_ref()
                    .is_some_and(|e| e.is_anonymous_function_definition())
                {
                    self.set_function_name(&val, name);
                }
            }
            if is_using {
                let hint = if decl.kind == VarKind::AwaitUsing {
                    DisposeHint::Async
                } else {
                    DisposeHint::Sync
                };
                if let Err(e) = self.add_disposable_resource(env, &val, hint) {
                    return Completion::Throw(e);
                }
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
                if kind == BindingKind::Var {
                    let var_scope = Environment::find_var_scope(env);
                    if !var_scope.borrow().bindings.contains_key(name) {
                        var_scope.borrow_mut().declare(name, kind);
                    }
                } else {
                    env.borrow_mut().declare(name, kind);
                }
                // Set through normal scope chain (handles with-objects, etc.)
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
                    if default.is_anonymous_function_definition() {
                        self.set_function_name(&v, name);
                    }
                }
                self.bind_pattern(inner, v, kind, env)
            }
            Pattern::Array(elements) => {
                let iterator = self.get_iterator(&val)?;
                if let JsValue::Object(o) = &iterator {
                    self.gc_temp_roots.push(o.id);
                }
                let mut done = false;
                let mut error: Option<JsValue> = None;
                for elem in elements {
                    if let Some(elem) = elem {
                        match elem {
                            ArrayPatternElement::Pattern(p) => {
                                let item = if done {
                                    JsValue::Undefined
                                } else {
                                    match self.iterator_step(&iterator) {
                                        Ok(Some(result)) => match self.iterator_value(&result) {
                                            Ok(v) => v,
                                            Err(e) => {
                                                done = true;
                                                error = Some(e);
                                                break;
                                            }
                                        },
                                        Ok(None) => {
                                            done = true;
                                            JsValue::Undefined
                                        }
                                        Err(e) => {
                                            done = true;
                                            error = Some(e);
                                            break;
                                        }
                                    }
                                };
                                if let Err(e) = self.bind_pattern(p, item, kind, env) {
                                    error = Some(e);
                                    break;
                                }
                            }
                            ArrayPatternElement::Rest(p) => {
                                let mut rest = Vec::new();
                                if !done {
                                    loop {
                                        match self.iterator_step(&iterator) {
                                            Ok(Some(result)) => {
                                                match self.iterator_value(&result) {
                                                    Ok(v) => rest.push(v),
                                                    Err(e) => {
                                                        done = true;
                                                        error = Some(e);
                                                        break;
                                                    }
                                                }
                                            }
                                            Ok(None) => {
                                                done = true;
                                                break;
                                            }
                                            Err(e) => {
                                                done = true;
                                                error = Some(e);
                                                break;
                                            }
                                        }
                                    }
                                }
                                if error.is_none() {
                                    let arr = self.create_array(rest);
                                    if let Err(e) = self.bind_pattern(p, arr, kind, env) {
                                        error = Some(e);
                                    }
                                }
                                break;
                            }
                        }
                    } else {
                        // Elision — skip one iterator step
                        if !done {
                            match self.iterator_step(&iterator) {
                                Ok(None) => {
                                    done = true;
                                }
                                Ok(Some(_)) => {}
                                Err(e) => {
                                    done = true;
                                    error = Some(e);
                                    break;
                                }
                            }
                        }
                    }
                }
                let unroot_iter = |s: &mut Self| {
                    if let JsValue::Object(o) = &iterator {
                        if let Some(pos) = s.gc_temp_roots.iter().rposition(|&id| id == o.id) {
                            s.gc_temp_roots.remove(pos);
                        }
                    }
                };
                if !done {
                    if let Some(err) = error {
                        let _ = self.iterator_close_result(&iterator);
                        unroot_iter(self);
                        return Err(err);
                    }
                    let r = self.iterator_close_result(&iterator);
                    unroot_iter(self);
                    return r;
                }
                unroot_iter(self);
                if let Some(err) = error {
                    return Err(err);
                }
                Ok(())
            }
            Pattern::Object(props) => {
                // RequireObjectCoercible + ToObject for primitives
                let obj_val = match self.to_object(&val) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => unreachable!(),
                };
                let mut excluded_keys = Vec::new();
                for prop in props {
                    match prop {
                        ObjectPatternProperty::Shorthand(name) => {
                            excluded_keys.push(name.clone());
                            let v = if let JsValue::Object(o) = &obj_val {
                                match self.get_object_property(o.id, name, &obj_val) {
                                    Completion::Normal(v) => v,
                                    Completion::Throw(e) => return Err(e),
                                    _ => JsValue::Undefined,
                                }
                            } else {
                                JsValue::Undefined
                            };
                            if kind == BindingKind::Var {
                                let var_scope = Environment::find_var_scope(env);
                                if !var_scope.borrow().bindings.contains_key(name) {
                                    var_scope.borrow_mut().declare(name, kind);
                                }
                            } else if !env.borrow().bindings.contains_key(name) {
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
                            let v = if let JsValue::Object(o) = &obj_val {
                                match self.get_object_property(o.id, &key_str, &obj_val) {
                                    Completion::Normal(v) => v,
                                    Completion::Throw(e) => return Err(e),
                                    _ => JsValue::Undefined,
                                }
                            } else {
                                JsValue::Undefined
                            };
                            self.bind_pattern(pat, v, kind, env)?;
                        }
                        ObjectPatternProperty::Rest(pat) => {
                            let rest_obj = self.create_object();
                            if let JsValue::Object(o) = &obj_val {
                                if let Some(src) = self.get_object(o.id) {
                                    let keys: Vec<String> = src.borrow().property_order.clone();
                                    for key in &keys {
                                        if !excluded_keys.contains(key) {
                                            let desc = src.borrow().get_own_property(key);
                                            if let Some(ref d) = desc
                                                && d.enumerable.unwrap_or(true)
                                            {
                                                let v = match self
                                                    .get_object_property(o.id, key, &obj_val)
                                                {
                                                    Completion::Normal(v) => v,
                                                    Completion::Throw(e) => return Err(e),
                                                    _ => JsValue::Undefined,
                                                };
                                                rest_obj.borrow_mut().insert_value(key.clone(), v);
                                            }
                                        }
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
            Pattern::MemberExpression(expr) => self.assign_to_expr(expr, val, env),
        }
    }

    fn exec_while(&mut self, w: &WhileStatement, env: &EnvRef) -> Completion {
        let mut v = JsValue::Undefined;
        loop {
            let test = match self.eval_expr(&w.test, env) {
                Completion::Normal(v) => v,
                other => return other,
            };
            if !to_boolean(&test) {
                break;
            }
            match self.exec_statement(&w.body, env) {
                Completion::Normal(val) => {
                    v = val;
                }
                Completion::Empty => {}
                Completion::Continue(None, cont_val) => {
                    if let Some(val) = cont_val {
                        v = val;
                    }
                }
                Completion::Break(None, break_val) => {
                    if let Some(val) = break_val {
                        v = val;
                    }
                    return Completion::Normal(v);
                }
                other => return other,
            }
        }
        Completion::Normal(v)
    }

    fn exec_do_while(&mut self, dw: &DoWhileStatement, env: &EnvRef) -> Completion {
        let mut v = JsValue::Undefined;
        loop {
            match self.exec_statement(&dw.body, env) {
                Completion::Normal(val) => {
                    v = val;
                }
                Completion::Empty => {}
                Completion::Continue(None, cont_val) => {
                    if let Some(val) = cont_val {
                        v = val;
                    }
                }
                Completion::Break(None, break_val) => {
                    if let Some(val) = break_val {
                        v = val;
                    }
                    return Completion::Normal(v);
                }
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
        Completion::Normal(v)
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
        let mut v = JsValue::Undefined;
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
                Completion::Normal(val) => {
                    v = val;
                }
                Completion::Empty => {}
                Completion::Continue(None, cont_val) => {
                    if let Some(val) = cont_val {
                        v = val;
                    }
                }
                Completion::Break(None, break_val) => {
                    if let Some(val) = break_val {
                        v = val;
                    }
                    return Completion::Normal(v);
                }
                other => return other,
            }
            if let Some(update) = &f.update {
                let comp = self.eval_expr(update, &for_env);
                if comp.is_abrupt() {
                    return comp;
                }
            }
        }
        Completion::Normal(v)
    }

    fn exec_for_in(&mut self, fi: &ForInStatement, env: &EnvRef) -> Completion {
        // Per spec 14.7.5.6 ForIn/OfHeadEvaluation:
        // If the LHS is a lexical declaration, create TDZ bindings before evaluating RHS
        let is_lexical =
            matches!(&fi.left, ForInOfLeft::Variable(decl) if decl.kind != VarKind::Var);
        let eval_env = if is_lexical {
            let tdz_env = Environment::new(Some(env.clone()));
            if let ForInOfLeft::Variable(decl) = &fi.left {
                if let Some(d) = decl.declarations.first() {
                    let mut names = Vec::new();
                    Self::collect_pattern_names(&d.pattern, &mut names);
                    for name in &names {
                        tdz_env.borrow_mut().declare(name, BindingKind::Let);
                    }
                }
            }
            tdz_env
        } else {
            env.clone()
        };

        let obj_val = match self.eval_expr(&fi.right, &eval_env) {
            Completion::Normal(v) => v,
            other => return other,
        };
        // After evaluating expr, restore to oldEnv (use original env from here)
        if obj_val.is_nullish() {
            return Completion::Normal(JsValue::Undefined);
        }
        let obj_val = match self.to_object(&obj_val) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Completion::Throw(e),
            _ => return Completion::Normal(JsValue::Undefined),
        };
        let mut v = JsValue::Undefined;
        if let JsValue::Object(ref o) = obj_val {
            let obj_id = o.id;
            let keys = {
                let is_proxy = self
                    .get_object(obj_id)
                    .map(|obj| obj.borrow().is_proxy())
                    .unwrap_or(false);
                if is_proxy {
                    match self.proxy_enumerable_keys_with_proto(obj_id) {
                        Ok(k) => k,
                        Err(e) => return Completion::Throw(e),
                    }
                } else if let Some(obj) = self.get_object(obj_id) {
                    obj.borrow().enumerable_keys_with_proto()
                } else {
                    return Completion::Normal(JsValue::Undefined);
                }
            };
            for key in keys {
                // Skip keys that have been deleted during iteration (proxy-aware)
                let still_exists = match self.proxy_has_property(obj_id, &key) {
                    Ok(b) => b,
                    Err(e) => return Completion::Throw(e),
                };
                if !still_exists {
                    continue;
                }
                let key_val = JsValue::String(JsString::from_str(&key));
                let for_env = Environment::new(Some(env.clone()));
                match &fi.left {
                    ForInOfLeft::Variable(decl) => {
                        let kind = match decl.kind {
                            VarKind::Var => BindingKind::Var,
                            VarKind::Let => BindingKind::Let,
                            VarKind::Const | VarKind::Using | VarKind::AwaitUsing => {
                                BindingKind::Const
                            }
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
                    ForInOfLeft::Pattern(pat) => match pat {
                        Pattern::Identifier(name) => {
                            if !env.borrow().has(name) && env.borrow().strict {
                                return Completion::Throw(
                                    self.create_reference_error(&format!("{name} is not defined")),
                                );
                            }
                            let _ = env.borrow_mut().set(name, key_val);
                        }
                        Pattern::MemberExpression(expr) => {
                            if let Err(e) = self.assign_to_expr(expr, key_val, env) {
                                return Completion::Throw(e);
                            }
                        }
                        _ => {
                            if let Err(e) =
                                self.bind_pattern(pat, key_val, BindingKind::Let, &for_env)
                            {
                                return Completion::Throw(e);
                            }
                        }
                    },
                }
                let result = self.exec_statement(&fi.body, &for_env);
                match result {
                    Completion::Normal(val) => {
                        v = val;
                    }
                    Completion::Empty => {}
                    Completion::Continue(None, cont_val) => {
                        if let Some(val) = cont_val {
                            v = val;
                        }
                    }
                    Completion::Break(None, break_val) => {
                        if let Some(val) = break_val {
                            v = val;
                        }
                        return Completion::Normal(v);
                    }
                    other => return other,
                }
            }
        }
        Completion::Normal(v)
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

        self.gc_root_value(&iterator);
        let result = self.exec_for_of_loop(fo, env, &iterator);
        self.gc_unroot_value(&iterator);
        result
    }

    fn exec_for_of_loop(
        &mut self,
        fo: &ForOfStatement,
        env: &EnvRef,
        iterator: &JsValue,
    ) -> Completion {
        let mut v = JsValue::Undefined;
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
            match self.iterator_complete(&step_result) {
                Ok(true) => break,
                Err(e) => {
                    return Completion::Throw(e);
                }
                _ => {}
            }
            let val = match self.iterator_value(&step_result) {
                Ok(v) => v,
                Err(e) => {
                    self.iterator_close(&iterator, e.clone());
                    return Completion::Throw(e);
                }
            };

            let for_env = Environment::new(Some(env.clone()));
            match &fo.left {
                ForInOfLeft::Variable(decl) => {
                    let is_using = matches!(decl.kind, VarKind::Using | VarKind::AwaitUsing);
                    let kind = match decl.kind {
                        VarKind::Var => BindingKind::Var,
                        VarKind::Let => BindingKind::Let,
                        VarKind::Const | VarKind::Using | VarKind::AwaitUsing => BindingKind::Const,
                    };
                    let bind_env = if decl.kind == VarKind::Var {
                        env
                    } else {
                        &for_env
                    };
                    if is_using {
                        let hint = if decl.kind == VarKind::AwaitUsing {
                            DisposeHint::Async
                        } else {
                            DisposeHint::Sync
                        };
                        if let Err(e) = self.add_disposable_resource(bind_env, &val, hint) {
                            self.iterator_close(&iterator, e.clone());
                            return Completion::Throw(e);
                        }
                    }
                    if let Some(d) = decl.declarations.first()
                        && let Err(e) = self.bind_pattern(&d.pattern, val, kind, bind_env)
                    {
                        self.iterator_close(&iterator, e.clone());
                        return Completion::Throw(e);
                    }
                }
                ForInOfLeft::Pattern(pat) => match pat {
                    Pattern::Identifier(name) => {
                        if !env.borrow().has(name) && env.borrow().strict {
                            self.iterator_close(&iterator, JsValue::Undefined);
                            return Completion::Throw(
                                self.create_reference_error(&format!("{name} is not defined")),
                            );
                        }
                        let _ = env.borrow_mut().set(name, val);
                    }
                    Pattern::MemberExpression(expr) => {
                        if let Err(e) = self.assign_to_expr(expr, val, env) {
                            self.iterator_close(&iterator, e.clone());
                            return Completion::Throw(e);
                        }
                    }
                    _ => {
                        if let Err(e) = self.bind_pattern(pat, val, BindingKind::Let, &for_env) {
                            self.iterator_close(&iterator, e.clone());
                            return Completion::Throw(e);
                        }
                    }
                },
            }
            let body_result = self.exec_statement(&fo.body, &for_env);
            let body_result = self.dispose_resources(&for_env, body_result);
            match body_result {
                Completion::Normal(val) => {
                    v = val;
                }
                Completion::Empty => {}
                Completion::Continue(None, cont_val) => {
                    if let Some(val) = cont_val {
                        v = val;
                    }
                }
                Completion::Break(None, break_val) => {
                    if let Some(val) = break_val {
                        v = val;
                    }
                    self.iterator_close(&iterator, JsValue::Undefined);
                    return Completion::Normal(v);
                }
                Completion::Return(v) => {
                    self.iterator_close(&iterator, JsValue::Undefined);
                    return Completion::Return(v);
                }
                Completion::Throw(e) => {
                    self.iterator_close(&iterator, JsValue::Undefined);
                    return Completion::Throw(e);
                }
                Completion::Break(Some(label), val) => {
                    self.iterator_close(&iterator, JsValue::Undefined);
                    return Completion::Break(Some(label), val);
                }
                Completion::Continue(Some(label), val) => {
                    self.iterator_close(&iterator, JsValue::Undefined);
                    return Completion::Continue(Some(label), val);
                }
                other => return other,
            }
        }
        Completion::Normal(v)
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
            // If we're yielding, don't run finally - generator will handle it on return/throw
            if matches!(result, Completion::Yield(_)) {
                return result;
            }
            let fin_env = Environment::new(Some(env.clone()));
            let fin_result = self.exec_statements(finalizer, &fin_env);
            if fin_result.is_abrupt() {
                return fin_result;
            }
        }
        result.update_empty(JsValue::Undefined)
    }

    fn exec_switch(&mut self, s: &SwitchStatement, env: &EnvRef) -> Completion {
        let disc = match self.eval_expr(&s.discriminant, env) {
            Completion::Normal(v) => v,
            other => return other,
        };
        let switch_env = Environment::new(Some(env.clone()));

        // Hoist function declarations from all case bodies
        // (only regular functions, not generators/async — those stay block-scoped)
        for case in &s.cases {
            for stmt in &case.consequent {
                if let Statement::FunctionDeclaration(f) = stmt {
                    if !f.is_generator && !f.is_async {
                        switch_env.borrow_mut().declare(&f.name, BindingKind::Var);
                        let func = JsFunction::User {
                            name: Some(f.name.clone()),
                            params: f.params.clone(),
                            body: f.body.clone(),
                            closure: switch_env.clone(),
                            is_arrow: false,
                            is_strict: Self::is_strict_mode_body(&f.body)
                                || switch_env.borrow().strict,
                            is_generator: f.is_generator,
                            is_async: f.is_async,
                            source_text: f.source_text.clone(),
                        };
                        let val = self.create_function(func);
                        let _ = switch_env.borrow_mut().set(&f.name, val);
                    }
                }
            }
        }

        let default_idx = s.cases.iter().position(|c| c.test.is_none());
        let a_end = default_idx.unwrap_or(s.cases.len());
        let mut v = JsValue::Undefined;

        // Phase 1: Search list A (cases before default) for a match
        let mut found = false;
        for (i, case) in s.cases[..a_end].iter().enumerate() {
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
                // Fall through from match in A through rest of A, then default + B
                if let Some(r) = self.exec_switch_cases(&s.cases[i..a_end], &switch_env, &mut v) {
                    return r;
                }
                if let Some(di) = default_idx {
                    if let Some(r) = self.exec_switch_cases(&s.cases[di..], &switch_env, &mut v) {
                        return r;
                    }
                }
                return Completion::Normal(v);
            }
        }

        if let Some(di) = default_idx {
            let b_start = di + 1;

            // Phase 2: Search list B (cases after default) for a match
            for (i, case) in s.cases[b_start..].iter().enumerate() {
                if case.test.is_none() {
                    continue;
                }
                let test = match self.eval_expr(case.test.as_ref().unwrap(), &switch_env) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                if strict_equality(&disc, &test) {
                    if let Some(r) =
                        self.exec_switch_cases(&s.cases[b_start + i..], &switch_env, &mut v)
                    {
                        return r;
                    }
                    return Completion::Normal(v);
                }
            }

            // Phase 3: No match anywhere — execute default, then fall through B
            if let Some(r) = self.exec_switch_cases(&s.cases[di..], &switch_env, &mut v) {
                return r;
            }
        }

        Completion::Normal(v)
    }

    fn exec_switch_cases(
        &mut self,
        cases: &[crate::ast::SwitchCase],
        env: &EnvRef,
        v: &mut JsValue,
    ) -> Option<Completion> {
        for case in cases {
            for stmt in &case.consequent {
                match self.exec_statement(stmt, env) {
                    Completion::Normal(val) => {
                        *v = val;
                    }
                    Completion::Empty => {}
                    Completion::Break(None, break_val) => {
                        return Some(Completion::Normal(break_val.unwrap_or(JsValue::Undefined)));
                    }
                    other => return Some(other),
                }
            }
        }
        None
    }

    pub(crate) fn add_disposable_resource(
        &mut self,
        env: &EnvRef,
        value: &JsValue,
        hint: DisposeHint,
    ) -> Result<(), JsValue> {
        if matches!(value, JsValue::Null | JsValue::Undefined) {
            return Ok(());
        }

        let sym_name = if hint == DisposeHint::Async {
            "asyncDispose"
        } else {
            "dispose"
        };
        let sym_key = self.get_symbol_key(sym_name);

        let mut method = JsValue::Undefined;

        if let Some(ref key) = sym_key {
            if let JsValue::Object(o) = value {
                if let Some(obj) = self.get_object(o.id) {
                    let val = obj.borrow().get_property(key);
                    if !matches!(val, JsValue::Undefined) {
                        method = val;
                    }
                }
            }
        }

        if matches!(method, JsValue::Undefined) && hint == DisposeHint::Async {
            if let Some(sync_key) = self.get_symbol_key("dispose") {
                if let JsValue::Object(o) = value {
                    if let Some(obj) = self.get_object(o.id) {
                        let val = obj.borrow().get_property(&sync_key);
                        if !matches!(val, JsValue::Undefined) {
                            method = val;
                        }
                    }
                }
            }
        }

        if matches!(method, JsValue::Undefined) {
            return Err(
                self.create_type_error("Object is not disposable (missing [Symbol.dispose])")
            );
        }

        if !self.is_callable(&method) {
            return Err(self.create_type_error("[Symbol.dispose] is not a function"));
        }

        let resource = DisposableResource {
            value: value.clone(),
            hint,
            dispose_method: method,
        };

        let mut env_ref = env.borrow_mut();
        if env_ref.dispose_stack.is_none() {
            env_ref.dispose_stack = Some(Vec::new());
        }
        env_ref.dispose_stack.as_mut().unwrap().push(resource);
        Ok(())
    }

    pub(crate) fn dispose_resources(&mut self, env: &EnvRef, completion: Completion) -> Completion {
        let stack = env.borrow_mut().dispose_stack.take();
        let Some(mut stack) = stack else {
            return completion;
        };
        if stack.is_empty() {
            return completion;
        }

        stack.reverse();
        let mut current_error: Option<JsValue> = match &completion {
            Completion::Throw(e) => Some(e.clone()),
            _ => None,
        };
        let had_error = current_error.is_some();

        for resource in &stack {
            let result = self.call_function(&resource.dispose_method, &resource.value, &[]);
            match result {
                Completion::Normal(v) => {
                    if resource.hint == DisposeHint::Async {
                        match self.await_value(&v) {
                            Completion::Normal(_) => {}
                            Completion::Throw(e) => {
                                current_error = Some(self.wrap_suppressed_error(e, current_error));
                            }
                            _ => {}
                        }
                    }
                }
                Completion::Throw(e) => {
                    current_error = Some(self.wrap_suppressed_error(e, current_error));
                }
                _ => {}
            }
        }

        if let Some(err) = current_error {
            if had_error {
                Completion::Throw(err)
            } else {
                Completion::Throw(err)
            }
        } else {
            completion
        }
    }

    pub(crate) fn wrap_suppressed_error(
        &mut self,
        new_error: JsValue,
        existing: Option<JsValue>,
    ) -> JsValue {
        if let Some(existing_err) = existing {
            let env = self.global_env.clone();
            let args = vec![new_error, existing_err];
            match self.call_global_constructor("SuppressedError", &args, &env) {
                Completion::Normal(v) => v,
                _ => args[0].clone(),
            }
        } else {
            new_error
        }
    }

    pub(crate) fn call_global_constructor(
        &mut self,
        name: &str,
        args: &[JsValue],
        env: &EnvRef,
    ) -> Completion {
        let ctor = env.borrow().get(name);
        if let Some(ctor_val) = ctor {
            let new_obj = self.create_object();
            if let JsValue::Object(ref o) = ctor_val
                && let Some(func_obj) = self.get_object(o.id)
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
            let prev_new_target = self.new_target.take();
            self.new_target = Some(ctor_val.clone());
            let result = self.call_function(&ctor_val, &this_val, args);
            self.new_target = prev_new_target;
            match result {
                Completion::Normal(v) if matches!(v, JsValue::Object(_)) => Completion::Normal(v),
                Completion::Normal(_) => Completion::Normal(this_val),
                other => other,
            }
        } else {
            Completion::Throw(self.create_type_error(&format!("{name} is not defined")))
        }
    }
}
