use super::*;
use crate::interpreter::types::JsObjectData;
use crate::types::JsValue;

fn key(s: impl AsRef<str>) -> JsPropertyKey {
    JsPropertyKey::from_str(s.as_ref())
}

fn desc(n: f64) -> PropertyDescriptor {
    PropertyDescriptor::data_default(JsValue::number(n))
}

fn num(d: RecordRef<'_>) -> f64 {
    d.value().and_then(JsValue::as_number).unwrap_or(f64::NAN)
}

fn is_inline(map: &PropertyMap) -> bool {
    matches!(map.inner, PropertyMapInner::Inline(_))
}

#[test]
fn inline_insert_and_get() {
    let _per_object = ShapesPaused::new();
    let mut m = PropertyMap::new();
    for i in 0..LINEAR_MAX {
        assert!(m.insert(key(format!("k{i}")), desc(i as f64)).is_none());
    }
    assert!(is_inline(&m));
    for i in 0..LINEAR_MAX {
        let d = m.get(&format!("k{i}")).expect("present");
        assert_eq!(num(d), i as f64);
    }
    assert!(m.get("missing").is_none());
}

#[test]
fn one_past_linear_max_triggers_spill() {
    let _per_object = ShapesPaused::new();
    let mut m = PropertyMap::new();
    for i in 0..LINEAR_MAX {
        m.insert(key(format!("k{i}")), desc(i as f64));
    }
    assert!(is_inline(&m));
    m.insert(key(format!("k{LINEAR_MAX}")), desc(LINEAR_MAX as f64));
    assert!(!is_inline(&m));
    for i in 0..=LINEAR_MAX {
        assert!(
            m.get(&format!("k{i}")).is_some(),
            "k{i} missing after spill"
        );
    }
}

#[test]
fn duplicate_insert_returns_previous_and_no_spill() {
    let _per_object = ShapesPaused::new();
    let mut m = PropertyMap::new();
    for i in 0..LINEAR_MAX {
        m.insert(key(format!("k{i}")), desc(i as f64));
    }
    let prev = m.insert(key("k0"), desc(99.0)).expect("had previous");
    assert_eq!(prev.value.as_ref().and_then(JsValue::as_number), Some(0.0));
    assert!(is_inline(&m));
    assert_eq!(num(m.get("k0").expect("present")), 99.0);
}

#[test]
fn reserve_spills_only_when_the_total_overflows_linear_max() {
    let _per_object = ShapesPaused::new();
    let mut m = PropertyMap::new();
    m.insert(key("a"), desc(1.0));
    m.reserve(LINEAR_MAX - 1);
    assert!(is_inline(&m), "fits inline: no spill");
    m.reserve(LINEAR_MAX);
    assert!(!is_inline(&m), "would overflow: spilled up front");
    assert_eq!(num(m.get("a").expect("kept across the spill")), 1.0);
    for i in 0..32 {
        m.insert(key(format!("k{i}")), desc(i as f64));
    }
    for i in 0..32 {
        assert_eq!(num(m.get(&format!("k{i}")).expect("present")), i as f64);
    }
}

#[test]
fn remove_after_spill_keeps_spilled() {
    let _per_object = ShapesPaused::new();
    let mut m = PropertyMap::new();
    for i in 0..=LINEAR_MAX {
        m.insert(key(format!("k{i}")), desc(i as f64));
    }
    assert!(!is_inline(&m));
    for i in 0..=LINEAR_MAX {
        assert!(m.remove(&format!("k{i}")).is_some());
    }
    assert!(!is_inline(&m), "spill must not collapse back to inline");
    assert!(m.remove("k0").is_none());
}

#[test]
fn iter_visits_all_entries_in_both_modes() {
    let _per_object = ShapesPaused::new();
    let mut m = PropertyMap::new();
    for i in 0..3 {
        m.insert(key(format!("k{i}")), desc(i as f64));
    }
    let mut seen: Vec<(String, f64)> = m
        .iter()
        .map(|(k, d)| {
            let n = d.value().and_then(JsValue::as_number).unwrap_or(-1.0);
            (k.to_string(), n)
        })
        .collect();
    seen.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        seen,
        vec![("k0".into(), 0.0), ("k1".into(), 1.0), ("k2".into(), 2.0)]
    );

    for i in 3..=LINEAR_MAX {
        m.insert(key(format!("k{i}")), desc(i as f64));
    }
    assert!(!is_inline(&m));
    let mut seen: Vec<(String, f64)> = m
        .iter()
        .map(|(k, d)| {
            let n = d.value().and_then(JsValue::as_number).unwrap_or(-1.0);
            (k.to_string(), n)
        })
        .collect();
    seen.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(seen.len(), LINEAR_MAX + 1);
    for (i, (k, n)) in seen.iter().enumerate() {
        assert_eq!(*k, format!("k{i}"));
        assert_eq!(*n, i as f64);
    }
}

#[test]
fn empty_iteration_is_empty() {
    let _per_object = ShapesPaused::new();
    let m = PropertyMap::new();
    assert_eq!(m.iter().count(), 0);
    assert_eq!(m.keys().count(), 0);
    assert_eq!(m.values().count(), 0);
}

#[test]
fn get_mut_in_both_modes() {
    let _per_object = ShapesPaused::new();
    let mut m = PropertyMap::new();
    m.insert(key("a"), desc(1.0));
    if let Some(d) = m.get_mut("a") {
        d.set_value(JsValue::number(7.0));
    }
    assert_eq!(num(m.get("a").expect("present")), 7.0);

    for i in 0..LINEAR_MAX {
        m.insert(key(format!("k{i}")), desc(i as f64));
    }
    assert!(!is_inline(&m));
    if let Some(d) = m.get_mut("a") {
        d.set_value(JsValue::number(42.0));
    }
    assert_eq!(num(m.get("a").expect("present")), 42.0);
}

#[test]
fn keys_keep_symbol_and_symbol_like_string_distinct_in_both_modes() {
    let _per_object = ShapesPaused::new();
    let mut m = PropertyMap::new();
    let symbol = JsPropertyKey::well_known_symbol("iterator");
    let text = key("Symbol(Symbol.iterator)");
    m.insert(symbol.clone(), desc(1.0));
    m.insert(text.clone(), desc(2.0));
    let keys: Vec<&JsPropertyKey> = m.keys().collect();
    assert!(keys.contains(&&symbol));
    assert!(keys.contains(&&text));
    assert_eq!(num(m.get(&symbol).expect("Symbol key present")), 1.0);
    assert_eq!(num(m.get(&text).expect("String key present")), 2.0);
    for i in 0..LINEAR_MAX {
        m.insert(key(format!("k{i}")), desc(i as f64));
    }
    assert!(!is_inline(&m));
    let keys: Vec<&JsPropertyKey> = m.keys().collect();
    assert!(keys.contains(&&symbol), "Symbol key lost on spill");
    assert!(keys.contains(&&text), "String key lost on spill");
}

#[test]
fn index_key_count_tracks_digit_leading_keys() {
    let _per_object = ShapesPaused::new();
    let mut m = PropertyMap::new();
    assert!(!m.may_hold_index_key());
    m.insert(key("length"), desc(0.0));
    m.insert(key(""), desc(0.0));
    m.insert(key("a1"), desc(0.0));
    assert!(
        !m.may_hold_index_key(),
        "names that do not start with a digit"
    );

    m.insert(key("7"), desc(1.0));
    assert!(m.may_hold_index_key());
    m.insert(key("7"), desc(2.0));
    m.remove("7");
    assert!(
        !m.may_hold_index_key(),
        "replacing a key must not count twice"
    );
    assert!(m.remove("7").is_none());
    assert!(
        !m.may_hold_index_key(),
        "removing an absent key changes nothing"
    );

    let mut fresh = PropertyMap::new();
    fresh.push_fresh(key("0"), desc(1.0));
    assert!(fresh.may_hold_index_key(), "push_fresh records it too");

    let mut spilled = PropertyMap::new();
    for i in 0..=LINEAR_MAX {
        spilled.insert(key(format!("k{i}")), desc(i as f64));
    }
    assert!(
        !spilled.may_hold_index_key(),
        "spilling alone does not set it"
    );
    spilled.insert(key("4294967294"), desc(1.0));
    assert!(spilled.may_hold_index_key(), "recorded in spilled mode");
    spilled.remove("4294967294");
    assert!(!spilled.may_hold_index_key(), "cleared in spilled mode");
}
