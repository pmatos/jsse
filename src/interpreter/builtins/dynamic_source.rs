use crate::ast::{FunctionExpr, SourceText};

/// The source text of a `Function`-family result: the parser's own, so the
/// wrapper shares one origin with the functions nested in it (stack traces
/// name an origin), falling back to a copy of `text`.
pub(super) fn of(parsed: &FunctionExpr, text: String, pua: bool) -> SourceText {
    match &parsed.source_text {
        Some(t) if t.as_str() == text => t.clone(),
        _ => SourceText::from(text),
    }
    .with_pua_encoded(pua)
}
