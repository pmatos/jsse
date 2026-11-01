use super::*;

fn key(name: &str) -> JsPropertyKey {
    JsPropertyKey::from_str(name)
}

fn desc(n: usize) -> PropertyDescriptor {
    PropertyDescriptor::data_default(JsValue::number(n as f64))
}

fn number(map: &PropertyMap, name: &str) -> Option<f64> {
    map.get(name)
        .and_then(|r| r.value())
        .and_then(JsValue::as_number)
}

fn shape_of(map: &PropertyMap) -> Option<&Rc<Shape>> {
    match &map.inner {
        PropertyMapInner::Shaped { shape, .. } => Some(shape),
        _ => None,
    }
}

fn built(names: &[&str]) -> PropertyMap {
    let mut map = PropertyMap::new();
    for (n, name) in names.iter().enumerate() {
        map.insert(key(name), desc(n));
    }
    map
}

fn order(map: &PropertyMap) -> Vec<String> {
    map.keys().map(|k| k.to_string()).collect()
}

#[test]
fn maps_built_the_same_way_share_one_shape() {
    let names = [
        "a1", "b1", "c1", "d1", "e1", "f1", "g1", "h1", "i1", "j1", "k1",
    ];
    let (x, y) = (built(&names), built(&names));
    assert!(Rc::ptr_eq(shape_of(&x).unwrap(), shape_of(&y).unwrap()));
    assert_eq!(order(&x), names);
    for (n, name) in names.iter().enumerate() {
        assert_eq!(number(&x, name), Some(n as f64));
    }
}

#[test]
fn a_different_key_order_is_a_different_shape_with_its_own_order() {
    let (x, y) = (built(&["a2", "b2", "c2"]), built(&["a2", "c2", "b2"]));
    assert!(!Rc::ptr_eq(shape_of(&x).unwrap(), shape_of(&y).unwrap()));
    assert_eq!(order(&x), ["a2", "b2", "c2"]);
    assert_eq!(order(&y), ["a2", "c2", "b2"]);
    assert_eq!(number(&y, "b2"), Some(2.0));
}

#[test]
fn replacing_a_value_keeps_the_shape_and_an_attribute_change_leaves_it() {
    let mut map = built(&["a3", "b3", "c3", "d3", "e3"]);
    let before = shape_of(&map).unwrap().clone();
    let old = map.insert(key("c3"), desc(9)).unwrap();
    assert_eq!(old.value.and_then(|v| v.as_number()), Some(2.0));
    assert!(Rc::ptr_eq(&before, shape_of(&map).unwrap()));
    assert_eq!(number(&map, "c3"), Some(9.0));
    map.insert(
        key("c3"),
        PropertyDescriptor::data(JsValue::number(5.0), false, false, true),
    );
    assert!(shape_of(&map).is_none(), "different attributes");
    map.get_stored_mut("a3").unwrap().set_writable(Some(false));
    assert_eq!(number(&map, "c3"), Some(5.0));
    assert_eq!(map.get("c3").unwrap().enumerable(), Some(false));
    assert_eq!(map.get("a3").unwrap().writable(), Some(false));
    assert_eq!(order(&map), ["a3", "b3", "c3", "d3", "e3"]);
}

#[test]
fn accessors_and_partial_descriptors_are_not_shaped() {
    let mut map = built(&["a13", "b13"]);
    assert!(shape_of(&map).is_some());
    map.insert(
        key("c13"),
        PropertyDescriptor::accessor(Some(JsValue::number(1.0)), None, true, true),
    );
    assert!(shape_of(&map).is_none());
    assert_eq!(order(&map), ["a13", "b13", "c13"]);
    assert!(map.get("c13").unwrap().getter().is_some());
    let mut partial = PropertyMap::new();
    partial.insert(
        key("p13"),
        PropertyDescriptor {
            value: Some(JsValue::number(1.0)),
            writable: None,
            get: None,
            set: None,
            enumerable: Some(true),
            configurable: Some(true),
        },
    );
    assert!(shape_of(&partial).is_none());
    assert_eq!(partial.get("p13").unwrap().writable(), None);
}

#[test]
fn every_attribute_combination_round_trips_through_a_shape() {
    for bits in 0..8u8 {
        let (w, e, c) = (bits & 1 != 0, bits & 2 != 0, bits & 4 != 0);
        let mut map = PropertyMap::new();
        map.insert(
            key("x14"),
            PropertyDescriptor::data(JsValue::number(1.0), w, e, c),
        );
        map.insert(
            key("y14"),
            PropertyDescriptor::data(JsValue::number(2.0), !w, !e, !c),
        );
        assert!(shape_of(&map).is_some());
        let x = map.get("x14").unwrap();
        assert_eq!(
            (x.writable(), x.enumerable(), x.configurable()),
            (Some(w), Some(e), Some(c))
        );
        let y = map.get("y14").unwrap().to_descriptor();
        assert_eq!(
            (y.writable, y.enumerable, y.configurable),
            (Some(!w), Some(!e), Some(!c))
        );
        assert_eq!(map.remove("y14").unwrap().configurable, Some(!c));
    }
}

#[test]
fn removing_the_last_key_returns_to_the_parent_shape() {
    let mut map = built(&["a4", "b4", "c4", "d4", "e4"]);
    let four = built(&["a4", "b4", "c4", "d4"]);
    assert!(map.remove("e4").is_some());
    assert!(Rc::ptr_eq(
        shape_of(&map).unwrap(),
        shape_of(&four).unwrap()
    ));
    assert_eq!(order(&map), ["a4", "b4", "c4", "d4"]);
    assert!(map.get("e4").is_none());
    map.insert(key("e4"), desc(7));
    assert_eq!(number(&map, "e4"), Some(7.0));
    assert!(map.remove("nope").is_none());
}

#[test]
fn removing_any_other_key_leaves_shapes_and_keeps_order() {
    for removed in 0..12 {
        let names: Vec<String> = (0..12).map(|n| format!("r5_{n}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut map = built(&refs);
        let expect: Vec<String> = names
            .iter()
            .enumerate()
            .filter(|&(n, _)| n != removed)
            .map(|(_, s)| s.clone())
            .collect();
        assert!(map.remove(&names[removed]).is_some());
        assert_eq!(order(&map), expect, "removed {removed}");
        for (n, name) in names.iter().enumerate() {
            let want = (n != removed).then_some(n as f64);
            assert_eq!(number(&map, name), want, "{name} after removing {removed}");
        }
        map.insert(key("late"), desc(99));
        assert_eq!(order(&map).last().map(String::as_str), Some("late"));
    }
}

#[test]
fn a_map_leaves_shapes_past_the_key_limit() {
    let mut map = PropertyMap::new();
    for n in 0..shape::SHAPED_MAX + 20 {
        map.insert(key(&format!("w6_{n}")), desc(n));
        let shaped = shape_of(&map).is_some();
        assert_eq!(shaped, n < shape::SHAPED_MAX, "{} keys", n + 1);
    }
    assert_eq!(map.len(), shape::SHAPED_MAX + 20);
    for n in 0..shape::SHAPED_MAX + 20 {
        assert_eq!(number(&map, &format!("w6_{n}")), Some(n as f64));
    }
    assert_eq!(map.keys().next().unwrap().to_string(), "w6_0");
}

#[test]
fn reserve_gives_an_empty_map_room_on_the_root_shape() {
    let mut map = PropertyMap::new();
    map.reserve(12);
    let PropertyMapInner::Shaped { values, .. } = &map.inner else {
        panic!("not shaped");
    };
    let capacity = values.capacity();
    assert!(capacity >= 12);
    for n in 0..12 {
        map.insert(key(&format!("v7_{n}")), desc(n));
    }
    let PropertyMapInner::Shaped { values, .. } = &map.inner else {
        panic!("not shaped");
    };
    assert_eq!(values.capacity(), capacity, "no regrowth");
    let mut big = PropertyMap::new();
    big.reserve(shape::SHAPED_MAX + 1);
    assert!(shape_of(&big).is_none(), "too many keys for a shape");
}

#[test]
fn a_paused_thread_builds_per_object_maps() {
    let _per_object = ShapesPaused::new();
    let mut map = built(&["a8", "b8", "c8"]);
    assert!(shape_of(&map).is_none());
    map.reserve(20);
    assert!(shape_of(&map).is_none());
    assert_eq!(order(&map), ["a8", "b8", "c8"]);
}

#[test]
fn pausing_nests_and_restores() {
    let outer = ShapesPaused::new();
    let inner = ShapesPaused::new();
    drop(inner);
    assert!(shape_of(&built(&["a9"])).is_none());
    drop(outer);
    assert!(shape_of(&built(&["a9"])).is_some());
}

#[test]
fn a_full_child_table_sends_new_keys_to_per_object_maps() {
    let keep: Vec<PropertyMap> = (0..520).map(|n| built(&[&format!("fan10_{n}")])).collect();
    let shaped = keep.iter().filter(|m| shape_of(m).is_some()).count();
    assert!((505..=512).contains(&shaped), "{shaped} shaped");
    for (n, map) in keep.iter().enumerate() {
        assert_eq!(number(map, &format!("fan10_{n}")), Some(0.0));
    }
    let late = built(&["fan10_0", "second"]);
    assert!(shape_of(&late).is_some(), "known branches keep growing");
}

#[test]
fn a_dropped_shape_is_rebuilt_when_the_sequence_comes_back() {
    let first = built(&["a11", "b11", "c11", "d11"]);
    drop(first);
    let again = built(&["a11", "b11", "c11", "d11"]);
    assert_eq!(order(&again), ["a11", "b11", "c11", "d11"]);
    assert_eq!(number(&again, "d11"), Some(3.0));
}

#[test]
fn random_operations_agree_with_a_model() {
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move |bound: usize| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state >> 33) as usize % bound
    };
    for round in 0..300 {
        let mut map = PropertyMap::new();
        let mut model: Vec<(String, f64)> = Vec::new();
        for step in 0..60 {
            let name = format!("m12_{}", next(18));
            match next(10) {
                0..=5 => {
                    let value = (round * 100 + step) as f64;
                    map.insert(
                        key(&name),
                        PropertyDescriptor::data_default(JsValue::number(value)),
                    );
                    match model.iter_mut().find(|(k, _)| *k == name) {
                        Some(slot) => slot.1 = value,
                        None => model.push((name, value)),
                    }
                }
                6..=8 => {
                    let had = model.iter().position(|(k, _)| *k == name);
                    assert_eq!(map.remove(&name).is_some(), had.is_some());
                    if let Some(at) = had {
                        model.remove(at);
                    }
                }
                _ => {
                    if next(2) == 0 {
                        map.reserve(next(6));
                    }
                }
            }
            assert_eq!(map.len(), model.len());
            let seen: Vec<(String, f64)> = map
                .iter()
                .map(|(k, r)| {
                    (
                        k.to_string(),
                        r.value().and_then(JsValue::as_number).unwrap(),
                    )
                })
                .collect();
            assert_eq!(seen, model, "round {round} step {step}");
            for (k, v) in &model {
                assert_eq!(number(&map, k), Some(*v));
                let slot = map.slot_of(k).unwrap();
                assert_eq!(map.entry_at(slot).unwrap().0.to_string(), *k);
            }
        }
    }
}

#[test]
fn removing_from_an_empty_shaped_map_finds_nothing() {
    let mut map = PropertyMap::new();
    map.reserve(4);
    assert!(shape_of(&map).is_some());
    assert!(map.remove("absent").is_none());
    assert_eq!(map.len(), 0);
    map.insert(key("only15"), desc(1));
    assert!(map.remove("only15").is_some());
    assert_eq!(map.len(), 0);
    assert!(map.remove("only15").is_none());
}
