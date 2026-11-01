//! Early errors for a `catch` clause: ECMAScript 14.15.1 and Annex B.3.4.

use super::*;

impl<'a> Parser<'a> {
    /// - 14.15.1: BoundNames of the CatchParameter contain no duplicates and
    ///   do not occur in the LexicallyDeclaredNames of the Block.
    /// - B.3.4: they must not occur in the VarDeclaredNames of the Block
    ///   either, unless the CatchParameter is a plain BindingIdentifier.
    pub(super) fn check_catch_clause(
        &self,
        param: Option<&Pattern>,
        body: &[Statement],
    ) -> Result<(), ParseError> {
        let Some(param) = param else {
            return Ok(());
        };
        let mut bound = Vec::new();
        param.bound_names(&mut bound);
        let mut seen = HashSet::new();
        for name in &bound {
            if !seen.insert(name.as_str()) {
                return Err(self.error(format!("Duplicate binding '{name}' in catch clause")));
            }
        }
        if bound.is_empty() {
            return Ok(());
        }
        let mut declared = Vec::new();
        for stmt in body {
            Self::collect_lexical_names(stmt, &mut declared, self.strict)?;
        }
        if !matches!(param, Pattern::Identifier(_)) {
            for stmt in body {
                Self::collect_var_declared_names(stmt, &mut declared);
            }
        }
        match bound.iter().find(|name| declared.contains(name)) {
            Some(name) => Err(self.error(format!("Identifier '{name}' has already been declared"))),
            None => Ok(()),
        }
    }
}
