use super::slot_index::{TAG_MAX, tag_of};
use super::*;

fn key(n: usize) -> JsPropertyKey {
    JsPropertyKey::from_str(&format!("k{n}"))
}

fn desc(n: usize) -> PropertyDescriptor {
    PropertyDescriptor::data_default(JsValue::number(n as f64))
}

fn number_of(map: &PropertyMap, n: usize) -> Option<f64> {
    map.get(&format!("k{n}"))
        .and_then(|r| r.value())
        .and_then(JsValue::as_number)
}

fn index_of(map: &PropertyMap) -> &SlotIndex {
    match &map.inner {
        PropertyMapInner::Spilled { index, .. } => index,
        PropertyMapInner::Inline(_) => panic!("map is still inline"),
    }
}

fn assert_consistent(map: &PropertyMap, model: &[(usize, usize)]) {
    assert_eq!(map.len(), model.len());
    let order: Vec<_> = map.iter().map(|(k, _)| k.as_bytes().to_vec()).collect();
    let expected: Vec<_> = model
        .iter()
        .map(|&(k, _)| format!("k{k}").into_bytes())
        .collect();
    assert_eq!(order, expected, "creation order");
    for &(k, v) in model {
        assert_eq!(number_of(map, k), Some(v as f64), "k{k}");
    }
}

#[test]
fn tags_cover_nine_to_tag_max_entries_and_hash_beyond() {
    let mut map = PropertyMap::new();
    for n in 0..=TAG_MAX + 8 {
        map.insert(key(n), desc(n));
        if n >= LINEAR_MAX {
            let tagged = matches!(index_of(&map), SlotIndex::Tags(_));
            assert_eq!(tagged, n < TAG_MAX, "{} entries", n + 1);
        }
        for m in 0..=n {
            assert_eq!(number_of(&map, m), Some(m as f64), "k{m} at {n}");
        }
        assert!(map.get("absent").is_none());
    }
}

#[test]
fn tag_index_tracks_removals_compaction_and_reinsertion() {
    let mut map = PropertyMap::new();
    for n in 0..60 {
        map.insert(key(n), desc(n));
    }
    for n in (50..60).chain([3, 7, 8, 9, 20]) {
        assert!(map.remove(&format!("k{n}")).is_some());
        assert!(map.get(&format!("k{n}")).is_none());
    }
    assert!(map.remove("k3").is_none());
    // Enough deletions to compact (entries.len() > 2 * live, at least 32 slots).
    for n in 10..48 {
        map.remove(&format!("k{n}"));
    }
    let live = [0, 1, 2, 4, 5, 6, 48, 49];
    for n in live {
        assert_eq!(number_of(&map, n), Some(n as f64));
    }
    assert_eq!(map.len(), live.len());
    map.insert(key(3), desc(300));
    assert_eq!(map.keys().last().unwrap().as_bytes(), b"k3");
    assert_eq!(number_of(&map, 3), Some(300.0));
}

#[test]
fn matches_a_model_under_random_inserts_and_removals() {
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move |bound: usize| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % bound as u64) as usize
    };
    for universe in [14, 40, 70, 150] {
        let mut map = PropertyMap::new();
        let mut model: Vec<(usize, usize)> = Vec::new();
        for step in 0..4000 {
            let k = next(universe);
            if next(3) == 0 {
                let removed = map.remove(&format!("k{k}")).is_some();
                let at = model.iter().position(|&(m, _)| m == k);
                assert_eq!(removed, at.is_some());
                if let Some(at) = at {
                    model.remove(at);
                }
            } else {
                map.insert(key(k), desc(step));
                match model.iter_mut().find(|(m, _)| *m == k) {
                    Some(slot) => slot.1 = step,
                    None => model.push((k, step)),
                }
            }
            if step % 97 == 0 {
                assert_consistent(&map, &model);
            }
        }
        assert_consistent(&map, &model);
        for k in 0..universe {
            let present = model.iter().any(|&(m, _)| m == k);
            assert_eq!(map.contains_key(&format!("k{k}")), present);
        }
    }
}

#[test]
fn keys_that_share_a_tag_stay_distinct() {
    let mut by_tag: HashMap<u8, Vec<usize>> = HashMap::new();
    for n in 0..4000 {
        by_tag
            .entry(tag_of(format!("k{n}").as_bytes()))
            .or_default()
            .push(n);
    }
    let group = by_tag
        .values()
        .find(|v| v.len() >= 10)
        .expect("ten keys share a tag");
    let mut map = PropertyMap::new();
    for &n in &group[..9] {
        map.insert(key(n), desc(n));
    }
    for &n in &group[..9] {
        assert_eq!(number_of(&map, n), Some(n as f64));
    }
    assert!(map.get(&format!("k{}", group[9])).is_none());
}
