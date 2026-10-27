//! The source registry behind `--node` stack traces: which file a function's
//! source belongs to, and the line and column of an offset in it.

use std::collections::HashMap;
use std::rc::{Rc, Weak};

pub(super) struct SourceEntry {
    pub(super) source: Rc<str>,
    pub(super) name: Option<Rc<str>>,
    pub(super) line_delta: i32,
    column_delta: i32,
    column_line: usize,
    line_starts: Option<Vec<u32>>,
}

/// A `vm` script or function source whose name and offsets apply once a frame
/// first mentions it; held weakly so a source nobody can reach is not kept.
struct PendingName {
    source: Weak<str>,
    name: Rc<str>,
    line_delta: i32,
    column_delta: i32,
    column_line: usize,
}

#[derive(Default)]
pub(crate) struct NodeStackState {
    pub(super) installed: bool,
    pub(super) sources: Vec<SourceEntry>,
    pub(super) scripts: Vec<(Rc<str>, String)>,
    /// Keyed by the address of the source's allocation; an entry whose source
    /// is gone is ignored, and dead ones are swept as the table doubles.
    pending: HashMap<usize, PendingName>,
    prune_at: usize,
    /// Native functions standing in for a running `vm` script's top level.
    pub(super) script_frames: Vec<(u64, Rc<str>)>,
    /// Source id and offset plus one of the last `throw` statement run.
    pub(super) throw_site: (usize, u32),
    /// The source of the last noted throw, its id and whether it is a `node:` one.
    pub(super) throw_source: Option<(Rc<str>, usize, bool)>,
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
        let (pending_name, line_delta, column_delta, column_line) = self
            .pending
            .get(&(Rc::as_ptr(source) as *const u8 as usize))
            .filter(|p| p.source.strong_count() > 0)
            .map_or((None, 0, 0, 0), |p| {
                (
                    Some(p.name.clone()),
                    p.line_delta,
                    p.column_delta,
                    p.column_line,
                )
            });
        let name = match pending_name {
            Some(file) => source_url(source).or(Some(file)),
            None => noted.or_else(|| source_url(source)),
        };
        self.sources.push(SourceEntry {
            source: source.clone(),
            name,
            line_delta,
            column_delta,
            column_line,
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
        let first_line_shift = if line == entry.column_line {
            entry.column_delta
        } else {
            0
        };
        let column =
            (text.encode_utf16().count() as i64 + 1 + i64::from(first_line_shift)).max(1) as usize;
        let line = (line as i64 + 1 + i64::from(entry.line_delta)).max(1) as usize;
        Some((entry.name.clone(), line, column))
    }
}

impl NodeStackState {
    /// Name `source` for the frames that will mention it: `name`, with line
    /// numbers shifted by `line_delta` and the columns of line `column_line`
    /// (0-based) by `column_delta`.
    pub(crate) fn name_pending(
        &mut self,
        source: &Rc<str>,
        name: &str,
        line_delta: i32,
        column_delta: i32,
        column_line: usize,
    ) {
        if self.pending.len() >= self.prune_at.max(64) {
            self.pending.retain(|_, p| p.source.strong_count() > 0);
            self.prune_at = self.pending.len() * 2;
        }
        self.pending.insert(
            Rc::as_ptr(source) as *const u8 as usize,
            PendingName {
                source: Rc::downgrade(source),
                name: Rc::from(name),
                line_delta,
                column_delta,
                column_line,
            },
        );
    }

    pub(super) fn script_frame_source(&self, func: u64) -> Option<Rc<str>> {
        self.script_frames
            .iter()
            .rev()
            .find(|(id, _)| *id == func)
            .map(|(_, source)| source.clone())
    }
}
