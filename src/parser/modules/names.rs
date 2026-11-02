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
        if let Some(name) = self.export_name_text() {
            self.advance()?;
            return Ok(name);
        }
        Err(self.error("Expected identifier or string"))
    }

    pub(in crate::parser) fn has_lone_surrogate(code_units: &[u16]) -> bool {
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

    fn export_name_text(&self) -> Option<String> {
        match &self.current {
            Token::Identifier(name) | Token::IdentifierWithEscape(name) => Some(name.clone()),
            Token::Keyword(kw) => Some(kw.to_string()),
            Token::NullLiteral => Some("null".to_string()),
            Token::BooleanLiteral(b) => Some(b.to_string()),
            _ => None,
        }
    }
}
