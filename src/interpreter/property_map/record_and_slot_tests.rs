use super::*;
use crate::interpreter::types::JsObjectData;
use crate::types::JsValue;

fn key(s: impl AsRef<str>) -> JsPropertyKey {
    JsPropertyKey::from_str(s.as_ref())
}

fn desc(n: f64) -> PropertyDescriptor {
    PropertyDescriptor::data_default(JsValue::number(n))
}

fn num(d: &PropertyRecord) -> f64 {
    d.value().and_then(JsValue::as_number).unwrap_or(f64::NAN)
}

fn is_inline(map: &PropertyMap) -> bool {
    matches!(map.inner, PropertyMapInner::Inline(_))
}

#[test]
fn record_round_trips_partial_descriptors() {
    let _per_object = ShapesPaused::new();
    let cases = [
        PropertyDescriptor {
            value: Some(JsValue::number(1.0)),
            writable: Some(false),
            get: None,
            set: None,
            enumerable: None,
            configurable: Some(true),
        },
        PropertyDescriptor {
            value: None,
            writable: None,
            get: Some(JsValue::UNDEFINED),
            set: None,
            enumerable: Some(false),
            configurable: None,
        },
        PropertyDescriptor {
            value: None,
            writable: None,
            get: None,
            set: Some(JsValue::number(2.0)),
            enumerable: Some(true),
            configurable: Some(false),
        },
        PropertyDescriptor {
            value: None,
            writable: None,
            get: None,
            set: None,
            enumerable: None,
            configurable: None,
        },
    ];
    for d in cases {
        let record = PropertyRecord::from(d.clone());
        let back = record.to_descriptor();
        assert_eq!(back.value.is_some(), d.value.is_some());
        assert_eq!(back.get.is_some(), d.get.is_some());
        assert_eq!(back.set.is_some(), d.set.is_some());
        assert_eq!(back.writable, d.writable);
        assert_eq!(back.enumerable, d.enumerable);
        assert_eq!(back.configurable, d.configurable);
        assert_eq!(record.is_data_descriptor(), d.is_data_descriptor());
        assert_eq!(record.is_accessor_descriptor(), d.is_accessor_descriptor());
        let moved = record.into_descriptor();
        assert_eq!(moved.writable, d.writable);
        assert_eq!(moved.value.is_some(), d.value.is_some());
    }
}

#[test]
fn record_setters_touch_only_their_field() {
    let _per_object = ShapesPaused::new();
    let mut record = PropertyRecord::from(PropertyDescriptor::data(
        JsValue::number(1.0),
        true,
        false,
        true,
    ));
    record.set_value(Some(JsValue::number(9.0)));
    assert_eq!(num(&record), 9.0);
    assert_eq!(record.writable(), Some(true));
    assert_eq!(record.enumerable(), Some(false));
    assert_eq!(record.configurable(), Some(true));
    record.set_writable(Some(false));
    assert_eq!(record.writable(), Some(false));
    assert_eq!(num(&record), 9.0);
    record.set_writable(None);
    assert_eq!(record.writable(), None);
    record.set_value(None);
    assert!(record.value().is_none());
    assert_eq!(record.enumerable(), Some(false));
}

#[test]
fn data_value_and_in_place_writes_respect_attributes() {
    let _per_object = ShapesPaused::new();
    let mut m = PropertyMap::new();
    m.insert(key("w"), desc(1.0));
    m.insert(
        key("ro"),
        PropertyDescriptor {
            value: Some(JsValue::number(2.0)),
            writable: Some(false),
            enumerable: Some(true),
            configurable: Some(true),
            get: None,
            set: None,
        },
    );
    m.insert(
        key("acc"),
        PropertyDescriptor {
            value: None,
            writable: None,
            enumerable: Some(true),
            configurable: Some(true),
            get: Some(JsValue::UNDEFINED),
            set: None,
        },
    );
    assert!(matches!(m.get("w").unwrap().read(), PropertyRead::Value(_)));
    assert!(matches!(
        m.get("ro").unwrap().read(),
        PropertyRead::Value(_)
    ));
    assert!(matches!(
        m.get("acc").unwrap().read(),
        PropertyRead::Value(v) if v.is_undefined()
    ));

    assert!(
        m.get_mut("w")
            .unwrap()
            .overwrite_writable_data(&JsValue::number(5.0))
    );
    assert_eq!(num(m.get("w").unwrap()), 5.0);
    assert!(
        !m.get_mut("ro")
            .unwrap()
            .overwrite_writable_data(&JsValue::number(5.0))
    );
    assert_eq!(num(m.get("ro").unwrap()), 2.0);
    assert!(
        !m.get_mut("acc")
            .unwrap()
            .overwrite_writable_data(&JsValue::number(5.0))
    );

    assert_eq!(
        m.get_mut("w").unwrap().add_to_writable_number(1.0),
        Some((5.0, 6.0))
    );
    assert_eq!(num(m.get("w").unwrap()), 6.0);
    assert_eq!(m.get_mut("ro").unwrap().add_to_writable_number(1.0), None);
    assert_eq!(num(m.get("ro").unwrap()), 2.0);
    m.get_mut("w")
        .unwrap()
        .set_value(Some(JsValue::string(crate::types::JsString::from_str("s"))));
    assert_eq!(m.get_mut("w").unwrap().add_to_writable_number(1.0), None);
}

#[test]
fn key_lookup_distinguishes_equal_length_keys_and_non_interned_copies() {
    let _per_object = ShapesPaused::new();
    let mut m = PropertyMap::new();
    m.insert(key("ab"), desc(1.0));
    m.insert(key("ac"), desc(2.0));
    m.insert(key("bb"), desc(3.0));
    assert_eq!(num(m.get("ab").unwrap()), 1.0);
    assert_eq!(num(m.get("ac").unwrap()), 2.0);
    assert_eq!(num(m.get("bb").unwrap()), 3.0);
    assert!(m.get("ba").is_none());
    assert!(m.get("a").is_none());
    let fresh_copy = JsPropertyKey::from_str("ac");
    assert_eq!(num(m.get(&fresh_copy).unwrap()), 2.0);
    assert!(m.remove(&fresh_copy).is_some());
    assert!(m.get("ac").is_none());
}

#[test]
fn hinted_lookup_agrees_with_plain_lookup_across_spill_and_removal() {
    let _per_object = ShapesPaused::new();
    let hint = Cell::new(0u32);
    let mut m = PropertyMap::new();
    let probe = key("target");
    assert!(m.get_hinted(&probe, Some(&hint)).is_none(), "empty map");
    for round in 0..2 {
        for i in 0..20 {
            m.insert(key(format!("k{i}")), desc(i as f64));
            assert!(
                m.get_hinted(&probe, Some(&hint)).is_none(),
                "round {round}: absent"
            );
        }
        m.insert(probe.clone(), desc(99.0));
        assert_eq!(num(m.get_hinted(&probe, Some(&hint)).unwrap()), 99.0);
        for i in 0..20 {
            let k = key(format!("k{i}"));
            let got = m.get_hinted(&k, Some(&Cell::new(7))).map(num);
            assert_eq!(got, Some(i as f64));
            assert_eq!(m.get_hinted(&k, None).map(num), Some(i as f64));
        }
        assert!(m.remove("target").is_some());
        assert!(m.get_hinted(&probe, Some(&hint)).is_none(), "removed");
        for i in 0..20 {
            m.remove(&format!("k{i}"));
        }
    }
}

#[test]
fn js_object_data_size_cap() {
    let _per_object = ShapesPaused::new();
    // #909: box cold state rather than raising the cap.
    let size = std::mem::size_of::<JsObjectData>();
    assert!(size <= 272, "JsObjectData grew to {size} bytes (cap 272)");
}

#[test]
fn slot_round_trips_in_both_modes() {
    let _per_object = ShapesPaused::new();
    let mut m = PropertyMap::new();
    for i in 0..(LINEAR_MAX + 3) {
        m.insert(key(format!("k{i}")), desc(i as f64));
        let name = format!("k{i}");
        let slot = m.slot_of(&name).expect("slot");
        let (k, d) = m.entry_at(slot).expect("entry");
        assert_eq!(k.as_bytes(), name.as_bytes());
        assert_eq!(num(d), i as f64);
    }
    assert!(!is_inline(&m));
    assert!(m.slot_of("missing").is_none());
    assert!(m.entry_at(10_000).is_none());
    let slot = m.slot_of("k1").unwrap();
    m.entry_at_mut(slot)
        .unwrap()
        .set_value(Some(JsValue::number(99.0)));
    assert_eq!(num(m.get("k1").unwrap()), 99.0);
}

#[test]
fn remembered_slot_is_detectable_after_remove_and_reinsert() {
    let _per_object = ShapesPaused::new();
    let mut m = PropertyMap::new();
    let total = LINEAR_MAX + 4;
    for i in 0..total {
        m.insert(key(format!("k{i}")), desc(i as f64));
    }
    let stale = m.slot_of("k2").unwrap();
    m.remove("k2").expect("removed");
    // The slot is a tombstone, so a verifying caller sees no entry.
    assert!(m.entry_at(stale).is_none());
    assert!(m.get("k2").is_none());
    assert!(m.slot_of("k2").is_none());
    // Every survivor is still reachable and its slot verifies.
    for i in (0..total).filter(|&i| i != 2) {
        let name = format!("k{i}");
        let slot = m.slot_of(&name).expect("survivor");
        assert_eq!(m.entry_at(slot).unwrap().0.as_bytes(), name.as_bytes());
        assert_eq!(num(m.get(&name).unwrap()), i as f64);
    }
    m.insert(key("k2"), desc(2.0));
    assert_eq!(num(m.get("k2").unwrap()), 2.0);
    assert_eq!(m.iter().count(), total);
    assert_eq!(m.len(), total);
}

fn order(m: &PropertyMap) -> Vec<String> {
    m.keys().map(|k| k.to_string()).collect()
}

#[test]
fn iteration_is_creation_order_in_both_modes() {
    let _per_object = ShapesPaused::new();
    for total in [3, LINEAR_MAX + 5] {
        let mut m = PropertyMap::new();
        for i in 0..total {
            m.insert(key(format!("k{i}")), desc(i as f64));
        }
        // Replacing keeps the position; removing and re-adding moves to the end.
        m.insert(key("k1"), desc(100.0));
        m.remove("k0");
        m.insert(key("k0"), desc(0.0));
        let mut want: Vec<String> = (1..total).map(|i| format!("k{i}")).collect();
        want.push("k0".into());
        assert_eq!(order(&m), want, "total {total}");
        let values: Vec<f64> = m.values().map(num).collect();
        assert_eq!(values.len(), total);
        assert_eq!(values[0], 100.0);
    }
}

#[test]
fn spilled_removal_keeps_order_through_tombstones_and_compaction() {
    let _per_object = ShapesPaused::new();
    let mut m = PropertyMap::new();
    let total = COMPACT_MIN_ENTRIES * 4;
    for i in 0..total {
        m.insert(key(format!("k{i}")), desc(i as f64));
    }
    assert!(!is_inline(&m));
    let mut want: Vec<usize> = (0..total).collect();
    // Delete every entry but each tenth, in a scattered order, crossing
    // the compaction threshold several times.
    for step in [3usize, 7, 1] {
        for i in (0..total).filter(|i| i % step == 0 && i % 10 != 0) {
            if m.remove(&format!("k{i}")).is_some() {
                want.retain(|w| *w != i);
            }
        }
    }
    assert_eq!(m.len(), want.len());
    assert_eq!(
        order(&m),
        want.iter().map(|i| format!("k{i}")).collect::<Vec<_>>()
    );
    for i in &want {
        let name = format!("k{i}");
        let slot = m.slot_of(&name).expect("survivor indexed");
        assert_eq!(m.entry_at(slot).unwrap().0.as_bytes(), name.as_bytes());
        assert_eq!(num(m.get(&name).unwrap()), *i as f64);
    }
    m.insert(key("late"), desc(-1.0));
    assert_eq!(order(&m).last().map(String::as_str), Some("late"));
}

#[test]
fn removing_every_spilled_entry_leaves_an_empty_reusable_map() {
    let _per_object = ShapesPaused::new();
    let mut m = PropertyMap::new();
    for i in 0..(LINEAR_MAX + 6) {
        m.insert(key(format!("k{i}")), desc(i as f64));
    }
    for i in 0..(LINEAR_MAX + 6) {
        assert!(m.remove(&format!("k{i}")).is_some());
    }
    assert_eq!(m.len(), 0);
    assert_eq!(m.iter().count(), 0);
    m.insert(key("again"), desc(1.0));
    assert_eq!(order(&m), vec!["again".to_string()]);
    assert_eq!(num(m.get("again").unwrap()), 1.0);
}

#[test]
fn removing_the_last_entry_leaves_index_consistent() {
    let _per_object = ShapesPaused::new();
    let mut m = PropertyMap::new();
    let total = LINEAR_MAX + 2;
    for i in 0..total {
        m.insert(key(format!("k{i}")), desc(i as f64));
    }
    let last = format!("k{}", total - 1);
    assert!(m.remove(&last).is_some());
    assert!(m.get(&last).is_none());
    assert_eq!(m.iter().count(), total - 1);
    for i in 0..total - 1 {
        assert!(m.get(&format!("k{i}")).is_some());
    }
}

#[test]
fn push_fresh_preserves_entries_across_the_spill_boundary() {
    let _per_object = ShapesPaused::new();
    let mut m = PropertyMap::new();
    for i in 0..LINEAR_MAX {
        m.push_fresh(key(format!("k{i}")), desc(i as f64));
    }
    assert!(is_inline(&m));
    m.push_fresh(key(format!("k{LINEAR_MAX}")), desc(LINEAR_MAX as f64));
    assert!(!is_inline(&m));
    for i in 0..=LINEAR_MAX {
        assert_eq!(num(m.get(&format!("k{i}")).expect("present")), i as f64);
    }
}

#[test]
fn heap_backed_linear_entries_beyond_inline_cap_stay_linear() {
    let _per_object = ShapesPaused::new();
    let mut m = PropertyMap::new();
    for i in 0..=INLINE_CAP {
        m.insert(key(format!("k{i}")), desc(i as f64));
    }
    assert!(is_inline(&m), "{} entries must not hash", INLINE_CAP + 1);
}
