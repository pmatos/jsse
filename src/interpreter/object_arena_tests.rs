use super::*;

#[test]
fn fresh_allocations_fill_current_chunk_before_growing() {
    let mut arena = ObjectArena::new();

    for expected in 0..CHUNK_SIZE as u64 {
        let (id, was_reuse) = arena.alloc(JsObjectData::new());
        assert_eq!(id, expected);
        assert!(!was_reuse);
        assert_eq!(arena.capacity(), CHUNK_SIZE as u64);
    }

    let (id, was_reuse) = arena.alloc(JsObjectData::new());
    assert_eq!(id, CHUNK_SIZE as u64);
    assert!(!was_reuse);
    assert_eq!(arena.capacity(), (CHUNK_SIZE * 2) as u64);
}

#[test]
fn freed_slots_are_reused_before_next_fresh_slot() {
    let mut arena = ObjectArena::new();
    let (first, _) = arena.alloc(JsObjectData::new());
    let (second, _) = arena.alloc(JsObjectData::new());

    arena.free(first);

    let (reused, was_reuse) = arena.alloc(JsObjectData::new());
    assert_eq!(reused, first);
    assert!(was_reuse);
    assert_eq!(arena.live_count(), 2);

    let (fresh, was_reuse) = arena.alloc(JsObjectData::new());
    assert_eq!(fresh, second + 1);
    assert!(!was_reuse);
}

#[test]
fn mutable_borrow_remembers_old_object_once() {
    let mut arena = ObjectArena::new();
    let (id, _) = arena.alloc(JsObjectData::new());
    let handle = arena.get_cell_expect(id);
    handle.promote();

    // Promotion alone remembers nothing; the collector decides.
    assert!(arena.take_remembered_ids().is_empty());
    handle.borrow_mut().extensible = false;
    handle.borrow_mut().extensible = true;
    assert_eq!(arena.take_remembered_ids(), vec![id]);
    assert!(arena.take_remembered_ids().is_empty());
}

#[test]
fn immutable_borrow_does_not_trigger_write_barrier() {
    let mut arena = ObjectArena::new();
    let (id, _) = arena.alloc(JsObjectData::new());
    let handle = arena.get_cell_expect(id);
    handle.promote();
    arena.take_remembered_ids();

    assert!(handle.borrow().extensible);
    assert!(arena.take_remembered_ids().is_empty());
}

#[test]
fn object_payloads_are_allocated_in_chunks() {
    let mut arena = ObjectArena::new();

    for _ in 0..CHUNK_SIZE {
        arena.alloc(JsObjectData::new());
    }
    assert_eq!(arena.storage_stats(), (1, CHUNK_SIZE, 0));

    arena.alloc(JsObjectData::new());
    assert_eq!(arena.storage_stats(), (2, CHUNK_SIZE + 1, 0));
    assert_eq!(
        std::mem::size_of::<ObjectHandle>(),
        std::mem::size_of::<usize>()
    );
}

#[test]
fn released_payload_blocks_are_reused() {
    let mut arena = ObjectArena::new();
    let (id, _) = arena.alloc(JsObjectData::new());
    assert_eq!(arena.storage_stats(), (1, 1, 0));

    arena.free(id);
    assert_eq!(arena.storage_stats(), (1, 1, 1));

    arena.alloc(JsObjectData::new());
    assert_eq!(arena.storage_stats(), (1, 1, 0));
}

#[test]
fn cloned_handle_delays_physical_reuse_after_logical_free() {
    let mut arena = ObjectArena::new();
    let (id, _) = arena.alloc(JsObjectData::new());
    let pinned = arena.get(id).unwrap();

    arena.free(id);
    assert_eq!(arena.storage_stats(), (1, 1, 0));

    let (reused_id, was_reuse) = arena.alloc(JsObjectData::new());
    assert_eq!(reused_id, id);
    assert!(was_reuse);
    assert_eq!(arena.storage_stats(), (1, 2, 0));

    drop(pinned);
    assert_eq!(arena.storage_stats(), (1, 2, 1));
}

#[test]
fn cloned_handle_keeps_storage_alive_after_arena_drop() {
    let (id, pinned, storage) = {
        let mut arena = ObjectArena::new();
        let (id, _) = arena.alloc(JsObjectData::new());
        (id, arena.get(id).unwrap(), Rc::clone(&arena.storage))
    };

    assert!(!storage.recycle_blocks.get());
    assert_eq!(pinned.id(), id);
    drop(pinned);
    assert!(storage.state.borrow().free_list.is_empty());
}

#[test]
fn prefetch_hints_follow_payload_kinds_and_leave_objects_intact() {
    use crate::interpreter::types::{
        ArrayData, Completion, IteratorKind, IteratorState, JsFunction, ObjectKind,
    };
    use crate::types::JsValue;

    let mut arena = ObjectArena::new();
    let mut function = JsObjectData::new();
    function.callable = Some(Box::new(JsFunction::native(
        "f".to_string(),
        0,
        |_, _, _| Completion::Normal(JsValue::UNDEFINED),
    )));
    let mut array = JsObjectData::new();
    array.kind = ObjectKind::Array(Box::new(ArrayData::default()));
    let mut iterator = JsObjectData::new();
    iterator.set_iterator_state(IteratorState::ArrayIterator {
        array_id: 0,
        index: 0,
        kind: IteratorKind::Value,
        done: false,
    });
    let live = [
        arena.alloc(JsObjectData::new()).0,
        arena.alloc(function).0,
        arena.alloc(array).0,
        arena.alloc(iterator).0,
    ];
    let (dead, _) = arena.alloc(JsObjectData::new());
    arena.free(dead);

    for id in live.into_iter().chain([dead, CHUNK_SIZE as u64 * 10]) {
        arena.prefetch_object(id);
        arena.prefetch_owned(id);
    }

    assert_eq!(arena.live_count(), live.len());
    assert!(arena.get_cell_expect(live[1]).borrow().callable.is_some());
    assert!(
        arena
            .get_cell_expect(live[2])
            .borrow()
            .array_elements()
            .is_some()
    );
    assert!(
        arena
            .get_cell_expect(live[3])
            .borrow()
            .iterator_state()
            .is_some()
    );
}
