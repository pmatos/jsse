//! V8's `JSON.parse` error messages under `--node` (#1510).
//!
//! The engine's own parser decides whether a text is valid. When it rejects
//! one and the stack hooks are installed, this scanner walks the text the way
//! V8's `JsonParser` does and words the first error as V8 does, with its
//! position, line and column. If it finds no error (the two disagree), the
//! engine's message stays.

use super::{JsonText, SourceTextMap, json_parse_value_inner};
use crate::interpreter::{Completion, Interpreter};

const CONTEXT: usize = 10;

struct Scanner<'a> {
    units: &'a [u16],
    pos: usize,
}

type Failure = String;

impl Scanner<'_> {
    fn at(&self, pos: usize) -> Option<u16> {
        self.units.get(pos).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.at(self.pos), Some(0x20 | 0x09 | 0x0A | 0x0D)) {
            self.pos += 1;
        }
    }

    fn located(&self, what: &str, pos: usize) -> Failure {
        let mut line = 1;
        let mut start = 0;
        let mut i = 0;
        while i < pos.min(self.units.len()) {
            let unit = self.units[i];
            if unit == 0x0A || (unit == 0x0D && self.at(i + 1) != Some(0x0A)) {
                line += 1;
                start = i + 1;
            }
            i += 1;
        }
        format!(
            "{what} in JSON at position {pos} (line {line} column {})",
            pos - start + 1
        )
    }

    fn text(&self, from: usize, to: usize) -> String {
        String::from_utf16_lossy(&self.units[from.min(self.units.len())..to.min(self.units.len())])
    }

    /// V8's `LookUpErrorMessageForJsonToken` for the unit at the cursor.
    fn unexpected(&self) -> Failure {
        let Some(unit) = self.at(self.pos) else {
            return "Unexpected end of JSON input".to_string();
        };
        match unit {
            0x22 => return self.located("Unexpected string", self.pos),
            0x2D | 0x30..=0x39 => return self.located("Unexpected number", self.pos),
            _ => {}
        }
        let whole = self.text(0, self.units.len());
        if ["NaN", "Infinity", "undefined", "[object Object]"].contains(&whole.as_str()) {
            return format!("\"{whole}\" is not valid JSON");
        }
        let token = self.text(self.pos, self.pos + 1);
        let len = self.units.len();
        if len < 2 * CONTEXT + 1 {
            return format!("Unexpected token '{token}', \"{whole}\" is not valid JSON");
        }
        if self.pos < CONTEXT {
            let shown = self.text(0, self.pos + CONTEXT);
            format!("Unexpected token '{token}', \"{shown}\"... is not valid JSON")
        } else if self.pos < len - CONTEXT {
            let shown = self.text(self.pos - CONTEXT, self.pos + CONTEXT);
            format!("Unexpected token '{token}', ...\"{shown}\"... is not valid JSON")
        } else {
            let shown = self.text(self.pos - CONTEXT, len);
            format!("Unexpected token '{token}', ...\"{shown}\" is not valid JSON")
        }
    }

    fn expect(&mut self, unit: u16, what: &str) -> Result<(), Failure> {
        self.skip_ws();
        if self.at(self.pos) == Some(unit) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.located(what, self.pos))
        }
    }

    fn string(&mut self) -> Result<(), Failure> {
        self.pos += 1;
        loop {
            let Some(unit) = self.at(self.pos) else {
                return Err(self.located("Unterminated string", self.units.len()));
            };
            match unit {
                0x22 => {
                    self.pos += 1;
                    return Ok(());
                }
                0..=0x1F => {
                    return Err(self.located("Bad control character in string literal", self.pos));
                }
                0x5C => {
                    self.pos += 1;
                    match self.at(self.pos) {
                        Some(0x22 | 0x5C | 0x2F | 0x62 | 0x66 | 0x6E | 0x72 | 0x74) => {}
                        Some(0x75) => {
                            for _ in 0..4 {
                                self.pos += 1;
                                let hex =
                                    self.at(self.pos).and_then(|u| char::from_u32(u32::from(u)));
                                if !hex.is_some_and(|c| c.is_ascii_hexdigit()) {
                                    return Err(self.located("Bad Unicode escape", self.pos));
                                }
                            }
                        }
                        None => return Err(self.located("Unterminated string", self.units.len())),
                        Some(_) => return Err(self.located("Bad escaped character", self.pos)),
                    }
                    self.pos += 1;
                }
                _ => self.pos += 1,
            }
        }
    }

    fn digits(&mut self) -> bool {
        let start = self.pos;
        while matches!(self.at(self.pos), Some(0x30..=0x39)) {
            self.pos += 1;
        }
        self.pos > start
    }

    fn number(&mut self) -> Result<(), Failure> {
        if self.at(self.pos) == Some(0x2D) {
            self.pos += 1;
            if !matches!(self.at(self.pos), Some(0x30..=0x39)) {
                return Err(self.located("No number after minus sign", self.pos));
            }
        }
        if self.at(self.pos) == Some(0x30) {
            self.pos += 1;
            if matches!(self.at(self.pos), Some(0x30..=0x39)) {
                return Err(self.located("Unexpected number", self.pos));
            }
        } else {
            self.digits();
        }
        if self.at(self.pos) == Some(0x2E) {
            self.pos += 1;
            if !self.digits() {
                return Err(self.located("Unterminated fractional number", self.pos));
            }
        }
        if matches!(self.at(self.pos), Some(0x65 | 0x45)) {
            self.pos += 1;
            if matches!(self.at(self.pos), Some(0x2B | 0x2D)) {
                self.pos += 1;
            }
            if !self.digits() {
                return Err(self.located("Exponent part is missing a number", self.pos));
            }
        }
        Ok(())
    }

    fn literal(&mut self, word: &str) -> Result<(), Failure> {
        for expected in word.bytes() {
            if self.at(self.pos) != Some(u16::from(expected)) {
                return Err(self.unexpected());
            }
            self.pos += 1;
        }
        Ok(())
    }

    fn value(&mut self) -> Result<(), Failure> {
        self.skip_ws();
        match self.at(self.pos) {
            Some(0x22) => self.string(),
            Some(0x2D | 0x30..=0x39) => self.number(),
            Some(0x74) => self.literal("true"),
            Some(0x66) => self.literal("false"),
            Some(0x6E) => self.literal("null"),
            Some(0x7B) => self.object(),
            Some(0x5B) => self.array(),
            _ => Err(self.unexpected()),
        }
    }

    fn object(&mut self) -> Result<(), Failure> {
        self.pos += 1;
        self.skip_ws();
        if self.at(self.pos) == Some(0x7D) {
            self.pos += 1;
            return Ok(());
        }
        let mut key_error = "Expected property name or '}'";
        loop {
            self.skip_ws();
            if self.at(self.pos) != Some(0x22) {
                return Err(self.located(key_error, self.pos));
            }
            self.string()?;
            self.expect(0x3A, "Expected ':' after property name")?;
            self.value()?;
            self.skip_ws();
            match self.at(self.pos) {
                Some(0x2C) => {
                    self.pos += 1;
                    key_error = "Expected double-quoted property name";
                }
                Some(0x7D) => {
                    self.pos += 1;
                    return Ok(());
                }
                _ => {
                    return Err(self.located("Expected ',' or '}' after property value", self.pos));
                }
            }
        }
    }

    fn array(&mut self) -> Result<(), Failure> {
        self.pos += 1;
        self.skip_ws();
        if self.at(self.pos) == Some(0x5D) {
            self.pos += 1;
            return Ok(());
        }
        loop {
            self.value()?;
            self.skip_ws();
            match self.at(self.pos) {
                Some(0x2C) => self.pos += 1,
                Some(0x5D) => {
                    self.pos += 1;
                    return Ok(());
                }
                _ => return Err(self.located("Expected ',' or ']' after array element", self.pos)),
            }
        }
    }
}

/// V8's message for the first error in `text`, if there is one.
pub(super) fn v8_json_message(text: &str) -> Option<String> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let mut scanner = Scanner {
        units: &units,
        pos: 0,
    };
    let outcome = scanner.value().and_then(|()| {
        scanner.skip_ws();
        if scanner.pos < units.len() {
            Err(scanner.located(
                "Unexpected non-whitespace character after JSON",
                scanner.pos,
            ))
        } else {
            Ok(())
        }
    });
    outcome.err()
}

fn word(interp: &mut Interpreter, input: &JsonText, result: Completion) -> Completion {
    if matches!(result, Completion::Throw(_))
        && interp.node_host.stack.installed
        && !input.stand_ins
        && let Some(message) = v8_json_message(&input.text)
    {
        return Completion::Throw(interp.create_error("SyntaxError", &message));
    }
    result
}

pub(crate) fn json_parse_value(interp: &mut Interpreter, input: &JsonText) -> Completion {
    let result = json_parse_value_inner(interp, &input.text, input.stand_ins, None, &input.text);
    word(interp, input, result)
}

pub(crate) fn json_parse_value_with_source(
    interp: &mut Interpreter,
    input: &JsonText,
) -> (Completion, SourceTextMap) {
    let mut source_map = SourceTextMap::default();
    let result = json_parse_value_inner(
        interp,
        &input.text,
        input.stand_ins,
        Some(&mut source_map),
        &input.text,
    );
    (word(interp, input, result), source_map)
}
