//! Dispatch tables for `switch` statements whose case selectors are all number
//! or string literals (#1491).
//!
//! Such selectors have no side effects, so CaseBlockEvaluation
//! (sec-runtime-semantics-caseblockevaluation) reduces to finding the first
//! clause whose selector `===` the discriminant. `===` never coerces, so a
//! number can only match a number selector and a string a string selector;
//! every other discriminant, and `NaN`, falls to the `default` clause.

use crate::types::JsValue;
use rustc_hash::{FxHashMap, FxHashSet};

/// A case selector the table dispatches on.
#[derive(Debug, Clone)]
pub(crate) enum CaseKey {
    Number(f64),
    String(Vec<u16>),
}

const ABSENT: u32 = u32::MAX;
/// Widest integer span a dense table covers.
const DENSE_MAX_SPAN: i64 = 4096;

#[derive(Debug, Default)]
enum Numbers {
    #[default]
    Empty,
    /// `targets[n - base]` for the integers `n` in `[base, base + len)`.
    Dense {
        base: i32,
        targets: Vec<u32>,
    },
    Sparse(FxHashMap<u64, u32>),
}

#[derive(Debug, Default)]
pub(crate) struct SwitchTable {
    numbers: Numbers,
    strings: FxHashMap<Box<[u16]>, u32>,
    /// Where a discriminant that matches no selector goes: the `default`
    /// clause's body, or the end of the statement.
    default_target: u32,
}

/// `+0` and `-0` are `===`, so they share one key; `NaN` is never `===`
/// anything and so has none.
fn number_key(n: f64) -> Option<u64> {
    if n.is_nan() {
        None
    } else if n == 0.0 {
        Some(0)
    } else {
        Some(n.to_bits())
    }
}

impl SwitchTable {
    /// `clauses` lists each case's selector and the code offset of its body in
    /// source order. A selector equal to an earlier one never wins: the first
    /// matching clause starts the evaluation.
    pub(crate) fn build(clauses: &[(CaseKey, u32)], default_target: u32) -> SwitchTable {
        let mut strings: FxHashMap<Box<[u16]>, u32> = FxHashMap::default();
        let mut numbers: Vec<(f64, u32)> = Vec::new();
        let mut seen: FxHashSet<u64> = FxHashSet::default();
        for (key, target) in clauses {
            match key {
                CaseKey::String(units) => {
                    strings.entry(units.as_slice().into()).or_insert(*target);
                }
                CaseKey::Number(n) => {
                    if number_key(*n).is_some_and(|key| seen.insert(key)) {
                        numbers.push((*n, *target));
                    }
                }
            }
        }
        SwitchTable {
            numbers: Self::number_table(&numbers),
            strings,
            default_target,
        }
    }

    fn number_table(numbers: &[(f64, u32)]) -> Numbers {
        if numbers.is_empty() {
            return Numbers::Empty;
        }
        let integers: Option<Vec<(i32, u32)>> = numbers
            .iter()
            .map(|&(n, target)| {
                let i = n as i32;
                (f64::from(i) == n).then_some((i, target))
            })
            .collect();
        if let Some(integers) = integers {
            let lo = integers.iter().map(|e| e.0).min().unwrap_or(0);
            let hi = integers.iter().map(|e| e.0).max().unwrap_or(0);
            let span = i64::from(hi) - i64::from(lo) + 1;
            if span <= DENSE_MAX_SPAN && span <= 16 * integers.len() as i64 + 64 {
                let mut targets = vec![ABSENT; span as usize];
                for (i, target) in integers {
                    targets[(i64::from(i) - i64::from(lo)) as usize] = target;
                }
                return Numbers::Dense { base: lo, targets };
            }
        }
        Numbers::Sparse(
            numbers
                .iter()
                .filter_map(|&(n, target)| Some((number_key(n)?, target)))
                .collect(),
        )
    }

    /// The code offset a switch on `discriminant` continues at.
    #[inline]
    pub(crate) fn target(&self, discriminant: &JsValue) -> u32 {
        let found = if let Some(n) = discriminant.as_number() {
            self.number_target(n)
        } else {
            discriminant
                .with_string(|units| self.strings.get(units).copied())
                .flatten()
        };
        found.unwrap_or(self.default_target)
    }

    #[inline]
    fn number_target(&self, n: f64) -> Option<u32> {
        match &self.numbers {
            Numbers::Empty => None,
            Numbers::Dense { base, targets } => {
                let i = n as i32;
                if f64::from(i) != n {
                    return None;
                }
                let at = usize::try_from(i64::from(i) - i64::from(*base)).ok()?;
                targets.get(at).copied().filter(|t| *t != ABSENT)
            }
            Numbers::Sparse(map) => map.get(&number_key(n)?).copied(),
        }
    }
}
