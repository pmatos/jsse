//! Launcher-side decisions for `--node` (#1219): whether the main program is a
//! CommonJS module, an ES module or an `-e`/REPL program. The prelude's
//! CommonJS loader acts on the answer (`__host_main_kind`).

use std::path::Path;

#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub(crate) enum NodeMainKind {
    /// A script file, run as the CommonJS main module.
    Cjs,
    /// A script file run as an ES module: no `require`.
    Esm,
    /// `-e` or the REPL: `require` and `module` are globals.
    #[default]
    Eval,
}

impl NodeMainKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Cjs => "cjs",
            Self::Esm => "esm",
            Self::Eval => "eval",
        }
    }

    /// `.mjs`, `--module`, and `.js` or extensionless files under a
    /// `"type": "module"` package are ES modules; every other script file is
    /// CommonJS.
    pub(crate) fn for_launch(file: Option<&Path>, eval: bool, module: bool) -> Self {
        match file {
            Some(path) if !eval => {
                let ext = path.extension();
                if module
                    || ext.is_some_and(|e| e == "mjs")
                    || (ext.is_none_or(|e| e == "js") && package_type_is_module(path))
                {
                    Self::Esm
                } else {
                    Self::Cjs
                }
            }
            _ => Self::Eval,
        }
    }
}

/// The nearest package.json above `file`, not crossing a `node_modules`
/// directory, declares `"type": "module"`.
fn package_type_is_module(file: &Path) -> bool {
    let Ok(file) = file.canonicalize() else {
        return false;
    };
    let mut dir = file.parent();
    while let Some(d) = dir {
        if d.file_name().is_some_and(|n| n == "node_modules") || d.parent().is_none() {
            return false;
        }
        if let Ok(text) = std::fs::read_to_string(d.join("package.json")) {
            return top_level_string(&text, "type").as_deref() == Some("module");
        }
        dir = d.parent();
    }
    false
}

/// The string value of `key` among the top-level members of a JSON object.
fn top_level_string(json: &str, key: &str) -> Option<String> {
    let b = json.as_bytes();
    let skip_ws = |mut i: usize| {
        while b.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        i
    };
    let mut depth = 0usize;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'{' | b'[' => {
                depth += 1;
                i += 1;
            }
            b'}' | b']' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            b'"' => {
                let (s, next) = read_string(b, i)?;
                i = next;
                let colon = skip_ws(i);
                if depth == 1 && s == key && b.get(colon) == Some(&b':') {
                    let value = skip_ws(colon + 1);
                    return (b.get(value) == Some(&b'"'))
                        .then(|| read_string(b, value))
                        .flatten()
                        .map(|(v, _)| v);
                }
            }
            _ => i += 1,
        }
    }
    None
}

/// The string starting at the quote `start` and the index after it. Escapes
/// are kept verbatim: only plain values such as `"module"` matter here.
fn read_string(b: &[u8], start: usize) -> Option<(String, usize)> {
    let mut i = start + 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => {
                let s = String::from_utf8_lossy(&b[start + 1..i]).into_owned();
                return Some((s, i + 1));
            }
            _ => i += 1,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_is_read_from_the_top_level_only() {
        let t = |j: &str| top_level_string(j, "type");
        assert_eq!(t(r#"{"type":"module"}"#).as_deref(), Some("module"));
        assert_eq!(
            t("{\n  \"name\": \"x\",\n  \"type\" : \"module\"\n}").as_deref(),
            Some("module")
        );
        assert_eq!(
            t(r#"{"exports":{"type":"module"},"type":"commonjs"}"#).as_deref(),
            Some("commonjs")
        );
        assert_eq!(t(r#"{"exports":{"type":"module"}}"#), None);
        assert_eq!(t(r#"{"description":"type: \"module\"","x":1}"#), None);
        assert_eq!(t(r#"{"type":true}"#), None);
        assert_eq!(t("not json"), None);
    }

    #[test]
    fn kinds_follow_the_extension_and_flags() {
        let k = NodeMainKind::for_launch;
        assert_eq!(k(None, false, false), NodeMainKind::Eval);
        assert_eq!(k(Some(Path::new("a.js")), true, false), NodeMainKind::Eval);
        assert_eq!(
            k(Some(Path::new("/nonexistent/a.js")), false, false),
            NodeMainKind::Cjs
        );
        assert_eq!(
            k(Some(Path::new("/nonexistent/a.mjs")), false, false),
            NodeMainKind::Esm
        );
        assert_eq!(
            k(Some(Path::new("/nonexistent/a.js")), false, true),
            NodeMainKind::Esm
        );
    }
}
