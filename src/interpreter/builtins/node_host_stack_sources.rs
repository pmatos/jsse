//! The source registry behind `--node` stack traces: which file a function's
//! source belongs to, and the line and column of an offset in it.

use std::rc::Rc;

pub(super) struct SourceEntry {
    pub(super) source: Rc<str>,
    pub(super) name: Option<Rc<str>>,
    pub(super) line_delta: i32,
    line_starts: Option<Vec<u32>>,
}

#[derive(Default)]
pub(crate) struct NodeStackState {
    pub(super) installed: bool,
    pub(super) sources: Vec<SourceEntry>,
    pub(super) scripts: Vec<(Rc<str>, String)>,
}

/// The last `//# sourceURL=` directive that sits on a line of its own, so a
/// string literal that merely mentions one is not mistaken for it.
fn source_url(source: &str) -> Option<Rc<str>> {
    let at = source.rfind("sourceURL=")?;
    let line = &source[source[..at].rfind('\n').map_or(0, |n| n + 1)..at];
    let comment = line.trim_start_matches([' ', '\t']);
    if comment != "//# " && comment != "//@ " {
        return None;
    }
    let url = source[at + 10..].split(['\n', '\r', ' ', '\t']).next()?;
    (!url.is_empty()).then(|| Rc::from(url))
}

fn line_starts(source: &str) -> Vec<u32> {
    let bytes = source.as_bytes();
    let mut starts = vec![0u32];
    let mut i = 0;
    while i < bytes.len() {
        let width = match bytes[i] {
            b'\n' => 1,
            b'\r' if bytes.get(i + 1) == Some(&b'\n') => 2,
            b'\r' => 1,
            0xE2 if bytes.get(i + 1) == Some(&0x80)
                && matches!(bytes.get(i + 2), Some(0xA8 | 0xA9)) =>
            {
                3
            }
            _ => 0,
        };
        if width == 0 {
            i += 1;
        } else {
            i += width;
            starts.push(i as u32);
        }
    }
    starts
}

impl NodeStackState {
    pub(super) fn source_id(&mut self, source: &Rc<str>) -> usize {
        if let Some(id) = self
            .sources
            .iter()
            .position(|e| Rc::ptr_eq(&e.source, source))
        {
            return id;
        }
        let noted = self
            .scripts
            .iter()
            .find(|(_, text)| text.len() == source.len() && text.as_str() == &**source)
            .map(|(name, _)| name.clone());
        self.sources.push(SourceEntry {
            source: source.clone(),
            name: noted.or_else(|| source_url(source)),
            line_delta: 0,
            line_starts: None,
        });
        self.sources.len() - 1
    }

    pub(super) fn locate(
        &mut self,
        id: usize,
        pos: u32,
    ) -> Option<(Option<Rc<str>>, usize, usize)> {
        let entry = self.sources.get_mut(id)?;
        let offset = (pos.checked_sub(1)? as usize).min(entry.source.len());
        let starts = entry
            .line_starts
            .get_or_insert_with(|| line_starts(&entry.source));
        let line = starts.partition_point(|&s| s as usize <= offset) - 1;
        let from = starts[line] as usize;
        let text = entry.source.get(from..offset).unwrap_or("");
        let column = text.encode_utf16().count() + 1;
        let line = (line as i64 + 1 + i64::from(entry.line_delta)).max(1) as usize;
        Some((entry.name.clone(), line, column))
    }
}
