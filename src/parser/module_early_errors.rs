//! Module early errors (ECMAScript 16.2.1.1): lexical/var name clashes and
//! exported bindings, checked once the whole ModuleItemList has been parsed.

use super::*;

impl<'a> Parser<'a> {
    /// §16.2.1.1 Static Semantics: Early Errors for Module
    pub(super) fn validate_module_early_errors(
        &self,
        items: &[ModuleItem],
    ) -> Result<(), ParseError> {
        use HashSet;

        let mut lex_names: HashSet<String> = HashSet::new();
        let mut var_names: HashSet<String> = HashSet::new();
        let mut exported_bindings: HashSet<String> = HashSet::new();

        // Collect all lexically-declared and var-declared names
        for item in items {
            match item {
                ModuleItem::Statement(stmt) => {
                    self.collect_module_var_names(stmt, &mut var_names);
                    self.collect_module_lex_names(stmt, &mut lex_names)?;
                }
                ModuleItem::ExportDeclaration(export) => {
                    self.collect_export_var_names(export, &mut var_names);
                    self.collect_export_lex_names(export, &mut lex_names)?;
                    self.collect_exported_bindings(export, &mut exported_bindings);
                }
                ModuleItem::ImportDeclaration(import) => {
                    for spec in &import.specifiers {
                        let name = match spec {
                            ImportSpecifier::Default(n, _) => n,
                            ImportSpecifier::Named { local, .. } => local,
                            ImportSpecifier::Namespace(n) => n,
                            ImportSpecifier::DeferredNamespace(n) => n,
                            ImportSpecifier::SourcePhase(n) => n,
                        };
                        // §13.1.1: import binding cannot be "arguments" or "eval"
                        if name == "arguments" || name == "eval" {
                            return Err(self.error(format!(
                                "'{name}' cannot be used as an import binding in module code"
                            )));
                        }
                        if !lex_names.insert(name.clone()) {
                            return Err(
                                self.error(format!("Duplicate binding '{name}' in module scope"))
                            );
                        }
                    }
                }
            }
        }

        // Check: LexicallyDeclaredNames ∩ VarDeclaredNames must be empty
        for name in &lex_names {
            if var_names.contains(name) {
                return Err(self.error(format!("Identifier '{name}' has already been declared")));
            }
        }

        // ExportedBindings must all be in VarDeclaredNames ∪ LexicallyDeclaredNames
        for name in &exported_bindings {
            if !var_names.contains(name) && !lex_names.contains(name) {
                return Err(self.error(format!("Export '{name}' is not defined")));
            }
        }

        Ok(())
    }

    pub(super) fn collect_module_var_names(&self, stmt: &Statement, names: &mut HashSet<String>) {
        let mut declared = Vec::new();
        Self::collect_var_declared_names(stmt, &mut declared);
        names.extend(declared);
    }

    pub(super) fn collect_module_lex_names(
        &self,
        stmt: &Statement,
        names: &mut HashSet<String>,
    ) -> Result<(), ParseError> {
        match stmt {
            Statement::Variable(decl) if decl.kind != VarKind::Var => {
                for d in &decl.declarations {
                    let mut n = Vec::new();
                    d.pattern.bound_names(&mut n);
                    for name in n {
                        if !names.insert(name.clone()) {
                            return Err(self
                                .error(format!("Identifier '{name}' has already been declared")));
                        }
                    }
                }
            }
            // In modules, function/class/generator declarations are lexically scoped
            Statement::FunctionDeclaration(f)
                if !f.name.is_empty() && !names.insert(f.name.clone()) =>
            {
                return Err(
                    self.error(format!("Identifier '{}' has already been declared", f.name))
                );
            }
            Statement::ClassDeclaration(c)
                if !c.name.is_empty() && !names.insert(c.name.clone()) =>
            {
                return Err(
                    self.error(format!("Identifier '{}' has already been declared", c.name))
                );
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn collect_export_var_names(
        &self,
        export: &ExportDeclaration,
        names: &mut HashSet<String>,
    ) {
        if let ExportDeclaration::Named {
            declaration: Some(decl),
            ..
        } = export
        {
            self.collect_module_var_names(decl, names);
        }
    }

    pub(super) fn collect_export_lex_names(
        &self,
        export: &ExportDeclaration,
        names: &mut HashSet<String>,
    ) -> Result<(), ParseError> {
        match export {
            ExportDeclaration::Named {
                declaration: Some(decl),
                ..
            } => {
                self.collect_module_lex_names(decl, names)?;
            }
            ExportDeclaration::DefaultFunction(f)
                if !f.name.is_empty() && !names.insert(f.name.clone()) =>
            {
                return Err(
                    self.error(format!("Identifier '{}' has already been declared", f.name))
                );
            }
            ExportDeclaration::DefaultClass(c)
                if !c.name.is_empty() && !names.insert(c.name.clone()) =>
            {
                return Err(
                    self.error(format!("Identifier '{}' has already been declared", c.name))
                );
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn collect_exported_bindings(
        &self,
        export: &ExportDeclaration,
        names: &mut HashSet<String>,
    ) {
        if let ExportDeclaration::Named {
            specifiers,
            source: None,
            ..
        } = export
        {
            for spec in specifiers {
                names.insert(spec.local.clone());
            }
        }
    }
}
