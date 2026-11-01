//! Module-goal entry points. The parser reads its first token when it is built, before the caller says
//! whether the source is a Script or a Module. Annex B.1.1 HTML-like
//! comments exist in Scripts only, so a Module re-reads that token.

use super::*;

impl<'a> Parser<'a> {
    pub(super) fn relex_first_token_as_module(&mut self) -> Result<(), ParseError> {
        let mut lexer = if self.lone_surrogate_stand_ins {
            Lexer::new_with_stand_ins(self.source)
        } else {
            Lexer::new(self.source)
        };
        lexer.is_module = true;
        self.prev_line_terminator = false;
        self.current = loop {
            match lexer.next_token()? {
                Token::LineTerminator => self.prev_line_terminator = true,
                token => break token,
            }
        };
        self.current_token_start = lexer.token_start();
        self.current_token_end = lexer.offset();
        self.lexer = lexer;
        Ok(())
    }

    pub(super) fn get_exported_names(&self, export: &ExportDeclaration) -> Vec<String> {
        match export {
            ExportDeclaration::Named {
                specifiers,
                declaration,
                ..
            } => {
                let mut names = Vec::new();
                for spec in specifiers {
                    names.push(spec.exported.clone());
                }
                if let Some(decl) = declaration {
                    names.extend(self.get_declaration_export_names(decl));
                }
                names
            }
            ExportDeclaration::Default(_)
            | ExportDeclaration::DefaultFunction(_)
            | ExportDeclaration::DefaultClass(_) => {
                vec!["default".to_string()]
            }
            ExportDeclaration::All { exported, .. } => {
                // export * as ns from "mod" exports 'ns'
                // export * from "mod" doesn't add to local exported names
                exported.iter().cloned().collect()
            }
        }
    }

    fn get_declaration_export_names(&self, decl: &Statement) -> Vec<String> {
        match decl {
            Statement::Variable(var) => {
                let mut names = Vec::new();
                for d in &var.declarations {
                    d.pattern.bound_names(&mut names);
                }
                names
            }
            Statement::FunctionDeclaration(f) => vec![f.name.clone()],
            Statement::ClassDeclaration(c) => vec![c.name.clone()],
            _ => vec![],
        }
    }
}
