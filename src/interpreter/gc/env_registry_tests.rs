use super::{EnvRegistry, NO_SLOT};
use crate::interpreter::types::{Binding, BindingKind, EnvRef, Environment};
use crate::types::JsValue;
use std::rc::Rc;

fn env_with_binding() -> EnvRef {
    let env = Environment::new(None);
    env.borrow_mut().bindings.insert(
        "x",
        Binding::new(JsValue::number(1.0), BindingKind::Var, true),
    );
    env
}

fn write(env: &EnvRef) {
    env.borrow_mut().bindings.insert(
        "x",
        Binding::new(JsValue::number(2.0), BindingKind::Var, true),
    );
}

fn registered_clean(registry: &EnvRegistry) -> EnvRef {
    let env = env_with_binding();
    registry.register(&env);
    env.borrow().mark_clean();
    env
}

#[test]
fn a_clean_environment_is_queued_by_its_first_write_only() {
    let registry = EnvRegistry::new();
    let env = registered_clean(&registry);
    assert!(registry.take_dirty().is_empty());

    write(&env);
    write(&env);
    let dirty = registry.take_dirty();
    assert_eq!(dirty.len(), 1);
    assert!(Rc::ptr_eq(&dirty[0], &env));
    assert!(registry.take_dirty().is_empty());
}

#[test]
fn rearming_queues_an_environment_for_the_next_collection() {
    let registry = EnvRegistry::new();
    let env = env_with_binding();
    registry.register(&env);
    env.borrow().bindings.rearm();
    assert!(!env.borrow().is_clean());
    assert_eq!(registry.take_dirty().len(), 1);
}

#[test]
fn registering_twice_keeps_one_slot() {
    let registry = EnvRegistry::new();
    let env = env_with_binding();
    assert_eq!(env.borrow().bindings.slot(), NO_SLOT);
    registry.register(&env);
    let slot = env.borrow().bindings.slot();
    registry.register(&env);
    assert_eq!(env.borrow().bindings.slot(), slot);
    assert_eq!(registry.len(), 1);
}

#[test]
fn a_dead_environment_gives_its_slot_to_the_next_one() {
    let registry = EnvRegistry::new();
    let first = registered_clean(&registry);
    let slot = first.borrow().bindings.slot();
    write(&first);
    assert_eq!(registry.len(), 1);

    drop(first);
    assert_eq!(registry.len(), 0);
    assert!(registry.take_dirty().is_empty());

    let second = registered_clean(&registry);
    assert_eq!(second.borrow().bindings.slot(), slot);
    assert_eq!(registry.len(), 1);
    assert!(registry.take_dirty().is_empty());
}

#[test]
fn a_stale_queue_entry_for_a_reused_slot_only_costs_a_scan() {
    let registry = EnvRegistry::new();
    let first = registered_clean(&registry);
    write(&first);
    drop(first);
    let second = registered_clean(&registry);
    write(&second);
    let dirty = registry.take_dirty();
    assert!(dirty.iter().all(|env| Rc::ptr_eq(env, &second)));
    assert!(!dirty.is_empty());
}

#[test]
fn a_major_collection_frees_the_slot_of_an_environment_whose_release_was_missed() {
    let registry = EnvRegistry::new();
    let env = registered_clean(&registry);
    let slot = env.borrow().bindings.slot();
    // Unregistering by hand stands for a release that could not borrow the slab.
    env.borrow().bindings.set_slot(NO_SLOT);
    drop(env);
    assert_eq!(registry.len(), 1);
    registry.begin_major();
    assert_eq!(registry.len(), 0);
    let next = registered_clean(&registry);
    assert_eq!(next.borrow().bindings.slot(), slot);
}

#[test]
fn one_interpreters_drain_leaves_the_other_interpreters_environments_queued() {
    let mine = EnvRegistry::new();
    let theirs = EnvRegistry::new();
    let env = registered_clean(&theirs);
    write(&env);

    assert!(mine.take_dirty().is_empty());
    assert_eq!(theirs.take_dirty().len(), 1);
    assert_eq!(mine.registered().len(), 0);
}

#[test]
fn a_major_collection_spends_the_queue_of_its_own_interpreter_only() {
    let mine = EnvRegistry::new();
    let theirs = EnvRegistry::new();
    let own = registered_clean(&mine);
    let other = registered_clean(&theirs);
    write(&own);
    write(&other);

    mine.begin_major();
    assert!(mine.take_dirty().is_empty());
    assert_eq!(theirs.take_dirty().len(), 1);
}
