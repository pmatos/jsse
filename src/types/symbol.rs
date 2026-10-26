use super::{
    JsPropertyKey, JsString, SYMBOL_PROPERTY_KEY_SIGIL, WELL_KNOWN_SYMBOL_KEYS,
    WELL_KNOWN_SYMBOL_NAMES, push_wtf8,
};
use std::sync::{Arc, OnceLock, Weak};

#[derive(Debug)]
pub(super) struct JsSymbolData {
    id: u64,
    description: Option<JsString>,
    /// Index into `WELL_KNOWN_SYMBOL_NAMES` for a realm intrinsic well-known
    /// symbol (§6.1.5.1); `None` for every other symbol.
    well_known: Option<u8>,
    /// Whether `Symbol.for` created this symbol, i.e. `KeyForSymbol` is not
    /// `undefined` (§20.4.2.6); only such symbols are not weakly holdable
    /// (§9.13 CanBeHeldWeakly). Distinct from the `SymbolIndex` below.
    registered: bool,
    /// The Symbol-valued property key of a non-well-known symbol, built on
    /// first use. It holds bytes only, never the symbol, so there is no
    /// reference cycle.
    key: OnceLock<JsPropertyKey>,
}

/// Live symbols by id, for the key-to-symbol direction of a property key
/// round trip. A symbol enters when its key is first built, so every user
/// symbol key in existence has a table entry for as long as the symbol lives.
/// The entries are `Weak`: the table never keeps a symbol alive, and dead
/// entries are swept whenever the table doubles. The table is per thread: an
/// interpreter, and so every symbol it can reach, is confined to one thread.
struct SymbolIndex {
    by_id: std::collections::HashMap<u64, Weak<JsSymbolData>, foldhash::fast::RandomState>,
    sweep_at: usize,
}

impl SymbolIndex {
    const MIN_SWEEP: usize = 1024;

    fn insert(&mut self, data: &Arc<JsSymbolData>) {
        if self.by_id.len() >= self.sweep_at {
            self.by_id.retain(|_, weak| weak.strong_count() > 0);
            self.sweep_at = (self.by_id.len() * 2).max(Self::MIN_SWEEP);
        }
        self.by_id.insert(data.id, Arc::downgrade(data));
    }
}

thread_local! {
    static SYMBOL_INDEX: std::cell::RefCell<SymbolIndex> = std::cell::RefCell::new(SymbolIndex {
        by_id: Default::default(),
        sweep_at: SymbolIndex::MIN_SWEEP,
    });
}

#[inline(never)]
fn register_symbol(data: &Arc<JsSymbolData>) {
    let _ = SYMBOL_INDEX.try_with(|index| index.borrow_mut().insert(data));
}

#[inline(never)]
fn registered_symbol(id: u64) -> Option<Arc<JsSymbolData>> {
    SYMBOL_INDEX
        .try_with(|index| index.borrow().by_id.get(&id)?.upgrade())
        .ok()?
}

#[derive(Clone, Debug)]
pub(crate) struct JsSymbol {
    pub(super) data: Arc<JsSymbolData>,
}

impl JsSymbol {
    pub(crate) fn new(id: u64, description: Option<JsString>) -> Self {
        Self {
            data: Arc::new(JsSymbolData {
                id,
                description,
                well_known: None,
                registered: false,
                key: OnceLock::new(),
            }),
        }
    }

    /// A symbol created by `Symbol.for` (§20.4.2.2), whose description is its
    /// registry key.
    pub(crate) fn for_key(id: u64, key: JsString) -> Self {
        Self {
            data: Arc::new(JsSymbolData {
                id,
                description: Some(key),
                well_known: None,
                registered: true,
                key: OnceLock::new(),
            }),
        }
    }

    /// The well-known symbol `%Symbol.<name>%` (§6.1.5.1). Panics if `name`
    /// is not one of the well-known symbol names.
    pub(crate) fn new_well_known(id: u64, name: &str) -> Self {
        let index = WELL_KNOWN_SYMBOL_NAMES
            .iter()
            .position(|known| *known == name)
            .expect("not a well-known symbol name");
        Self {
            data: Arc::new(JsSymbolData {
                id,
                description: Some(JsString::from_str(&format!("Symbol.{name}"))),
                well_known: Some(index as u8),
                registered: false,
                key: OnceLock::new(),
            }),
        }
    }

    pub(crate) fn id(&self) -> u64 {
        self.data.id
    }

    #[inline]
    pub(crate) fn is_registered(&self) -> bool {
        self.data.registered
    }

    pub(crate) fn description(&self) -> Option<&JsString> {
        self.data.description.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn shares_storage_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.data, &other.data)
    }

    /// Convert to the internal Symbol-valued property key. Well-known symbols
    /// use a stable key without an id, so bootstrap lookups can construct the
    /// same key directly; every other symbol includes its unique id. The key
    /// also carries the description (WTF-8) so it can be recovered from the key
    /// alone; identity is the id, never the description. A user symbol builds
    /// its key once, so later conversions are a refcount bump. The memo is
    /// deliberately not interned: reads must not fill the bounded interner, so
    /// only a property store admits a symbol key to it, as before.
    #[inline]
    pub(crate) fn to_property_key(&self) -> JsPropertyKey {
        if let Some(index) = self.data.well_known {
            return WELL_KNOWN_SYMBOL_KEYS[index as usize].clone();
        }
        match self.data.key.get() {
            Some(key) => key.clone(),
            None => self.init_property_key(),
        }
    }

    #[cold]
    #[inline(never)]
    fn init_property_key(&self) -> JsPropertyKey {
        self.data
            .key
            .get_or_init(|| {
                register_symbol(&self.data);
                self.build_property_key()
            })
            .clone()
    }

    fn build_property_key(&self) -> JsPropertyKey {
        use std::io::Write as _;
        let description_len = self.description().map_or(0, |d| d.len() * 3);
        let mut bytes = Vec::with_capacity(description_len + 32);
        bytes.push(SYMBOL_PROPERTY_KEY_SIGIL);
        match self.description() {
            Some(desc) => {
                bytes.extend_from_slice(b"Symbol(");
                push_wtf8(desc.as_units(), &mut bytes);
                bytes.extend_from_slice(b")#");
            }
            None => bytes.extend_from_slice(b"Symbol#"),
        }
        let _ = write!(bytes, "{}", self.id());
        JsPropertyKey {
            bytes: Arc::from(bytes),
        }
    }

    /// The live symbol that `key` was built from, if it still exists. The
    /// result is the original symbol, so identity and description survive a
    /// round trip through a property key; `None` for a string key, a
    /// well-known symbol key, or a symbol that has since been dropped.
    pub(crate) fn from_property_key(key: &JsPropertyKey) -> Option<Self> {
        let encoding = key.symbol_encoding()?;
        let hash = encoding.iter().rposition(|&b| b == b'#')?;
        let id: u64 = std::str::from_utf8(&encoding[hash + 1..])
            .ok()?
            .parse()
            .ok()?;
        let data = registered_symbol(id)?;
        // Another interpreter on this thread may have reused the id; an entry
        // whose key differs is not this key's symbol.
        let own_key = data.key.get()?;
        (own_key == key).then_some(Self { data })
    }

    pub(crate) fn descriptive_string(&self) -> JsString {
        let desc = self.description().map_or(&[][..], JsString::as_units);
        let mut units = Vec::with_capacity(desc.len() + 8);
        units.extend("Symbol(".encode_utf16());
        units.extend_from_slice(desc);
        units.push(u16::from(b')'));
        JsString::from_vec(units)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_property_key_is_built_once_per_symbol() {
        let long = JsString::from_str(&"m".repeat(1000));
        for description in [JsString::from_str("memo"), long] {
            let symbol = JsSymbol::new(501, Some(description));
            let first = symbol.to_property_key();
            assert!(first.same_allocation(&symbol.to_property_key()));
            assert!(first.same_allocation(&symbol.clone().to_property_key()));
        }
    }

    #[test]
    fn property_key_round_trips_to_the_original_symbol() {
        let descriptions = [
            None,
            Some(vec![]),
            Some(vec![0xD800, 0x62]),
            Some("x)#9".encode_utf16().collect::<Vec<u16>>()),
        ];
        for (i, description) in descriptions.into_iter().enumerate() {
            let symbol = JsSymbol::new(600 + i as u64, description.map(JsString::from_vec));
            let key = symbol.to_property_key();
            let back = JsSymbol::from_property_key(&key).expect("symbol is alive");
            assert!(back.shares_storage_with(&symbol));
        }
        assert!(JsSymbol::from_property_key(&JsPropertyKey::from_str("Symbol#600")).is_none());
        let well_known = JsSymbol::new_well_known(700, "iterator").to_property_key();
        assert!(JsSymbol::from_property_key(&well_known).is_none());
    }

    #[test]
    fn symbol_index_does_not_keep_symbols_alive() {
        let symbol = JsSymbol::new(800, Some(JsString::from_str("transient")));
        let key = symbol.to_property_key();
        let weak = Arc::downgrade(&symbol.data);
        drop(symbol);
        assert_eq!(weak.strong_count(), 0);
        assert!(JsSymbol::from_property_key(&key).is_none());
        // The key alone still decodes to the description and id.
        let (description, id) = key.user_symbol_parts().expect("user symbol key");
        assert_eq!(id, 800);
        assert_eq!(description.map(|d| d.to_string()), Some("transient".into()));
    }

    #[test]
    fn symbol_index_ignores_a_reused_id() {
        let first = JsSymbol::new(900, Some(JsString::from_str("first")));
        let second = JsSymbol::new(900, Some(JsString::from_str("second")));
        let first_key = first.to_property_key();
        let second_key = second.to_property_key();
        assert!(JsSymbol::from_property_key(&second_key).is_some());
        assert!(JsSymbol::from_property_key(&first_key).is_none());
    }

    #[test]
    fn symbol_index_sweeps_dead_entries() {
        let live = JsSymbol::new(1_000_000, None);
        live.to_property_key();
        for id in 0..(SymbolIndex::MIN_SWEEP as u64 * 4) {
            JsSymbol::new(2_000_000 + id, None).to_property_key();
        }
        let size = SYMBOL_INDEX.with(|index| index.borrow().by_id.len());
        assert!(size < SymbolIndex::MIN_SWEEP * 2, "{size} entries retained");
        assert!(JsSymbol::from_property_key(&live.to_property_key()).is_some());
    }

    #[test]
    fn symbol_clones_share_one_word_data_pointer() {
        let symbol = JsSymbol::new(7, Some(JsString::from_str("description")));
        let cloned = symbol.clone();

        assert_eq!(
            std::mem::size_of::<JsSymbol>(),
            std::mem::size_of::<Arc<JsSymbolData>>()
        );
        assert!(symbol.shares_storage_with(&cloned));
        assert_eq!(cloned.id(), 7);
        assert_eq!(
            cloned.description().map(JsString::to_rust_string),
            Some("description".to_string())
        );
    }

    #[test]
    fn registered_flag_marks_only_symbol_for_symbols() {
        assert!(JsSymbol::for_key(1, JsString::from_str("k")).is_registered());
        assert!(!JsSymbol::new(2, Some(JsString::from_str("k"))).is_registered());
        assert!(!JsSymbol::new_well_known(3, "iterator").is_registered());
        let registered = JsSymbol::for_key(4, JsString::from_str("k"));
        assert_eq!(
            registered.description().map(JsString::to_rust_string),
            Some("k".into())
        );
        assert!(registered.clone().is_registered());
    }

    #[test]
    fn registered_flag_survives_a_property_key_round_trip() {
        let registered = JsSymbol::for_key(5, JsString::from_str("k"));
        let back = JsSymbol::from_property_key(&registered.to_property_key()).expect("alive");
        assert!(back.is_registered());
    }
}
