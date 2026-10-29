use super::*;

impl<'a> Parser<'a> {
    /// Early errors of a FunctionBody's top-level declarations:
    /// - §15.2.1: no BoundName of FormalParameters occurs in the
    ///   LexicallyDeclaredNames of the FunctionBody.
    /// - §15.2.1: LexicallyDeclaredNames contains no duplicates.
    /// - §15.2.1: no LexicallyDeclaredName occurs in the VarDeclaredNames.
    ///
    /// Top-level function declarations of a function body are var-scoped
    /// (§8.2.5 TopLevelVarDeclaredNames), so they collide with a `let`,
    /// `const` or `class` of the same name.
    pub(super) fn check_function_body_declarations(
        &self,
        stmts: &[Statement],
        param_names: Option<&HashSet<String>>,
    ) -> Result<(), ParseError> {
        let redeclared =
            |name: &str| self.error(format!("Identifier '{name}' has already been declared"));
        let mut lexical: HashSet<String> = HashSet::new();
        for stmt in stmts {
            let names = match stmt {
                Statement::Variable(vd) if vd.kind != VarKind::Var => {
                    Self::bound_names_from_decl(vd)
                }
                Statement::ClassDeclaration(cls) => vec![cls.name.clone()],
                _ => continue,
            };
            for name in names {
                if param_names.is_some_and(|p| p.contains(&name)) || lexical.contains(&name) {
                    return Err(redeclared(&name));
                }
                lexical.insert(name);
            }
        }
        if lexical.is_empty() {
            return Ok(());
        }
        let mut var_names = Vec::new();
        for stmt in stmts {
            Self::collect_top_level_var_declared_names(stmt, &mut var_names);
        }
        match var_names.iter().find(|n| lexical.contains(*n)) {
            Some(name) => Err(redeclared(name)),
            None => Ok(()),
        }
    }
}
