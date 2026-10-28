use super::*;

impl<'a> Parser<'a> {
    pub(super) fn parse_module_export_name(&mut self) -> Result<String, ParseError> {
        // ModuleExportName: IdentifierName | StringLiteral
        if let Token::StringLiteral(s) = &self.current {
            if Self::has_lone_surrogate(s) {
                return Err(self.error("An export name cannot include a lone surrogate."));
            }
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

    fn has_lone_surrogate(code_units: &[u16]) -> bool {
        let mut i = 0;
        while i < code_units.len() {
            let cu = code_units[i];
            if (0xD800..=0xDBFF).contains(&cu) {
                if i + 1 < code_units.len() && (0xDC00..=0xDFFF).contains(&code_units[i + 1]) {
                    i += 2; // valid surrogate pair
                } else {
                    return true; // lone high surrogate
                }
            } else if (0xDC00..=0xDFFF).contains(&cu) {
                return true; // lone low surrogate
            } else {
                i += 1;
            }
        }
        false
    }

    pub(super) fn current_identifier_name_including_keywords(&self) -> Option<String> {
        match &self.current {
            Token::Identifier(name) | Token::IdentifierWithEscape(name) => Some(name.clone()),
            Token::Keyword(kw) => Some(keyword_to_string(kw)),
            Token::NullLiteral => Some("null".to_string()),
            Token::BooleanLiteral(b) => Some(b.to_string()),
            _ => None,
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
