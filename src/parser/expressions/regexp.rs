use super::*;

pub(super) fn validate_regexp_literal(pattern: &[u16], flags: &str) -> Result<(), ParseError> {
    let valid_flags = "dgimsuyv";
    for c in flags.chars() {
        if !valid_flags.contains(c) {
            return Err(ParseError {
                message: format!("Invalid regular expression flags '{}'", flags),
            });
        }
    }
    let mut seen = HashSet::new();
    for c in flags.chars() {
        if !seen.insert(c) {
            return Err(ParseError {
                message: format!("Invalid regular expression flags '{}'", flags),
            });
        }
    }
    if flags.contains('u') && flags.contains('v') {
        return Err(ParseError {
            message: format!("Invalid regular expression flags '{}'", flags),
        });
    }
    if let Err(msg) = crate::interpreter::validate_js_pattern_units(pattern, flags) {
        return Err(ParseError { message: msg });
    }
    Ok(())
}
