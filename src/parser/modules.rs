use super::*;

impl<'a> Parser<'a> {
    pub(super) fn parse_import_declaration(&mut self) -> Result<ImportDeclaration, ParseError> {
        self.advance()?; // import

        // import "module" (side effect import)
        if let Token::StringLiteral(source) = &self.current {
            let source = String::from_utf16_lossy(source);
            self.advance()?;
            self.eat_semicolon()?;
            return Ok(ImportDeclaration {
                specifiers: vec![],
                source,
            });
        }

        let mut specifiers = Vec::new();

        // import defaultExport from "module"
        if let Some(name) = self.current_identifier_name() {
            self.advance()?;
            specifiers.push(ImportSpecifier::Default(name));

            // import defaultExport, { named } from "module"
            // import defaultExport, * as ns from "module"
            if self.current == Token::Comma {
                self.advance()?;
                if self.current == Token::Star {
                    self.advance()?;
                    self.eat_as()?;
                    let local = self
                        .current_identifier_name()
                        .ok_or_else(|| self.error("Expected identifier after 'as'"))?;
                    self.advance()?;
                    specifiers.push(ImportSpecifier::Namespace(local));
                } else {
                    self.eat(&Token::LeftBrace)?;
                    self.parse_named_imports(&mut specifiers)?;
                    self.eat(&Token::RightBrace)?;
                }
            }
        }
        // import * as ns from "module"
        else if self.current == Token::Star {
            self.advance()?;
            self.eat_as()?;
            let local = self
                .current_identifier_name()
                .ok_or_else(|| self.error("Expected identifier after 'as'"))?;
            self.advance()?;
            specifiers.push(ImportSpecifier::Namespace(local));
        }
        // import { named } from "module"
        else if self.current == Token::LeftBrace {
            self.advance()?;
            self.parse_named_imports(&mut specifiers)?;
            self.eat(&Token::RightBrace)?;
        } else {
            return Err(self.error("Expected import specifier"));
        }

        self.eat_from()?;
        let source = self.parse_module_specifier()?;
        self.eat_semicolon()?;

        Ok(ImportDeclaration { specifiers, source })
    }

    fn parse_named_imports(
        &mut self,
        specifiers: &mut Vec<ImportSpecifier>,
    ) -> Result<(), ParseError> {
        while self.current != Token::RightBrace {
            let imported = self.parse_module_export_name()?;

            let local = if self.is_as_keyword() {
                self.advance()?; // as
                let local = self
                    .current_identifier_name()
                    .ok_or_else(|| self.error("Expected identifier after 'as'"))?;
                self.advance()?;
                local
            } else {
                imported.clone()
            };

            specifiers.push(ImportSpecifier::Named { imported, local });

            if self.current == Token::Comma {
                self.advance()?;
            } else {
                break;
            }
        }
        Ok(())
    }

    pub(super) fn parse_export_declaration(&mut self) -> Result<ExportDeclaration, ParseError> {
        self.advance()?; // export

        // export default ...
        if self.current == Token::Keyword(Keyword::Default) {
            self.advance()?;
            return self.parse_export_default();
        }

        // export * from "module"
        // export * as ns from "module"
        if self.current == Token::Star {
            self.advance()?;
            let exported = if self.is_as_keyword() {
                self.advance()?;
                let name = self.parse_module_export_name()?;
                Some(name)
            } else {
                None
            };
            self.eat_from()?;
            let source = self.parse_module_specifier()?;
            self.eat_semicolon()?;
            return Ok(ExportDeclaration::All { exported, source });
        }

        // export { named }
        // export { named } from "module"
        if self.current == Token::LeftBrace {
            self.advance()?;
            let specifiers = self.parse_export_specifiers()?;
            self.eat(&Token::RightBrace)?;

            let source = if self.is_from_keyword() {
                self.advance()?;
                Some(self.parse_module_specifier()?)
            } else {
                None
            };
            self.eat_semicolon()?;
            return Ok(ExportDeclaration::Named {
                specifiers,
                source,
                declaration: None,
            });
        }

        // export var/let/const/function/class/async function
        let declaration = self.parse_export_declaration_statement()?;
        Ok(ExportDeclaration::Named {
            specifiers: vec![],
            source: None,
            declaration: Some(Box::new(declaration)),
        })
    }

    fn parse_export_default(&mut self) -> Result<ExportDeclaration, ParseError> {
        // export default function name() {}
        // export default function() {}
        if self.current == Token::Keyword(Keyword::Function) {
            let func = self.parse_function_for_export()?;
            return Ok(ExportDeclaration::DefaultFunction(func));
        }

        // export default async function name() {}
        // export default async function() {}
        if matches!(&self.current, Token::Keyword(Keyword::Async)) {
            let saved_lt = self.prev_line_terminator;
            let saved = self.advance()?;
            if self.current == Token::Keyword(Keyword::Function) && !self.prev_line_terminator {
                let func = self.parse_async_function_for_export()?;
                return Ok(ExportDeclaration::DefaultFunction(func));
            }
            self.push_back(self.current.clone(), self.prev_line_terminator);
            self.current = saved;
            self.prev_line_terminator = saved_lt;
        }

        // export default class Name {}
        // export default class {}
        if self.current == Token::Keyword(Keyword::Class) {
            let class = self.parse_class_for_export()?;
            return Ok(ExportDeclaration::DefaultClass(class));
        }

        // export default expression
        let expr = self.parse_assignment_expression()?;
        self.eat_semicolon()?;
        Ok(ExportDeclaration::Default(Box::new(expr)))
    }

    fn parse_export_specifiers(&mut self) -> Result<Vec<ExportSpecifier>, ParseError> {
        let mut specifiers = Vec::new();
        while self.current != Token::RightBrace {
            let local = self.parse_module_export_name()?;

            let exported = if self.is_as_keyword() {
                self.advance()?;
                self.parse_module_export_name()?
            } else {
                local.clone()
            };

            specifiers.push(ExportSpecifier { local, exported });

            if self.current == Token::Comma {
                self.advance()?;
            } else {
                break;
            }
        }
        Ok(specifiers)
    }

    fn parse_export_declaration_statement(&mut self) -> Result<Statement, ParseError> {
        match &self.current {
            Token::Keyword(Keyword::Var) => self.parse_variable_statement(),
            Token::Keyword(Keyword::Let) | Token::Keyword(Keyword::Const) => {
                self.parse_lexical_declaration()
            }
            Token::Keyword(Keyword::Function) => self.parse_function_declaration(),
            Token::Keyword(Keyword::Class) => self.parse_class_declaration(),
            Token::Keyword(Keyword::Async) => {
                if self.is_async_function() {
                    self.parse_function_declaration()
                } else {
                    Err(self.error("Expected declaration"))
                }
            }
            _ => Err(self.error("Expected declaration after export")),
        }
    }

    fn parse_function_for_export(&mut self) -> Result<FunctionDecl, ParseError> {
        let start = self.current_token_start;
        self.advance()?; // function
        let is_generator = self.eat_star()?;

        let name = if let Some(n) = self.current_identifier_name() {
            self.advance()?;
            n
        } else {
            String::new()
        };

        let params = self.parse_formal_parameters()?;
        let (body, _) = self.parse_function_body_with_context(is_generator, false)?;

        let source_text = Some(self.source_since(start));

        Ok(FunctionDecl {
            name,
            params,
            body,
            is_async: false,
            is_generator,
            source_text,
        })
    }

    fn parse_async_function_for_export(&mut self) -> Result<FunctionDecl, ParseError> {
        let start = self.current_token_start;
        self.advance()?; // function
        let is_generator = self.eat_star()?;

        let name = if let Some(n) = self.current_identifier_name() {
            self.advance()?;
            n
        } else {
            String::new()
        };

        let params = self.parse_formal_parameters()?;
        let (body, _) = self.parse_function_body_with_context(is_generator, true)?;

        let source_text = Some(self.source_since(start));

        Ok(FunctionDecl {
            name,
            params,
            body,
            is_async: true,
            is_generator,
            source_text,
        })
    }

    fn parse_class_for_export(&mut self) -> Result<ClassDecl, ParseError> {
        let start = self.current_token_start;
        self.advance()?; // class

        let name = if let Some(n) = self.current_identifier_name() {
            self.advance()?;
            n
        } else {
            String::new()
        };

        let super_class = if self.current == Token::Keyword(Keyword::Extends) {
            self.advance()?;
            Some(Box::new(self.parse_left_hand_side_expression()?))
        } else {
            None
        };

        // parse_class_body handles { ... }
        let body = self.parse_class_body()?;

        let source_text = Some(self.source_since(start));

        Ok(ClassDecl {
            name,
            super_class,
            body,
            source_text,
        })
    }

    fn parse_module_specifier(&mut self) -> Result<String, ParseError> {
        match &self.current {
            Token::StringLiteral(s) => {
                let s = String::from_utf16_lossy(s);
                self.advance()?;
                Ok(s)
            }
            _ => Err(self.error("Expected module specifier string")),
        }
    }

    fn parse_module_export_name(&mut self) -> Result<String, ParseError> {
        // ModuleExportName: IdentifierName | StringLiteral
        if let Token::StringLiteral(s) = &self.current {
            let s = String::from_utf16_lossy(s);
            self.advance()?;
            return Ok(s);
        }
        // Accept any identifier name (including keywords)
        if let Some(name) = self.current_identifier_name_including_keywords() {
            self.advance()?;
            return Ok(name);
        }
        Err(self.error("Expected identifier or string"))
    }

    pub(super) fn current_identifier_name_including_keywords(&self) -> Option<String> {
        match &self.current {
            Token::Identifier(name) | Token::IdentifierWithEscape(name) => Some(name.clone()),
            Token::Keyword(kw) => Some(keyword_to_string(kw)),
            _ => None,
        }
    }

    fn is_as_keyword(&self) -> bool {
        matches!(&self.current, Token::Identifier(s) if s == "as")
    }

    fn eat_as(&mut self) -> Result<(), ParseError> {
        if self.is_as_keyword() {
            self.advance()?;
            Ok(())
        } else {
            Err(self.error("Expected 'as'"))
        }
    }

    fn is_from_keyword(&self) -> bool {
        matches!(&self.current, Token::Identifier(s) if s == "from")
    }

    fn eat_from(&mut self) -> Result<(), ParseError> {
        if self.is_from_keyword() {
            self.advance()?;
            Ok(())
        } else {
            Err(self.error("Expected 'from'"))
        }
    }
}

fn keyword_to_string(kw: &Keyword) -> String {
    match kw {
        Keyword::Async => "async",
        Keyword::Await => "await",
        Keyword::Break => "break",
        Keyword::Case => "case",
        Keyword::Catch => "catch",
        Keyword::Class => "class",
        Keyword::Const => "const",
        Keyword::Continue => "continue",
        Keyword::Debugger => "debugger",
        Keyword::Default => "default",
        Keyword::Delete => "delete",
        Keyword::Do => "do",
        Keyword::Else => "else",
        Keyword::Enum => "enum",
        Keyword::Export => "export",
        Keyword::Extends => "extends",
        Keyword::Finally => "finally",
        Keyword::For => "for",
        Keyword::Function => "function",
        Keyword::If => "if",
        Keyword::Import => "import",
        Keyword::In => "in",
        Keyword::Instanceof => "instanceof",
        Keyword::Let => "let",
        Keyword::New => "new",
        Keyword::Of => "of",
        Keyword::Return => "return",
        Keyword::Static => "static",
        Keyword::Super => "super",
        Keyword::Switch => "switch",
        Keyword::This => "this",
        Keyword::Throw => "throw",
        Keyword::Try => "try",
        Keyword::Typeof => "typeof",
        Keyword::Var => "var",
        Keyword::Void => "void",
        Keyword::While => "while",
        Keyword::With => "with",
        Keyword::Yield => "yield",
    }
    .to_string()
}
