use std::fmt;
use std::str::Chars;

#[derive(Clone, Debug, PartialEq)]
pub enum Token {
    // Identifiers and keywords
    Identifier(String),
    Keyword(Keyword),

    // Literals
    NumericLiteral(f64),
    LegacyOctalLiteral(f64),
    BigIntLiteral(String),
    StringLiteral(String),
    BooleanLiteral(bool),
    NullLiteral,
    RegExpLiteral { pattern: String, flags: String },

    // Template literals: (cooked, raw) — cooked is None for invalid escapes in tagged templates
    NoSubstitutionTemplate(Option<String>, String),
    TemplateHead(Option<String>, String),
    TemplateMiddle(Option<String>, String),
    TemplateTail(Option<String>, String),

    // Punctuators
    LeftBrace,                // {
    RightBrace,               // }
    LeftParen,                // (
    RightParen,               // )
    LeftBracket,              // [
    RightBracket,             // ]
    Dot,                      // .
    Ellipsis,                 // ...
    Semicolon,                // ;
    Comma,                    // ,
    LessThan,                 // <
    GreaterThan,              // >
    LessThanEqual,            // <=
    GreaterThanEqual,         // >=
    Equal,                    // ==
    NotEqual,                 // !=
    StrictEqual,              // ===
    StrictNotEqual,           // !==
    Plus,                     // +
    Minus,                    // -
    Star,                     // *
    Percent,                  // %
    Exponent,                 // **
    Increment,                // ++
    Decrement,                // --
    LeftShift,                // <<
    RightShift,               // >>
    UnsignedRightShift,       // >>>
    Ampersand,                // &
    Pipe,                     // |
    Caret,                    // ^
    Bang,                     // !
    Tilde,                    // ~
    LogicalAnd,               // &&
    LogicalOr,                // ||
    NullishCoalescing,        // ??
    Question,                 // ?
    OptionalChain,            // ?.
    Colon,                    // :
    Assign,                   // =
    PlusAssign,               // +=
    MinusAssign,              // -=
    StarAssign,               // *=
    PercentAssign,            // %=
    ExponentAssign,           // **=
    LeftShiftAssign,          // <<=
    RightShiftAssign,         // >>=
    UnsignedRightShiftAssign, // >>>=
    AmpersandAssign,          // &=
    PipeAssign,               // |=
    CaretAssign,              // ^=
    LogicalAndAssign,         // &&=
    LogicalOrAssign,          // ||=
    NullishAssign,            // ??=
    Arrow,                    // =>
    Slash,                    // /
    SlashAssign,              // /=

    // Special
    LineTerminator,
    Eof,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Keyword {
    Async,
    Await,
    Break,
    Case,
    Catch,
    Class,
    Const,
    Continue,
    Debugger,
    Default,
    Delete,
    Do,
    Else,
    Enum,
    Export,
    Extends,
    Finally,
    For,
    Function,
    If,
    Import,
    In,
    Instanceof,
    Let,
    New,
    Of,
    Return,
    Static,
    Super,
    Switch,
    This,
    Throw,
    Try,
    Typeof,
    Var,
    Void,
    While,
    With,
    Yield,
}

impl Keyword {
    pub fn from_str(s: &str) -> Option<Keyword> {
        match s {
            "async" => Some(Keyword::Async),
            "await" => Some(Keyword::Await),
            "break" => Some(Keyword::Break),
            "case" => Some(Keyword::Case),
            "catch" => Some(Keyword::Catch),
            "class" => Some(Keyword::Class),
            "const" => Some(Keyword::Const),
            "continue" => Some(Keyword::Continue),
            "debugger" => Some(Keyword::Debugger),
            "default" => Some(Keyword::Default),
            "delete" => Some(Keyword::Delete),
            "do" => Some(Keyword::Do),
            "else" => Some(Keyword::Else),
            "enum" => Some(Keyword::Enum),
            "export" => Some(Keyword::Export),
            "extends" => Some(Keyword::Extends),
            "finally" => Some(Keyword::Finally),
            "for" => Some(Keyword::For),
            "function" => Some(Keyword::Function),
            "if" => Some(Keyword::If),
            "import" => Some(Keyword::Import),
            "in" => Some(Keyword::In),
            "instanceof" => Some(Keyword::Instanceof),
            "let" => Some(Keyword::Let),
            "new" => Some(Keyword::New),
            "of" => Some(Keyword::Of),
            "return" => Some(Keyword::Return),
            "static" => Some(Keyword::Static),
            "super" => Some(Keyword::Super),
            "switch" => Some(Keyword::Switch),
            "this" => Some(Keyword::This),
            "throw" => Some(Keyword::Throw),
            "try" => Some(Keyword::Try),
            "typeof" => Some(Keyword::Typeof),
            "var" => Some(Keyword::Var),
            "void" => Some(Keyword::Void),
            "while" => Some(Keyword::While),
            "with" => Some(Keyword::With),
            "yield" => Some(Keyword::Yield),
            _ => None,
        }
    }
}

impl fmt::Display for Keyword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
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
        };
        write!(f, "{s}")
    }
}

#[derive(Clone, Debug)]
pub struct SourceLocation {
    pub line: u32,
    pub column: u32,
    pub offset: usize,
}

#[derive(Clone, Debug)]
pub struct LexError {
    pub message: String,
    pub location: SourceLocation,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{}: {}",
            self.location.line, self.location.column, self.message
        )
    }
}

pub struct Lexer<'a> {
    source: &'a str,
    chars: Chars<'a>,
    current: Option<char>,
    offset: usize,
    line: u32,
    column: u32,
    pub strict: bool,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        let mut chars = source.chars();
        let current = chars.next();
        Self {
            source,
            chars,
            current,
            offset: 0,
            line: 1,
            column: 0,
            strict: false,
        }
    }

    fn peek(&self) -> Option<char> {
        self.current
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.current;
        if let Some(c) = ch {
            self.offset += c.len_utf8();
            self.column += 1;
            self.current = self.chars.next();
        }
        ch
    }

    fn peek_next(&self) -> Option<char> {
        self.chars.clone().next()
    }

    fn location(&self) -> SourceLocation {
        SourceLocation {
            line: self.line,
            column: self.column,
            offset: self.offset,
        }
    }

    fn error(&self, message: impl Into<String>) -> LexError {
        LexError {
            message: message.into(),
            location: self.location(),
        }
    }

    fn is_line_terminator(ch: char) -> bool {
        matches!(ch, '\n' | '\r' | '\u{2028}' | '\u{2029}')
    }

    fn is_whitespace(ch: char) -> bool {
        matches!(
            ch,
            '\t' | '\u{000B}' | '\u{000C}' | ' ' | '\u{00A0}' | '\u{FEFF}'
        ) || ch.is_whitespace() && !Self::is_line_terminator(ch)
    }

    fn is_identifier_start(ch: char) -> bool {
        ch == '_' || ch == '$' || ch.is_ascii_alphabetic() || unicode_id_start(ch)
    }

    fn is_identifier_continue(ch: char) -> bool {
        ch == '_'
            || ch == '$'
            || ch.is_ascii_alphanumeric()
            || ch == '\u{200C}'
            || ch == '\u{200D}'
            || unicode_id_continue(ch)
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if Self::is_whitespace(ch) {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_line_comment(&mut self) {
        // skip past //
        while let Some(ch) = self.peek() {
            if Self::is_line_terminator(ch) {
                break;
            }
            self.advance();
        }
    }

    fn skip_block_comment(&mut self) -> Result<bool, LexError> {
        let mut has_line_terminator = false;
        loop {
            match self.advance() {
                Some('*') => {
                    if self.peek() == Some('/') {
                        self.advance();
                        return Ok(has_line_terminator);
                    }
                }
                Some(ch) if Self::is_line_terminator(ch) => {
                    has_line_terminator = true;
                    self.handle_newline(ch);
                }
                Some(_) => {}
                None => return Err(self.error("Unterminated block comment")),
            }
        }
    }

    fn handle_newline(&mut self, ch: char) {
        if ch == '\r' && self.peek() == Some('\n') {
            self.advance();
        }
        self.line += 1;
        self.column = 0;
    }

    fn read_string(&mut self, quote: char) -> Result<String, LexError> {
        let mut s = String::new();
        loop {
            match self.advance() {
                None => return Err(self.error("Unterminated string literal")),
                Some(ch) if ch == quote => return Ok(s),
                Some(ch) if Self::is_line_terminator(ch) => {
                    return Err(self.error("Unterminated string literal"));
                }
                Some('\\') => {
                    let esc = self.read_escape_sequence()?;
                    s.push_str(&esc);
                }
                Some(ch) => s.push(ch),
            }
        }
    }

    fn read_escape_sequence(&mut self) -> Result<String, LexError> {
        match self.advance() {
            None => Err(self.error("Unterminated escape sequence")),
            Some('n') => Ok("\n".to_string()),
            Some('r') => Ok("\r".to_string()),
            Some('t') => Ok("\t".to_string()),
            Some('b') => Ok("\u{0008}".to_string()),
            Some('f') => Ok("\u{000C}".to_string()),
            Some('v') => Ok("\u{000B}".to_string()),
            Some(ch @ '0'..='7') => {
                if ch == '0' && !self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    return Ok("\0".to_string()); // \0 (null character, not octal)
                }
                if self.strict {
                    return Err(self.error("Octal escape sequences are not allowed in strict mode"));
                }
                let mut val = (ch as u32) - ('0' as u32);
                if self.peek().is_some_and(|c| ('0'..='7').contains(&c)) {
                    val = val * 8 + (self.advance().unwrap() as u32 - '0' as u32);
                    if ch <= '3' && self.peek().is_some_and(|c| ('0'..='7').contains(&c)) {
                        val = val * 8 + (self.advance().unwrap() as u32 - '0' as u32);
                    }
                }
                Ok(char::from_u32(val).map(|c| c.to_string()).unwrap_or_default())
            }
            Some('x') => {
                let h1 = self
                    .advance()
                    .ok_or_else(|| self.error("Invalid hex escape"))?;
                let h2 = self
                    .advance()
                    .ok_or_else(|| self.error("Invalid hex escape"))?;
                let val = hex_val(h1)
                    .and_then(|a| hex_val(h2).map(|b| a * 16 + b))
                    .ok_or_else(|| self.error("Invalid hex escape"))?;
                Ok(char::from_u32(val)
                    .map(|c| c.to_string())
                    .unwrap_or_default())
            }
            Some('u') => self.read_unicode_escape(),
            Some(ch) if Self::is_line_terminator(ch) => {
                self.handle_newline(ch);
                Ok(String::new())
            }
            Some(ch) => Ok(ch.to_string()),
        }
    }

    fn read_unicode_escape(&mut self) -> Result<String, LexError> {
        if self.peek() == Some('{') {
            self.advance(); // skip {
            let mut val: u32 = 0;
            let mut digits = 0;
            while let Some(ch) = self.peek() {
                if ch == '}' {
                    self.advance();
                    if digits == 0 {
                        return Err(self.error("Invalid Unicode escape"));
                    }
                    return char::from_u32(val)
                        .map(|c| c.to_string())
                        .ok_or_else(|| self.error("Invalid Unicode code point"));
                }
                let d = hex_val(ch).ok_or_else(|| self.error("Invalid Unicode escape"))?;
                val = val * 16 + d;
                if val > 0x10FFFF {
                    return Err(self.error("Unicode code point out of range"));
                }
                digits += 1;
                self.advance();
            }
            Err(self.error("Unterminated Unicode escape"))
        } else {
            let mut val: u32 = 0;
            for _ in 0..4 {
                let ch = self
                    .advance()
                    .ok_or_else(|| self.error("Invalid Unicode escape"))?;
                let d = hex_val(ch).ok_or_else(|| self.error("Invalid Unicode escape"))?;
                val = val * 16 + d;
            }
            char::from_u32(val)
                .map(|c| c.to_string())
                .ok_or_else(|| self.error("Invalid Unicode code point"))
        }
    }

    fn read_numeric_literal(&mut self, first: char) -> Result<Token, LexError> {
        let mut s = String::new();
        s.push(first);

        if first == '0' {
            match self.peek() {
                Some('x' | 'X') => return self.read_hex_literal(s),
                Some('o' | 'O') => return self.read_octal_literal(s),
                Some('b' | 'B') => return self.read_binary_literal(s),
                Some(c) if c.is_ascii_digit() => {
                    return self.read_legacy_octal_or_decimal(s);
                }
                _ => {}
            }
        }

        // Decimal
        self.read_decimal_digits(&mut s);

        if self.peek() == Some('.') {
            s.push('.');
            self.advance();
            self.read_decimal_digits(&mut s);
        }

        if self.peek().is_some_and(|c| c == 'e' || c == 'E') {
            s.push(self.advance().unwrap());
            if self.peek().is_some_and(|c| c == '+' || c == '-') {
                s.push(self.advance().unwrap());
            }
            self.read_decimal_digits(&mut s);
        }

        if self.peek() == Some('n') {
            self.advance();
            let clean: String = s.chars().filter(|&c| c != '_').collect();
            return Ok(Token::BigIntLiteral(clean));
        }

        let clean: String = s.chars().filter(|&c| c != '_').collect();
        let val: f64 = clean
            .parse()
            .map_err(|_| self.error("Invalid numeric literal"))?;
        Ok(Token::NumericLiteral(val))
    }

    fn read_decimal_digits(&mut self, s: &mut String) {
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() || ch == '_' {
                s.push(ch);
                self.advance();
            } else {
                break;
            }
        }
    }

    fn read_hex_literal(&mut self, mut s: String) -> Result<Token, LexError> {
        s.push(self.advance().unwrap()); // x/X
        while let Some(ch) = self.peek() {
            if ch.is_ascii_hexdigit() || ch == '_' {
                s.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        if self.peek() == Some('n') {
            self.advance();
            let clean: String = s.chars().filter(|&c| c != '_').collect();
            return Ok(Token::BigIntLiteral(clean));
        }
        let hex_part: String = s[2..].chars().filter(|&c| c != '_').collect();
        let val =
            u64::from_str_radix(&hex_part, 16).map_err(|_| self.error("Invalid hex literal"))?;
        Ok(Token::NumericLiteral(val as f64))
    }

    fn read_octal_literal(&mut self, mut s: String) -> Result<Token, LexError> {
        s.push(self.advance().unwrap()); // o/O
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() && ch < '8' || ch == '_' {
                s.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        if self.peek() == Some('n') {
            self.advance();
            let clean: String = s.chars().filter(|&c| c != '_').collect();
            return Ok(Token::BigIntLiteral(clean));
        }
        let oct_part: String = s[2..].chars().filter(|&c| c != '_').collect();
        let val =
            u64::from_str_radix(&oct_part, 8).map_err(|_| self.error("Invalid octal literal"))?;
        Ok(Token::NumericLiteral(val as f64))
    }

    fn read_legacy_octal_or_decimal(&mut self, mut s: String) -> Result<Token, LexError> {
        let mut is_octal = true;
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                if ch >= '8' {
                    is_octal = false;
                }
                s.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        if is_octal && self.peek() != Some('.') && self.peek() != Some('e') && self.peek() != Some('E') {
            let oct_part = &s[1..]; // skip leading 0
            let val = u64::from_str_radix(oct_part, 8)
                .map_err(|_| self.error("Invalid octal literal"))?;
            Ok(Token::LegacyOctalLiteral(val as f64))
        } else {
            // Non-octal decimal (e.g. 09, 0.5 after leading zero digits)
            if self.peek() == Some('.') {
                s.push('.');
                self.advance();
                self.read_decimal_digits(&mut s);
            }
            if self.peek().is_some_and(|c| c == 'e' || c == 'E') {
                s.push(self.advance().unwrap());
                if self.peek().is_some_and(|c| c == '+' || c == '-') {
                    s.push(self.advance().unwrap());
                }
                self.read_decimal_digits(&mut s);
            }
            let val: f64 = s.parse().map_err(|_| self.error("Invalid numeric literal"))?;
            Ok(Token::NumericLiteral(val))
        }
    }

    fn read_binary_literal(&mut self, mut s: String) -> Result<Token, LexError> {
        s.push(self.advance().unwrap()); // b/B
        while let Some(ch) = self.peek() {
            if ch == '0' || ch == '1' || ch == '_' {
                s.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        if self.peek() == Some('n') {
            self.advance();
            let clean: String = s.chars().filter(|&c| c != '_').collect();
            return Ok(Token::BigIntLiteral(clean));
        }
        let bin_part: String = s[2..].chars().filter(|&c| c != '_').collect();
        let val =
            u64::from_str_radix(&bin_part, 2).map_err(|_| self.error("Invalid binary literal"))?;
        Ok(Token::NumericLiteral(val as f64))
    }

    fn read_identifier(&mut self, first: char) -> Token {
        let mut name = String::new();
        name.push(first);
        while let Some(ch) = self.peek() {
            if Self::is_identifier_continue(ch) {
                name.push(ch);
                self.advance();
            } else if ch == '\\' {
                // Unicode escape in identifier — simplified handling
                self.advance();
                if let Ok(esc) = self.read_unicode_escape() {
                    name.push_str(&esc);
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        match name.as_str() {
            "true" => Token::BooleanLiteral(true),
            "false" => Token::BooleanLiteral(false),
            "null" => Token::NullLiteral,
            _ => {
                if let Some(kw) = Keyword::from_str(&name) {
                    Token::Keyword(kw)
                } else {
                    Token::Identifier(name)
                }
            }
        }
    }

    pub fn lex_regex(&mut self) -> Result<Token, LexError> {
        let mut pattern = String::new();
        let mut in_class = false;
        loop {
            match self.peek() {
                None | Some('\n') | Some('\r') => {
                    return Err(LexError {
                        message: "Unterminated regular expression".to_string(),
                        location: self.location(),
                    });
                }
                Some('/') if !in_class => {
                    self.advance();
                    break;
                }
                Some('[') => {
                    in_class = true;
                    pattern.push(self.advance().unwrap());
                }
                Some(']') => {
                    in_class = false;
                    pattern.push(self.advance().unwrap());
                }
                Some('\\') => {
                    pattern.push(self.advance().unwrap());
                    if let Some(c) = self.peek() {
                        pattern.push(self.advance().unwrap());
                    }
                }
                Some(_) => {
                    pattern.push(self.advance().unwrap());
                }
            }
        }
        let mut flags = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphabetic() {
                flags.push(self.advance().unwrap());
            } else {
                break;
            }
        }
        Ok(Token::RegExpLiteral { pattern, flags })
    }

    pub fn next_token(&mut self) -> Result<Token, LexError> {
        loop {
            self.skip_whitespace();

            let ch = match self.peek() {
                None => return Ok(Token::Eof),
                Some(ch) => ch,
            };

            if Self::is_line_terminator(ch) {
                self.advance();
                self.handle_newline(ch);
                return Ok(Token::LineTerminator);
            }

            // Comments
            if ch == '/' {
                if self.peek_next() == Some('/') {
                    self.advance();
                    self.advance();
                    self.skip_line_comment();
                    continue;
                }
                if self.peek_next() == Some('*') {
                    self.advance();
                    self.advance();
                    let had_lt = self.skip_block_comment()?;
                    if had_lt {
                        return Ok(Token::LineTerminator);
                    }
                    continue;
                }
            }

            // Hashbang
            if ch == '#' && self.offset == 0 && self.peek_next() == Some('!') {
                self.skip_line_comment();
                continue;
            }

            self.advance();

            // String literals
            if ch == '\'' || ch == '"' {
                let s = self.read_string(ch)?;
                return Ok(Token::StringLiteral(s));
            }

            // Template literals
            if ch == '`' {
                return self.read_template_literal();
            }

            // Numeric literals
            if ch.is_ascii_digit() {
                return self.read_numeric_literal(ch);
            }
            if ch == '.' && self.peek().is_some_and(|c| c.is_ascii_digit()) {
                return self.read_numeric_literal(ch);
            }

            // Identifiers
            if Self::is_identifier_start(ch) {
                return Ok(self.read_identifier(ch));
            }

            // Punctuators
            return self.read_punctuator(ch);
        }
    }

    // Returns (cooked, raw, is_tail). is_tail=true means ended with backtick, false means ${
    fn read_template_chars(&mut self) -> Result<(Option<String>, String, bool), LexError> {
        let mut cooked = Some(String::new());
        let mut raw = String::new();
        loop {
            match self.advance() {
                None => return Err(self.error("Unterminated template literal")),
                Some('`') => return Ok((cooked, raw, true)),
                Some('$') if self.peek() == Some('{') => {
                    self.advance();
                    return Ok((cooked, raw, false));
                }
                Some('\\') => {
                    raw.push('\\');
                    let before_offset = self.offset;
                    match self.read_escape_sequence() {
                        Ok(esc) => {
                            raw.push_str(&self.source[before_offset..self.offset]);
                            if let Some(ref mut c) = cooked {
                                c.push_str(&esc);
                            }
                        }
                        Err(_) => {
                            // Invalid escape: cooked becomes undefined, raw gets source chars
                            raw.push_str(&self.source[before_offset..self.offset]);
                            cooked = None;
                        }
                    }
                }
                Some(ch) if Self::is_line_terminator(ch) => {
                    if ch == '\r' && self.peek() == Some('\n') {
                        raw.push('\r');
                        raw.push('\n');
                    } else {
                        raw.push(ch);
                    }
                    self.handle_newline(ch);
                    if let Some(ref mut c) = cooked {
                        c.push('\n');
                    }
                }
                Some(ch) => {
                    raw.push(ch);
                    if let Some(ref mut c) = cooked {
                        c.push(ch);
                    }
                }
            }
        }
    }

    fn read_template_literal(&mut self) -> Result<Token, LexError> {
        let (cooked, raw, is_tail) = self.read_template_chars()?;
        if is_tail {
            Ok(Token::NoSubstitutionTemplate(cooked, raw))
        } else {
            Ok(Token::TemplateHead(cooked, raw))
        }
    }

    pub fn read_template_continuation(&mut self) -> Result<Token, LexError> {
        let (cooked, raw, is_tail) = self.read_template_chars()?;
        if is_tail {
            Ok(Token::TemplateTail(cooked, raw))
        } else {
            Ok(Token::TemplateMiddle(cooked, raw))
        }
    }

    fn read_punctuator(&mut self, ch: char) -> Result<Token, LexError> {
        match ch {
            '{' => Ok(Token::LeftBrace),
            '}' => Ok(Token::RightBrace),
            '(' => Ok(Token::LeftParen),
            ')' => Ok(Token::RightParen),
            '[' => Ok(Token::LeftBracket),
            ']' => Ok(Token::RightBracket),
            ';' => Ok(Token::Semicolon),
            ',' => Ok(Token::Comma),
            '~' => Ok(Token::Tilde),
            ':' => Ok(Token::Colon),

            '.' => {
                if self.peek() == Some('.') && self.peek_next() == Some('.') {
                    self.advance();
                    self.advance();
                    Ok(Token::Ellipsis)
                } else {
                    Ok(Token::Dot)
                }
            }

            '?' => {
                if self.peek() == Some('?') {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Ok(Token::NullishAssign)
                    } else {
                        Ok(Token::NullishCoalescing)
                    }
                } else if self.peek() == Some('.')
                    && !self.peek_next().is_some_and(|c| c.is_ascii_digit())
                {
                    self.advance();
                    Ok(Token::OptionalChain)
                } else {
                    Ok(Token::Question)
                }
            }

            '<' => {
                if self.peek() == Some('<') {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Ok(Token::LeftShiftAssign)
                    } else {
                        Ok(Token::LeftShift)
                    }
                } else if self.peek() == Some('=') {
                    self.advance();
                    Ok(Token::LessThanEqual)
                } else {
                    Ok(Token::LessThan)
                }
            }

            '>' => {
                if self.peek() == Some('>') {
                    self.advance();
                    if self.peek() == Some('>') {
                        self.advance();
                        if self.peek() == Some('=') {
                            self.advance();
                            Ok(Token::UnsignedRightShiftAssign)
                        } else {
                            Ok(Token::UnsignedRightShift)
                        }
                    } else if self.peek() == Some('=') {
                        self.advance();
                        Ok(Token::RightShiftAssign)
                    } else {
                        Ok(Token::RightShift)
                    }
                } else if self.peek() == Some('=') {
                    self.advance();
                    Ok(Token::GreaterThanEqual)
                } else {
                    Ok(Token::GreaterThan)
                }
            }

            '=' => {
                if self.peek() == Some('=') {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Ok(Token::StrictEqual)
                    } else {
                        Ok(Token::Equal)
                    }
                } else if self.peek() == Some('>') {
                    self.advance();
                    Ok(Token::Arrow)
                } else {
                    Ok(Token::Assign)
                }
            }

            '!' => {
                if self.peek() == Some('=') {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Ok(Token::StrictNotEqual)
                    } else {
                        Ok(Token::NotEqual)
                    }
                } else {
                    Ok(Token::Bang)
                }
            }

            '+' => {
                if self.peek() == Some('+') {
                    self.advance();
                    Ok(Token::Increment)
                } else if self.peek() == Some('=') {
                    self.advance();
                    Ok(Token::PlusAssign)
                } else {
                    Ok(Token::Plus)
                }
            }

            '-' => {
                if self.peek() == Some('-') {
                    self.advance();
                    Ok(Token::Decrement)
                } else if self.peek() == Some('=') {
                    self.advance();
                    Ok(Token::MinusAssign)
                } else {
                    Ok(Token::Minus)
                }
            }

            '*' => {
                if self.peek() == Some('*') {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Ok(Token::ExponentAssign)
                    } else {
                        Ok(Token::Exponent)
                    }
                } else if self.peek() == Some('=') {
                    self.advance();
                    Ok(Token::StarAssign)
                } else {
                    Ok(Token::Star)
                }
            }

            '/' => {
                if self.peek() == Some('=') {
                    self.advance();
                    Ok(Token::SlashAssign)
                } else {
                    Ok(Token::Slash)
                }
            }

            '%' => {
                if self.peek() == Some('=') {
                    self.advance();
                    Ok(Token::PercentAssign)
                } else {
                    Ok(Token::Percent)
                }
            }

            '&' => {
                if self.peek() == Some('&') {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Ok(Token::LogicalAndAssign)
                    } else {
                        Ok(Token::LogicalAnd)
                    }
                } else if self.peek() == Some('=') {
                    self.advance();
                    Ok(Token::AmpersandAssign)
                } else {
                    Ok(Token::Ampersand)
                }
            }

            '|' => {
                if self.peek() == Some('|') {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        Ok(Token::LogicalOrAssign)
                    } else {
                        Ok(Token::LogicalOr)
                    }
                } else if self.peek() == Some('=') {
                    self.advance();
                    Ok(Token::PipeAssign)
                } else {
                    Ok(Token::Pipe)
                }
            }

            '^' => {
                if self.peek() == Some('=') {
                    self.advance();
                    Ok(Token::CaretAssign)
                } else {
                    Ok(Token::Caret)
                }
            }

            _ => Err(self.error(format!("Unexpected character: {ch}"))),
        }
    }

    pub fn tokenize_all(&mut self) -> Result<Vec<Token>, LexError> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            if token == Token::Eof {
                tokens.push(token);
                break;
            }
            tokens.push(token);
        }
        Ok(tokens)
    }
}

fn hex_val(ch: char) -> Option<u32> {
    match ch {
        '0'..='9' => Some(ch as u32 - '0' as u32),
        'a'..='f' => Some(ch as u32 - 'a' as u32 + 10),
        'A'..='F' => Some(ch as u32 - 'A' as u32 + 10),
        _ => None,
    }
}

fn unicode_id_start(ch: char) -> bool {
    // Simplified: use Unicode properties for non-ASCII
    !ch.is_ascii() && unicode_ident::is_xid_start(ch)
}

fn unicode_id_continue(ch: char) -> bool {
    !ch.is_ascii() && unicode_ident::is_xid_continue(ch)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lex(src: &str) -> Vec<Token> {
        let mut lexer = Lexer::new(src);
        lexer.tokenize_all().unwrap()
    }

    fn lex_no_lt(src: &str) -> Vec<Token> {
        lex(src)
            .into_iter()
            .filter(|t| !matches!(t, Token::LineTerminator))
            .collect()
    }

    #[test]
    fn empty_source() {
        assert_eq!(lex(""), vec![Token::Eof]);
    }

    #[test]
    fn identifiers_and_keywords() {
        assert_eq!(
            lex_no_lt("var x = 42;"),
            vec![
                Token::Keyword(Keyword::Var),
                Token::Identifier("x".into()),
                Token::Assign,
                Token::NumericLiteral(42.0),
                Token::Semicolon,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn string_literals() {
        assert_eq!(
            lex_no_lt(r#""hello""#),
            vec![Token::StringLiteral("hello".into()), Token::Eof]
        );
        assert_eq!(
            lex_no_lt(r"'he\nllo'"),
            vec![Token::StringLiteral("he\nllo".into()), Token::Eof]
        );
    }

    #[test]
    fn numeric_literals() {
        assert_eq!(
            lex_no_lt("0xff"),
            vec![Token::NumericLiteral(255.0), Token::Eof]
        );
        assert_eq!(
            lex_no_lt("0b1010"),
            vec![Token::NumericLiteral(10.0), Token::Eof]
        );
        assert_eq!(
            lex_no_lt("0o77"),
            vec![Token::NumericLiteral(63.0), Token::Eof]
        );
        assert_eq!(
            lex_no_lt("1_000"),
            vec![Token::NumericLiteral(1000.0), Token::Eof]
        );
        assert_eq!(
            lex_no_lt("1e3"),
            vec![Token::NumericLiteral(1000.0), Token::Eof]
        );
    }

    #[test]
    fn boolean_null() {
        assert_eq!(
            lex_no_lt("true false null"),
            vec![
                Token::BooleanLiteral(true),
                Token::BooleanLiteral(false),
                Token::NullLiteral,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn punctuators() {
        assert_eq!(lex_no_lt("==="), vec![Token::StrictEqual, Token::Eof]);
        assert_eq!(lex_no_lt("!=="), vec![Token::StrictNotEqual, Token::Eof]);
        assert_eq!(lex_no_lt("=>"), vec![Token::Arrow, Token::Eof]);
        assert_eq!(lex_no_lt("..."), vec![Token::Ellipsis, Token::Eof]);
        assert_eq!(
            lex_no_lt(">>>="),
            vec![Token::UnsignedRightShiftAssign, Token::Eof]
        );
    }

    #[test]
    fn comments() {
        assert_eq!(
            lex_no_lt("// comment\n42"),
            vec![Token::NumericLiteral(42.0), Token::Eof]
        );
        assert_eq!(
            lex_no_lt("/* block */ 42"),
            vec![Token::NumericLiteral(42.0), Token::Eof]
        );
    }

    #[test]
    fn template_literal() {
        assert_eq!(
            lex_no_lt("`hello`"),
            vec![Token::NoSubstitutionTemplate(Some("hello".into()), "hello".into()), Token::Eof]
        );
    }

    #[test]
    fn bigint_literal() {
        assert_eq!(
            lex_no_lt("42n"),
            vec![Token::BigIntLiteral("42".into()), Token::Eof]
        );
        assert_eq!(
            lex_no_lt("0xFFn"),
            vec![Token::BigIntLiteral("0xFF".into()), Token::Eof]
        );
    }

    #[test]
    fn unicode_escape_in_string() {
        assert_eq!(
            lex_no_lt(r#""\u0041""#),
            vec![Token::StringLiteral("A".into()), Token::Eof]
        );
        assert_eq!(
            lex_no_lt(r#""\u{1F600}""#),
            vec![Token::StringLiteral("😀".into()), Token::Eof]
        );
    }
}
