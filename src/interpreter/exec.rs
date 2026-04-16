use super::*;

impl Interpreter {
    pub(crate) fn exec_statements(&mut self, stmts: &[Statement], env: &EnvRef) -> Completion {
        // Hoist var and function declarations
        let var_scope = Environment::find_var_scope(env);
        let is_global = var_scope.borrow().global_object.is_some();
        let is_block_scope = !Rc::ptr_eq(env, &var_scope);

        // §16.1.7 GlobalDeclarationInstantiation: pre-check CanDeclareGlobalFunction
        // and CanDeclareGlobalVar before any hoisting takes place.
        if is_global && let Some(err) = self.check_global_declarations(stmts, env) {
            return err;
        }

        for stmt in stmts {
            // Recursively hoist var declarations from all sub-statements
            self.hoist_vars_from_stmt(stmt, &var_scope, is_global);
            // Check for function declarations (including inside labels)
            if let Some(f) = Self::unwrap_labeled_function(stmt) {
                self.hoist_function_decl(f, env, is_global);
            }
        }

        // §14.2.2 BlockDeclarationInstantiation / §10.2.11 FunctionDeclarationInstantiation:
        // Hoist let/const/class declarations as uninitialized (TDZ) bindings
        Self::hoist_lexical_declarations(stmts, env);

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
                        && !top_level_var_names.contains(&name)
                        && name != "arguments";
                    if is_param {
                        continue;
                    }
                    // Annex B §B.3.3.1 step 22.f: skip "arguments" in function scopes
                    if name == "arguments" && !is_global && env.borrow().is_function_scope {
                        continue;
                    }
                    // Annex B: skip if non-simple params and name matches a parent binding
                    if !env.borrow().has_simple_params {
                        let has_parent_binding = env
                            .borrow()
                            .parent
                            .as_ref()
                            .map(|p| p.borrow().bindings.contains_key(&name))
                            .unwrap_or(false);
                        if has_parent_binding {
                            continue;
                        }
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
            self.gc_safepoint();
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

    pub(crate) fn unwrap_labeled_function(stmt: &Statement) -> Option<&FunctionDecl> {
        match stmt {
            Statement::FunctionDeclaration(f) => Some(f),
            Statement::Labeled(_, inner) => Self::unwrap_labeled_function(inner),
            _ => None,
        }
    }

    /// §9.1.1.4.16 CanDeclareGlobalFunction
    fn can_declare_global_function(global_obj: &Rc<RefCell<JsObjectData>>, name: &str) -> bool {
        let gb = global_obj.borrow();
        if let Some(desc) = gb.properties.get(name) {
            if desc.configurable == Some(true) {
                return true;
            }
            if desc.value.is_some() && desc.writable == Some(true) && desc.enumerable == Some(true)
            {
                return true;
            }
            false
        } else {
            gb.extensible
        }
    }

    /// §9.1.1.4.15 CanDeclareGlobalVar
    fn can_declare_global_var(global_obj: &Rc<RefCell<JsObjectData>>, name: &str) -> bool {
        let gb = global_obj.borrow();
        if gb.properties.contains_key(name) {
            return true;
        }
        gb.extensible
    }

    /// Collect lexical declaration names (let/const/class) from top-level statements.
    fn collect_lex_names(stmts: &[Statement]) -> Vec<String> {
        let mut names = Vec::new();
        for stmt in stmts {
            match stmt {
                Statement::Variable(decl) if matches!(decl.kind, VarKind::Let | VarKind::Const) => {
                    for d in &decl.declarations {
                        Self::collect_pattern_names(&d.pattern, &mut names);
                    }
                }
                Statement::ClassDeclaration(c) if !c.name.is_empty() => {
                    names.push(c.name.clone());
                }
                _ => {}
            }
        }
        names
    }

    /// §9.1.1.4.13 HasRestrictedGlobalProperty
    fn has_restricted_global_property(global_obj: &Rc<RefCell<JsObjectData>>, name: &str) -> bool {
        let gb = global_obj.borrow();
        if let Some(desc) = gb.properties.get(name) {
            desc.configurable == Some(false)
        } else {
            false
        }
    }

    /// Pre-check all global function/var/lex declarations per §16.1.7.
    /// Returns Some(Completion::Throw) if any check fails, None if all OK.
    fn check_global_declarations(
        &mut self,
        stmts: &[Statement],
        env: &EnvRef,
    ) -> Option<Completion> {
        let global_obj = env.borrow().global_object.clone();
        let global_obj = global_obj?;

        // §16.1.7 step 3: Check lexical names
        let lex_names = Self::collect_lex_names(stmts);
        for name in &lex_names {
            // Step 3a: If envRec.HasLexicalDeclaration(name) is true, throw SyntaxError
            if let Some(binding) = env.borrow().bindings.get(name)
                && matches!(binding.kind, BindingKind::Let | BindingKind::Const)
            {
                let err = self.create_error(
                    "SyntaxError",
                    &format!("Identifier '{}' has already been declared", name),
                );
                return Some(Completion::Throw(err));
            }
            // Step 3b-d: If HasRestrictedGlobalProperty(name) is true, throw SyntaxError
            // Non-eval var/function declarations create non-configurable global properties,
            // so this also catches var-lex collisions for non-eval declarations.
            if Self::has_restricted_global_property(&global_obj, name) {
                let err = self.create_error(
                    "SyntaxError",
                    &format!("Identifier '{}' has already been declared", name),
                );
                return Some(Completion::Throw(err));
            }
        }

        // §16.1.7 step 6: For each var name, check HasLexicalDeclaration
        let mut var_names = HashSet::default();
        Self::collect_var_names_from_stmts(stmts, &mut var_names);
        for name in &var_names {
            if let Some(binding) = env.borrow().bindings.get(name)
                && matches!(binding.kind, BindingKind::Let | BindingKind::Const)
            {
                let err = self.create_error(
                    "SyntaxError",
                    &format!("Identifier '{}' has already been declared", name),
                );
                return Some(Completion::Throw(err));
            }
        }

        // Collect function declaration names (step 10)
        let mut declared_function_names: Vec<String> = Vec::new();
        for stmt in stmts.iter().rev() {
            if let Some(f) = Self::unwrap_labeled_function(stmt)
                && !declared_function_names.contains(&f.name)
            {
                // Step 6 also applies to function decl names
                if let Some(binding) = env.borrow().bindings.get(&f.name)
                    && matches!(binding.kind, BindingKind::Let | BindingKind::Const)
                {
                    let err = self.create_error(
                        "SyntaxError",
                        &format!("Identifier '{}' has already been declared", f.name),
                    );
                    return Some(Completion::Throw(err));
                }
                if !Self::can_declare_global_function(&global_obj, &f.name) {
                    let err = self
                        .create_type_error(&format!("Cannot declare global function '{}'", f.name));
                    return Some(Completion::Throw(err));
                }
                declared_function_names.push(f.name.clone());
            }
        }

        // Collect var declaration names (step 12)
        for name in &var_names {
            if !declared_function_names.contains(name)
                && !Self::can_declare_global_var(&global_obj, name)
            {
                let err =
                    self.create_type_error(&format!("Cannot declare global variable '{}'", name));
                return Some(Completion::Throw(err));
            }
        }

        None
    }

    fn hoist_function_decl(&mut self, f: &FunctionDecl, env: &EnvRef, is_global: bool) {
        let enclosing_strict = env.borrow().strict;
        let func = JsFunction::User {
            name: Some(f.name.clone()),
            params: Rc::new(f.params.clone()),
            body: Rc::new(f.body.clone()),
            closure: env.clone(),
            is_arrow: false,
            is_strict: f.body_is_strict || enclosing_strict,
            is_generator: f.is_generator,
            is_async: f.is_async,
            is_method: false,
            source_text: f.source_text.clone(),
            captured_new_target: None,
        };
        let val = self.create_function(func);
        if is_global {
            // §16.1.7 step 17c: CreateGlobalFunctionBinding(fn, fo, false)
            env.borrow_mut()
                .declare_global_function_binding(&f.name, val, false);
        } else {
            env.borrow_mut().declare(&f.name, BindingKind::Var);
            let _ = env.borrow_mut().set(&f.name, val);
        }
    }

    /// §14.2.2 BlockDeclarationInstantiation: Hoist let/const/class declarations as
    /// uninitialized (TDZ) bindings at the top of a block scope.
    fn hoist_lexical_declarations(stmts: &[Statement], env: &EnvRef) {
        for stmt in stmts {
            match stmt {
                Statement::Variable(decl)
                    if matches!(
                        decl.kind,
                        VarKind::Let | VarKind::Const | VarKind::Using | VarKind::AwaitUsing
                    ) =>
                {
                    let kind = match decl.kind {
                        VarKind::Let => BindingKind::Let,
                        _ => BindingKind::Const,
                    };
                    for d in &decl.declarations {
                        let mut names = Vec::new();
                        Self::collect_pattern_names(&d.pattern, &mut names);
                        for name in names {
                            env.borrow_mut().declare(&name, kind);
                        }
                    }
                }
                Statement::ClassDeclaration(c) if !c.name.is_empty() => {
                    env.borrow_mut().declare(&c.name, BindingKind::Const);
                }
                _ => {}
            }
        }
    }

    pub(crate) fn collect_pattern_names(pat: &Pattern, names: &mut Vec<String>) {
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

    pub(crate) fn hoist_vars_from_stmt(
        &self,
        stmt: &Statement,
        var_scope: &EnvRef,
        is_global: bool,
    ) {
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
                if let Some(ForInit::Variable(decl)) = &f.init
                    && decl.kind == VarKind::Var
                {
                    for d in &decl.declarations {
                        self.hoist_pattern(&d.pattern, var_scope, is_global);
                    }
                }
                self.hoist_vars_from_stmt(&f.body, var_scope, is_global);
            }
            Statement::ForIn(fi) => {
                if let ForInOfLeft::Variable(decl) = &fi.left
                    && decl.kind == VarKind::Var
                {
                    for d in &decl.declarations {
                        self.hoist_pattern(&d.pattern, var_scope, is_global);
                    }
                }
                self.hoist_vars_from_stmt(&fi.body, var_scope, is_global);
            }
            Statement::ForOf(fo) => {
                if let ForInOfLeft::Variable(decl) = &fo.left
                    && decl.kind == VarKind::Var
                {
                    for d in &decl.declarations {
                        self.hoist_pattern(&d.pattern, var_scope, is_global);
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

    pub(crate) fn collect_var_names_from_pattern(pat: &Pattern, out: &mut HashSet<String>) {
        match pat {
            Pattern::Identifier(name) => {
                out.insert(name.clone());
            }
            Pattern::Array(elems) => {
                for elem in elems.iter().flatten() {
                    match elem {
                        ArrayPatternElement::Pattern(p) | ArrayPatternElement::Rest(p) => {
                            Self::collect_var_names_from_pattern(p, out);
                        }
                    }
                }
            }
            Pattern::Object(props) => {
                for prop in props {
                    match prop {
                        ObjectPatternProperty::KeyValue(_, p) | ObjectPatternProperty::Rest(p) => {
                            Self::collect_var_names_from_pattern(p, out);
                        }
                        ObjectPatternProperty::Shorthand(name) => {
                            out.insert(name.clone());
                        }
                    }
                }
            }
            Pattern::Assign(inner, _) | Pattern::Rest(inner) => {
                Self::collect_var_names_from_pattern(inner, out);
            }
            Pattern::MemberExpression(_) => {}
        }
    }

    pub(crate) fn collect_var_names_from_stmts(stmts: &[Statement], out: &mut HashSet<String>) {
        for stmt in stmts {
            Self::collect_var_names_from_stmt(stmt, out);
        }
    }

    fn collect_var_names_from_stmt(stmt: &Statement, out: &mut HashSet<String>) {
        match stmt {
            Statement::Variable(decl) if decl.kind == VarKind::Var => {
                for d in &decl.declarations {
                    Self::collect_var_names_from_pattern(&d.pattern, out);
                }
            }
            Statement::Block(stmts) => Self::collect_var_names_from_stmts(stmts, out),
            Statement::If(i) => {
                Self::collect_var_names_from_stmt(&i.consequent, out);
                if let Some(alt) = &i.alternate {
                    Self::collect_var_names_from_stmt(alt, out);
                }
            }
            Statement::While(w) => Self::collect_var_names_from_stmt(&w.body, out),
            Statement::DoWhile(d) => Self::collect_var_names_from_stmt(&d.body, out),
            Statement::For(f) => {
                if let Some(ForInit::Variable(decl)) = &f.init
                    && decl.kind == VarKind::Var
                {
                    for d in &decl.declarations {
                        Self::collect_var_names_from_pattern(&d.pattern, out);
                    }
                }
                Self::collect_var_names_from_stmt(&f.body, out);
            }
            Statement::ForIn(fi) => {
                if let ForInOfLeft::Variable(decl) = &fi.left
                    && decl.kind == VarKind::Var
                {
                    for d in &decl.declarations {
                        Self::collect_var_names_from_pattern(&d.pattern, out);
                    }
                }
                Self::collect_var_names_from_stmt(&fi.body, out);
            }
            Statement::ForOf(fo) => {
                if let ForInOfLeft::Variable(decl) = &fo.left
                    && decl.kind == VarKind::Var
                {
                    for d in &decl.declarations {
                        Self::collect_var_names_from_pattern(&d.pattern, out);
                    }
                }
                Self::collect_var_names_from_stmt(&fo.body, out);
            }
            Statement::Switch(sw) => {
                for case in &sw.cases {
                    Self::collect_var_names_from_stmts(&case.consequent, out);
                }
            }
            Statement::Try(t) => {
                Self::collect_var_names_from_stmts(&t.block, out);
                if let Some(handler) = &t.handler {
                    Self::collect_var_names_from_stmts(&handler.body, out);
                }
                if let Some(finalizer) = &t.finalizer {
                    Self::collect_var_names_from_stmts(finalizer, out);
                }
            }
            Statement::Labeled(_, inner) => Self::collect_var_names_from_stmt(inner, out),
            Statement::With(_, inner) => Self::collect_var_names_from_stmt(inner, out),
            _ => {}
        }
    }

    // Annex B.3.3: recursively find function declarations inside blocks
    // for var-scope hoisting at the function/global level.
    // `blocked` tracks lexical names from enclosing scopes that would
    // cause an early error if a var with the same name were declared.
    pub(crate) fn collect_annexb_function_names(
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
                        let mut stmt = s;
                        while let Statement::Labeled(_, inner_s) = stmt {
                            stmt = inner_s;
                        }
                        if let Statement::FunctionDeclaration(f) = stmt
                            && !f.is_generator
                            && !f.is_async
                            && !names.contains(&f.name)
                            && !blocked.contains(&f.name)
                            && !block_lexicals.contains(&f.name)
                        {
                            names.push(f.name.clone());
                        }
                    }
                    // Recurse with block lexicals and function decl names added to blocked set
                    let prev_len = blocked.len();
                    blocked.extend(block_lexicals);
                    for s in inner {
                        let mut stmt = s;
                        while let Statement::Labeled(_, inner_s) = stmt {
                            stmt = inner_s;
                        }
                        if let Statement::FunctionDeclaration(f) = stmt
                            && !blocked.contains(&f.name)
                        {
                            blocked.push(f.name.clone());
                        }
                    }
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
                    if let Some(ForInit::Variable(decl)) = &f.init
                        && matches!(decl.kind, VarKind::Let | VarKind::Const)
                    {
                        for d in &decl.declarations {
                            Self::collect_pattern_names(&d.pattern, blocked);
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
                    if let ForInOfLeft::Variable(decl) = &fi.left
                        && matches!(decl.kind, VarKind::Let | VarKind::Const)
                    {
                        for d in &decl.declarations {
                            Self::collect_pattern_names(&d.pattern, blocked);
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
                    if let ForInOfLeft::Variable(decl) = &fo.left
                        && matches!(decl.kind, VarKind::Let | VarKind::Const)
                    {
                        for d in &decl.declarations {
                            Self::collect_pattern_names(&d.pattern, blocked);
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
                Statement::With(_, inner) => {
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
                            let mut stmt = cs;
                            while let Statement::Labeled(_, inner_s) = stmt {
                                stmt = inner_s;
                            }
                            if let Statement::FunctionDeclaration(f) = stmt
                                && !f.is_generator
                                && !f.is_async
                                && !names.contains(&f.name)
                                && !blocked.contains(&f.name)
                                && !switch_lexicals.contains(&f.name)
                            {
                                names.push(f.name.clone());
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
                            // B.3.5: simple BindingIdentifier catch params
                            // do NOT block var redeclaration
                            if !matches!(param, Pattern::Identifier(_)) {
                                Self::collect_pattern_names(param, blocked);
                            }
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
                let has_async_dispose = self.in_state_machine
                    && stmts.iter().any(
                        |s| matches!(s, Statement::Variable(d) if d.kind == VarKind::AwaitUsing),
                    );
                let result = self.exec_statements(stmts, &block_env);
                let result = self.dispose_resources(&block_env, result);
                if has_async_dispose && !result.is_abrupt() {
                    self.pending_async_dispose_await = true;
                }
                result
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
                if self.to_boolean_val(&test) {
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
            Statement::For(f) => self.exec_for(f, env, None),
            Statement::ForIn(fi) => self.exec_for_in(fi, env, None),
            Statement::ForOf(fo) => self.exec_for_of(fo, env, None),
            Statement::Return(expr) => {
                let val = if let Some(e) = expr {
                    let can_tco = env.borrow().strict
                        && self.generator_context.is_none()
                        && !self.in_state_machine
                        && Self::expr_may_contain_tail_call(e);
                    if can_tco {
                        self.in_tail_position = true;
                    }
                    let result = self.eval_expr(e, env);
                    self.in_tail_position = false;
                    match result {
                        Completion::Normal(v) => v,
                        Completion::TailCall { .. } => return result,
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
                let comp = match stmt.as_ref() {
                    Statement::For(f) => self.exec_for(f, env, Some(label)),
                    Statement::ForIn(fi) => self.exec_for_in(fi, env, Some(label)),
                    Statement::ForOf(fo) => self.exec_for_of(fo, env, Some(label)),
                    Statement::While(_) | Statement::DoWhile(_) => {
                        self.exec_labeled_loop(stmt, env, label)
                    }
                    _ => self.exec_statement(stmt, env),
                };
                match &comp {
                    Completion::Break(Some(l), val) if l == label => {
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
                        let with_env = Rc::new(RefCell::new(Environment {
                            bindings: HashMap::default(),
                            parent: Some(env.clone()),
                            strict: env.borrow().strict,
                            is_function_scope: false,
                            is_arrow_scope: false,
                            with_object: Some(WithObject {
                                _object: obj_data,
                                obj_id: obj_ref.id,
                            }),
                            dispose_stack: None,
                            global_object: None,
                            annexb_function_names: None,
                            class_private_names: None,
                            is_field_initializer: false,
                            arguments_immutable: false,
                            has_parameter_expressions: false,
                            has_simple_params: true,
                            is_simple_catch_scope: false,
                            is_derived_constructor_scope: false,
                            indirect_bindings: None,
                            module_path: None,
                        }));
                        self.with_scope_depth += 1;
                        self.has_ever_entered_with = true;
                        let c = self.exec_statement(body, &with_env);
                        self.with_scope_depth -= 1;
                        // UpdateEmpty(C, undefined) per §14.11.2 step 9
                        match c {
                            Completion::Empty => Completion::Normal(JsValue::Undefined),
                            Completion::Break(label, None) => {
                                Completion::Break(label, Some(JsValue::Undefined))
                            }
                            Completion::Continue(label, None) => {
                                Completion::Continue(label, Some(JsValue::Undefined))
                            }
                            other => other,
                        }
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
                            // Check intermediate scopes between env's parent and var_scope;
                            // if any has a binding for the same name (except simple catch scopes),
                            // skip the Annex B write.
                            let mut blocked_by_intermediate = false;
                            let mut cursor = env.borrow().parent.clone();
                            while let Some(cur) = cursor {
                                if Rc::ptr_eq(&cur, &var_scope) {
                                    break;
                                }
                                let cur_b = cur.borrow();
                                if cur_b.bindings.contains_key(&f.name)
                                    && !cur_b.is_simple_catch_scope
                                {
                                    blocked_by_intermediate = true;
                                    break;
                                }
                                cursor = cur_b.parent.clone();
                            }
                            if !blocked_by_intermediate {
                                let val = env.borrow().get(&f.name).unwrap_or(JsValue::Undefined);
                                let _ = var_scope.borrow_mut().set(&f.name, val);
                            }
                        }
                    }
                }
                Completion::Empty
            }
            Statement::ClassDeclaration(cd) => {
                let class_val = self.eval_class(
                    &cd.name,
                    &cd.name,
                    &cd.super_class,
                    &cd.body,
                    env,
                    cd.source_text.clone(),
                );
                match class_val {
                    Completion::Normal(val) => {
                        env.borrow_mut().declare(&cd.name, BindingKind::Let);
                        env.borrow_mut().initialize_binding(&cd.name, val);
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
                && matches!(d.pattern, Pattern::Identifier(_))
            {
                continue;
            }
            // §13.3.2.4 step 2: ResolveBinding BEFORE evaluating Initializer
            // Pre-resolve with-scope binding so the reference is captured before
            // side effects in the initializer can change it.
            let pre_resolved_with = if (self.with_scope_depth > 0 || self.has_ever_entered_with)
                && decl.kind == VarKind::Var
                && d.init.is_some()
                && let Pattern::Identifier(ref name) = d.pattern
            {
                match self.resolve_with_has_binding(name, env) {
                    Ok(obj_id) => obj_id,
                    Err(e) => return Completion::Throw(e),
                }
            } else {
                None
            };
            let val = if let Some(init) = &d.init {
                // For anonymous class expressions, pass the binding name into
                // eval_class so SetFunctionName happens before static fields.
                if let Pattern::Identifier(ref name) = d.pattern
                    && let Expression::Class(ce) = init
                    && ce.name.is_none()
                {
                    match self.eval_class(
                        name,
                        "",
                        &ce.super_class,
                        &ce.body,
                        env,
                        ce.source_text.clone(),
                    ) {
                        Completion::Normal(v) => v,
                        other => return other,
                    }
                } else {
                    match self.eval_expr(init, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    }
                }
            } else {
                JsValue::Undefined
            };
            if let Pattern::Identifier(ref name) = d.pattern
                && d.init
                    .as_ref()
                    .is_some_and(|e| e.is_anonymous_function_definition())
            {
                // Skip for class expressions — name already set in eval_class
                if !d
                    .init
                    .as_ref()
                    .is_some_and(|e| matches!(e, Expression::Class(ce) if ce.name.is_none()))
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
            // Use pre-resolved with-scope binding if available
            if let Some(with_obj_id) = pre_resolved_with {
                if let Pattern::Identifier(ref name) = d.pattern {
                    let strict = env.borrow().strict;
                    if let Err(e) = self.with_set_mutable_binding(with_obj_id, name, val, strict) {
                        return Completion::Throw(e);
                    }
                } else if let Err(e) = self.bind_pattern(&d.pattern, val, kind, env) {
                    return Completion::Throw(e);
                }
            } else if let Err(e) = self.bind_pattern(&d.pattern, val, kind, env) {
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
                    let already_declared = {
                        let vs = var_scope.borrow();
                        vs.bindings.contains_key(name)
                            || vs
                                .global_object
                                .as_ref()
                                .is_some_and(|g| g.borrow().properties.contains_key(name))
                    };
                    if !already_declared {
                        var_scope.borrow_mut().declare(name, kind);
                    }
                    // For var initializers inside with-scopes, write through with-object
                    if self.with_scope_depth > 0 || self.has_ever_entered_with {
                        match self.resolve_with_has_binding(name, env) {
                            Ok(Some(obj_id)) => {
                                let strict = env.borrow().strict;
                                self.with_set_mutable_binding(obj_id, name, val, strict)
                            }
                            Ok(None) => env.borrow_mut().set(name, val),
                            Err(e) => Err(e),
                        }
                    } else {
                        env.borrow_mut().set(name, val)
                    }
                } else {
                    env.borrow_mut().declare(name, kind);
                    env.borrow_mut().initialize_binding(name, val);
                    Ok(())
                }
            }
            Pattern::Assign(inner, default) => {
                let v = if val.is_undefined() {
                    // Pre-declare as TDZ before evaluating default so self-references throw
                    if let Pattern::Identifier(ref name) = **inner {
                        let target = if kind == BindingKind::Var {
                            Environment::find_var_scope(env)
                        } else {
                            env.clone()
                        };
                        if !target.borrow().bindings.contains_key(name) {
                            target.borrow_mut().bindings.insert(
                                name.to_string(),
                                Binding {
                                    value: JsValue::Undefined,
                                    kind,
                                    initialized: false,
                                    deletable: false,
                                },
                            );
                        }
                    }
                    match self.eval_expr(default, env) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Err(e),
                        _ => JsValue::Undefined,
                    }
                } else {
                    val
                };
                if let Pattern::Identifier(ref name) = **inner
                    && default.is_anonymous_function_definition()
                {
                    self.set_function_name(&v, name);
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
                    if let JsValue::Object(o) = &iterator
                        && let Some(pos) = s.gc_temp_roots.iter().rposition(|&id| id == o.id)
                    {
                        s.gc_temp_roots.remove(pos);
                    }
                };
                if let Some(err) = error {
                    if !done {
                        let _ = self.iterator_close_result(&iterator);
                    }
                    unroot_iter(self);
                    return Err(err);
                }
                if !done {
                    let r = self.iterator_close_result(&iterator);
                    unroot_iter(self);
                    return r;
                }
                unroot_iter(self);
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
                                env.borrow_mut().set(name, v)?;
                            } else {
                                env.borrow_mut().declare(name, kind);
                                env.borrow_mut().initialize_binding(name, v);
                            }
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

                            // Spec §14.3.3.3: For SingleNameBinding, ResolveBinding
                            // must happen BEFORE GetV (property access).
                            let single_name = match pat {
                                Pattern::Identifier(n) => Some((n.as_str(), None::<&Expression>)),
                                Pattern::Assign(inner, dflt)
                                    if matches!(**inner, Pattern::Identifier(_)) =>
                                {
                                    if let Pattern::Identifier(n) = &**inner {
                                        Some((n.as_str(), Some(dflt.as_ref())))
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            };

                            if let Some((binding_name, default_expr)) = single_name
                                && kind == BindingKind::Var
                            {
                                // Step 2: ResolveBinding(bindingId, environment)
                                let var_scope = Environment::find_var_scope(env);
                                let already = {
                                    let vs = var_scope.borrow();
                                    vs.bindings.contains_key(binding_name)
                                        || vs.global_object.as_ref().is_some_and(|g| {
                                            g.borrow().properties.contains_key(binding_name)
                                        })
                                };
                                if !already {
                                    var_scope.borrow_mut().declare(binding_name, kind);
                                }
                                let resolved =
                                    if self.with_scope_depth > 0 || self.has_ever_entered_with {
                                        self.resolve_with_has_binding(binding_name, env)?
                                    } else {
                                        None
                                    };

                                // Step 3: v = GetV(value, propertyName)
                                let mut v = if let JsValue::Object(o) = &obj_val {
                                    match self.get_object_property(o.id, &key_str, &obj_val) {
                                        Completion::Normal(v) => v,
                                        Completion::Throw(e) => return Err(e),
                                        _ => JsValue::Undefined,
                                    }
                                } else {
                                    JsValue::Undefined
                                };

                                // Step 4: If v undefined and initializer present, evaluate
                                if v.is_undefined()
                                    && let Some(dflt) = default_expr
                                {
                                    v = match self.eval_expr(dflt, env) {
                                        Completion::Normal(v) => v,
                                        Completion::Throw(e) => return Err(e),
                                        _ => JsValue::Undefined,
                                    };
                                    if dflt.is_anonymous_function_definition() {
                                        self.set_function_name(&v, binding_name);
                                    }
                                }

                                // Step 6: InitializeReferencedBinding(lhs, v)
                                let strict = env.borrow().strict;
                                match resolved {
                                    Some(obj_id) => self.with_set_mutable_binding(
                                        obj_id,
                                        binding_name,
                                        v,
                                        strict,
                                    )?,
                                    None => env.borrow_mut().set(binding_name, v)?,
                                }
                            } else {
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
                        }
                        ObjectPatternProperty::Rest(pat) => {
                            let rest_obj = self.create_object();
                            if let JsValue::Object(o) = &obj_val {
                                let pairs =
                                    self.copy_data_properties(o.id, &obj_val, &excluded_keys)?;
                                for (k, v) in pairs {
                                    rest_obj.borrow_mut().insert_value(k, v);
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
            self.gc_safepoint();
            let test = match self.eval_expr(&w.test, env) {
                Completion::Normal(v) => v,
                other => return other,
            };
            if !self.to_boolean_val(&test) {
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
            self.gc_safepoint();
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
            if !self.to_boolean_val(&test) {
                break;
            }
        }
        Completion::Normal(v)
    }

    fn exec_labeled_loop(&mut self, stmt: &Statement, env: &EnvRef, label: &str) -> Completion {
        match stmt {
            Statement::While(w) => {
                let mut v = JsValue::Undefined;
                loop {
                    self.gc_safepoint();
                    let test = match self.eval_expr(&w.test, env) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    if !self.to_boolean_val(&test) {
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
                        Completion::Continue(Some(ref l), cont_val) if l == label => {
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
            Statement::DoWhile(dw) => {
                let mut v = JsValue::Undefined;
                loop {
                    self.gc_safepoint();
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
                        Completion::Continue(Some(ref l), cont_val) if l == label => {
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
                    if !self.to_boolean_val(&test) {
                        break;
                    }
                }
                Completion::Normal(v)
            }
            _ => unreachable!("exec_labeled_loop called with non-loop statement"),
        }
    }

    fn exec_for(&mut self, f: &ForStatement, env: &EnvRef, label: Option<&str>) -> Completion {
        let for_env = Environment::new(Some(env.clone()));
        // Collect lexical binding names for per-iteration freshness (§14.7.4.2)
        let per_iteration_bindings: Vec<(String, BindingKind)> =
            if let Some(ForInit::Variable(decl)) = &f.init
                && matches!(decl.kind, VarKind::Let | VarKind::Const)
            {
                let kind = if decl.kind == VarKind::Let {
                    BindingKind::Let
                } else {
                    BindingKind::Const
                };
                let mut names = Vec::new();
                for d in &decl.declarations {
                    Self::collect_pattern_names(&d.pattern, &mut names);
                }
                names.into_iter().map(|n| (n, kind)).collect()
            } else {
                Vec::new()
            };

        if let Some(init) = &f.init {
            match init {
                ForInit::Variable(decl) => {
                    let decl_env = if decl.kind == VarKind::Var {
                        env
                    } else {
                        &for_env
                    };
                    let comp = self.exec_variable_declaration(decl, decl_env);
                    if comp.is_abrupt() {
                        return self.dispose_resources(&for_env, comp);
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

        // Per-iteration environment creation (§14.7.4.2 step 3.e CreatePerIterationEnvironment)
        // For let bindings, create a fresh environment each iteration with copies of current values
        let mut iter_env = if !per_iteration_bindings.is_empty() {
            let new_env = Environment::new(Some(env.clone()));
            for (name, kind) in &per_iteration_bindings {
                let val = for_env.borrow().get(name).unwrap_or(JsValue::Undefined);
                new_env.borrow_mut().declare(name, *kind);
                new_env.borrow_mut().initialize_binding(name, val);
            }
            new_env
        } else {
            for_env.clone()
        };

        let mut v = JsValue::Undefined;
        let result = 'for_loop: {
            loop {
                self.gc_safepoint();
                if let Some(test) = &f.test {
                    let val = match self.eval_expr(test, &iter_env) {
                        Completion::Normal(v) => v,
                        other => break 'for_loop other,
                    };
                    if !self.to_boolean_val(&val) {
                        break;
                    }
                }
                match self.exec_statement(&f.body, &iter_env) {
                    Completion::Normal(val) => {
                        v = val;
                    }
                    Completion::Empty => {}
                    Completion::Continue(None, cont_val) => {
                        if let Some(val) = cont_val {
                            v = val;
                        }
                    }
                    Completion::Continue(Some(ref l), cont_val) if label == Some(l.as_str()) => {
                        if let Some(val) = cont_val {
                            v = val;
                        }
                    }
                    Completion::Break(None, break_val) => {
                        if let Some(val) = break_val {
                            v = val;
                        }
                        break;
                    }
                    other => break 'for_loop other,
                }
                // CreatePerIterationEnvironment before update
                if !per_iteration_bindings.is_empty() {
                    let new_env = Environment::new(Some(env.clone()));
                    for (name, kind) in &per_iteration_bindings {
                        let val = iter_env.borrow().get(name).unwrap_or(JsValue::Undefined);
                        new_env.borrow_mut().declare(name, *kind);
                        new_env.borrow_mut().initialize_binding(name, val);
                    }
                    iter_env = new_env;
                }
                if let Some(update) = &f.update {
                    let comp = self.eval_expr(update, &iter_env);
                    if comp.is_abrupt() {
                        break 'for_loop comp;
                    }
                }
            }
            Completion::Normal(v)
        };
        if !per_iteration_bindings.is_empty() {
            self.dispose_resources(&iter_env, result)
        } else {
            self.dispose_resources(&for_env, result)
        }
    }

    fn exec_for_in(
        &mut self,
        fi: &ForInStatement,
        env: &EnvRef,
        label: Option<&str>,
    ) -> Completion {
        // Annex B: for-in initializer (sloppy mode var declarations only)
        if let ForInOfLeft::Variable(decl) = &fi.left
            && decl.kind == VarKind::Var
            && let Some(d) = decl.declarations.first()
            && let Some(init_expr) = &d.init
        {
            let init_val = match self.eval_expr(init_expr, env) {
                Completion::Normal(v) => v,
                other => return other,
            };
            if let Pattern::Identifier(name) = &d.pattern {
                self.set_function_name(&init_val, name);
                let _ = env.borrow_mut().set(name, init_val);
            }
        }

        // Per spec 14.7.5.6 ForIn/OfHeadEvaluation:
        // If the LHS is a lexical declaration, create TDZ bindings before evaluating RHS
        let is_lexical =
            matches!(&fi.left, ForInOfLeft::Variable(decl) if decl.kind != VarKind::Var);
        let eval_env = if is_lexical {
            let tdz_env = Environment::new(Some(env.clone()));
            if let ForInOfLeft::Variable(decl) = &fi.left
                && let Some(d) = decl.declarations.first()
            {
                let mut names = Vec::new();
                Self::collect_pattern_names(&d.pattern, &mut names);
                for name in &names {
                    tdz_env.borrow_mut().declare(name, BindingKind::Let);
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
                let needs_proxy_path = self
                    .get_object(obj_id)
                    .map(|obj| {
                        let b = obj.borrow();
                        b.is_proxy() || b.module_namespace.is_some()
                    })
                    .unwrap_or(false);
                if needs_proxy_path {
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
                self.gc_safepoint();
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
                    ForInOfLeft::Expression(expr) => {
                        match expr {
                            Expression::Identifier(_) => {
                                if let Err(e) = self.assign_to_expr(expr, key_val, env) {
                                    return Completion::Throw(e);
                                }
                            }
                            Expression::Member(..) => {
                                if let Err(e) = self.assign_to_expr(expr, key_val, env) {
                                    return Completion::Throw(e);
                                }
                            }
                            _ => {
                                // Evaluate for side effects, then throw ReferenceError
                                match self.eval_expr(expr, env) {
                                    Completion::Normal(_) => {}
                                    other => return other,
                                }
                                return Completion::Throw(self.create_reference_error(
                                    "Invalid left-hand side in for-in loop",
                                ));
                            }
                        }
                    }
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
                    Completion::Continue(Some(ref l), cont_val) if label == Some(l.as_str()) => {
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

    fn collect_for_decl_bound_names(left: &ForInOfLeft) -> Vec<String> {
        let mut names = Vec::new();
        if let ForInOfLeft::Variable(decl) = left
            && matches!(
                decl.kind,
                VarKind::Let | VarKind::Const | VarKind::Using | VarKind::AwaitUsing
            )
        {
            for d in &decl.declarations {
                Self::collect_pattern_bound_names(&d.pattern, &mut names);
            }
        }
        names
    }

    fn collect_pattern_bound_names(pat: &Pattern, names: &mut Vec<String>) {
        match pat {
            Pattern::Identifier(n) => names.push(n.clone()),
            Pattern::Array(elems) => {
                for elem in elems.iter().flatten() {
                    match elem {
                        ArrayPatternElement::Pattern(p) | ArrayPatternElement::Rest(p) => {
                            Self::collect_pattern_bound_names(p, names);
                        }
                    }
                }
            }
            Pattern::Object(props) => {
                for prop in props {
                    match prop {
                        ObjectPatternProperty::KeyValue(_, p) | ObjectPatternProperty::Rest(p) => {
                            Self::collect_pattern_bound_names(p, names);
                        }
                        ObjectPatternProperty::Shorthand(n) => names.push(n.clone()),
                    }
                }
            }
            Pattern::Assign(p, _) | Pattern::Rest(p) => Self::collect_pattern_bound_names(p, names),
            Pattern::MemberExpression(_) => {}
        }
    }

    fn exec_for_of(
        &mut self,
        fo: &ForOfStatement,
        env: &EnvRef,
        label: Option<&str>,
    ) -> Completion {
        // §14.7.5.12 ForIn/OfHeadEvaluation: create TDZ env for bound names
        let tdz_names = Self::collect_for_decl_bound_names(&fo.left);
        let eval_env = if !tdz_names.is_empty() {
            let tdz_env = Environment::new(Some(env.clone()));
            for name in &tdz_names {
                tdz_env.borrow_mut().bindings.insert(
                    name.clone(),
                    Binding {
                        value: JsValue::Undefined,
                        kind: BindingKind::Let,
                        initialized: false,
                        deletable: false,
                    },
                );
            }
            tdz_env
        } else {
            env.clone()
        };

        let iterable = match self.eval_expr(&fo.right, &eval_env) {
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
        let result = self.exec_for_of_loop(fo, env, &iterator, label);
        self.gc_unroot_value(&iterator);
        result
    }

    fn exec_for_of_loop(
        &mut self,
        fo: &ForOfStatement,
        env: &EnvRef,
        iterator: &JsValue,
        loop_label: Option<&str>,
    ) -> Completion {
        let mut v = JsValue::Undefined;
        loop {
            self.gc_safepoint();
            let step_result = match self.iterator_next(iterator) {
                Ok(v) => v,
                Err(e) => return Completion::Throw(e),
            };
            let step_result = if fo.is_await {
                match self.await_value(&step_result) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => {
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
                            self.iterator_close(iterator, e.clone());
                            return Completion::Throw(e);
                        }
                    }
                    if let Some(d) = decl.declarations.first()
                        && let Err(e) = self.bind_pattern(&d.pattern, val, kind, bind_env)
                    {
                        self.iterator_close(iterator, e.clone());
                        return Completion::Throw(e);
                    }
                }
                ForInOfLeft::Pattern(pat) => match self.assign_to_for_pattern(pat, val, env) {
                    Completion::Normal(_) | Completion::Empty => {}
                    Completion::Throw(e) => {
                        self.iterator_close(iterator, e.clone());
                        return Completion::Throw(e);
                    }
                    other => return other,
                },
                ForInOfLeft::Expression(expr) => match expr {
                    Expression::Identifier(_) | Expression::Member(..) => {
                        if let Err(e) = self.assign_to_expr(expr, val, env) {
                            self.iterator_close(iterator, e.clone());
                            return Completion::Throw(e);
                        }
                    }
                    _ => {
                        match self.eval_expr(expr, env) {
                            Completion::Normal(_) => {}
                            Completion::Throw(e) => {
                                self.iterator_close(iterator, e.clone());
                                return Completion::Throw(e);
                            }
                            other => return other,
                        }
                        let e =
                            self.create_reference_error("Invalid left-hand side in for-of loop");
                        self.iterator_close(iterator, e.clone());
                        return Completion::Throw(e);
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
                    if let Err(e) = self.iterator_close_result(iterator) {
                        return Completion::Throw(e);
                    }
                    return Completion::Normal(v);
                }
                Completion::Return(ret_v) => {
                    if let Err(e) = self.iterator_close_result(iterator) {
                        return Completion::Throw(e);
                    }
                    return Completion::Return(ret_v);
                }
                Completion::Throw(e) => {
                    self.iterator_close(iterator, e.clone());
                    return Completion::Throw(e);
                }
                Completion::Break(Some(label), val) => {
                    if let Err(e) = self.iterator_close_result(iterator) {
                        return Completion::Throw(e);
                    }
                    return Completion::Break(Some(label), val);
                }
                Completion::Continue(Some(lbl), val) => {
                    if loop_label == Some(lbl.as_str()) {
                        if let Some(v2) = val {
                            v = v2;
                        }
                    } else {
                        if let Err(e) = self.iterator_close_result(iterator) {
                            return Completion::Throw(e);
                        }
                        return Completion::Continue(Some(lbl), val);
                    }
                }
                other => return other,
            }
        }
        Completion::Normal(v)
    }

    fn exec_try(&mut self, t: &TryStatement, env: &EnvRef) -> Completion {
        let block_env = Environment::new(Some(env.clone()));
        let result = self.exec_statements(&t.block, &block_env);
        // §14.2.2: DisposeResources for the try block's scope (using declarations)
        let result = self.dispose_resources(&block_env, result);
        let result = match result {
            Completion::Throw(val) => {
                if let Some(handler) = &t.handler {
                    let catch_env = Environment::new(Some(env.clone()));
                    if let Some(param) = &handler.param {
                        if matches!(param, Pattern::Identifier(_)) {
                            catch_env.borrow_mut().is_simple_catch_scope = true;
                        }
                        if let Err(e) = self.bind_pattern(param, val, BindingKind::Let, &catch_env)
                        {
                            return Completion::Throw(e);
                        }
                    }
                    let catch_block_env = Environment::new(Some(catch_env.clone()));
                    self.exec_statements(&handler.body, &catch_block_env)
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
                let unwrapped = Self::unwrap_labeled_function(stmt);
                if let Some(f) = unwrapped
                    && !f.is_generator
                    && !f.is_async
                {
                    switch_env.borrow_mut().declare(&f.name, BindingKind::Var);
                    let enclosing_strict = switch_env.borrow().strict;
                    let func = JsFunction::User {
                        name: Some(f.name.clone()),
                        params: Rc::new(f.params.clone()),
                        body: Rc::new(f.body.clone()),
                        closure: switch_env.clone(),
                        is_arrow: false,
                        is_strict: f.body_is_strict || enclosing_strict,
                        is_generator: f.is_generator,
                        is_async: f.is_async,
                        is_method: false,
                        source_text: f.source_text.clone(),
                        captured_new_target: None,
                    };
                    let val = self.create_function(func);
                    let _ = switch_env.borrow_mut().set(&f.name, val);
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
                    let r = self.dispose_resources(&switch_env, r);
                    return r;
                }
                if let Some(di) = default_idx
                    && let Some(r) = self.exec_switch_cases(&s.cases[di..], &switch_env, &mut v)
                {
                    let r = self.dispose_resources(&switch_env, r);
                    return r;
                }
                return self.dispose_resources(&switch_env, Completion::Normal(v));
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
                        let r = self.dispose_resources(&switch_env, r);
                        return r;
                    }
                    return self.dispose_resources(&switch_env, Completion::Normal(v));
                }
            }

            // Phase 3: No match anywhere — execute default, then fall through B
            if let Some(r) = self.exec_switch_cases(&s.cases[di..], &switch_env, &mut v) {
                let r = self.dispose_resources(&switch_env, r);
                return r;
            }
        }

        self.dispose_resources(&switch_env, Completion::Normal(v))
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
                        return Some(Completion::Normal(break_val.unwrap_or_else(|| v.clone())));
                    }
                    other => return Some(other.update_empty(v.clone())),
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
        // §AddDisposableResource step 1a: early return for null/undefined only for sync-dispose
        if matches!(value, JsValue::Null | JsValue::Undefined) {
            if hint == DisposeHint::Sync {
                return Ok(());
            }
            // For async-dispose, we still need a resource record with method=undefined
            // so that DisposeResources will perform Await(undefined) per §10.4.4.3 Dispose step 3
            let resource = DisposableResource {
                value: JsValue::Undefined,
                hint,
                dispose_method: JsValue::Undefined,
            };
            let mut env_ref = env.borrow_mut();
            if env_ref.dispose_stack.is_none() {
                env_ref.dispose_stack = Some(Vec::new());
            }
            env_ref.dispose_stack.as_mut().unwrap().push(resource);
            return Ok(());
        }

        let sym_name = if hint == DisposeHint::Async {
            "asyncDispose"
        } else {
            "dispose"
        };
        let sym_key = self.get_symbol_key(sym_name);

        let mut method = JsValue::Undefined;

        if let Some(ref key) = sym_key
            && let JsValue::Object(o) = value
        {
            let obj_id = o.id;
            match self.get_object_property(obj_id, key, value) {
                Completion::Normal(v) if !matches!(v, JsValue::Undefined | JsValue::Null) => {
                    method = v;
                }
                Completion::Throw(e) => return Err(e),
                _ => {}
            }
        }

        if matches!(method, JsValue::Undefined) && hint == DisposeHint::Async {
            let sync_key = self.get_symbol_key("dispose");
            if let Some(ref key) = sync_key
                && let JsValue::Object(o) = value
            {
                let obj_id = o.id;
                match self.get_object_property(obj_id, key, value) {
                    Completion::Normal(v) if !matches!(v, JsValue::Undefined | JsValue::Null) => {
                        method = v;
                    }
                    Completion::Throw(e) => return Err(e),
                    _ => {}
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
        let _had_error = current_error.is_some();

        for resource in &stack {
            // §10.4.4.3 Dispose: If method is undefined, result is undefined; else Call(method, V)
            let result = if matches!(resource.dispose_method, JsValue::Undefined) {
                Completion::Normal(JsValue::Undefined)
            } else {
                self.call_function(&resource.dispose_method, &resource.value, &[])
            };
            match result {
                Completion::Normal(v) if resource.hint == DisposeHint::Async => {
                    match self.await_value(&v) {
                        Completion::Normal(_) => {}
                        Completion::Throw(e) => {
                            current_error = Some(self.wrap_suppressed_error(e, current_error));
                        }
                        _ => {}
                    }
                }
                Completion::Throw(e) => {
                    current_error = Some(self.wrap_suppressed_error(e, current_error));
                }
                _ => {}
            }
        }

        if let Some(err) = current_error {
            Completion::Throw(err)
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
            let env = self.realm().global_env.clone();
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

    fn expr_may_contain_tail_call(expr: &Expression) -> bool {
        match expr {
            Expression::Call(_, _) | Expression::TaggedTemplate(_, _) => true,
            Expression::Conditional(_, cons, alt) => {
                Self::expr_may_contain_tail_call(cons) || Self::expr_may_contain_tail_call(alt)
            }
            Expression::Logical(_, _, right) => Self::expr_may_contain_tail_call(right),
            Expression::Sequence(exprs) | Expression::Comma(exprs) => {
                exprs.last().is_some_and(Self::expr_may_contain_tail_call)
            }
            _ => false,
        }
    }
}
