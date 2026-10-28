/// ES lookbehinds match right-to-left and never backtrack into their captures.
/// fancy-regex 0.20 matches variable-length hard lookbehinds forward, so a
/// lookbehind capture of unbounded width read by a backref outside it
/// (`/(?<=([abc]+)).\1/`) must go to the custom RTL matcher.
pub(super) fn unbounded_capture_read_outside(source: &str) -> bool {
    super::regexp::lookbehind_captures_with_external_backrefs(source).is_some()
        && super::regexp_lookbehind::extract_lookbehinds(source)
            .0
            .iter()
            .any(|lb| lb.content.contains(['+', '*']))
}
