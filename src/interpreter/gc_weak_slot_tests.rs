//! WeakRef and FinalizationRegistry weak slots: cleared by major collections,
//! held by minor ones, kept alive for the running job (ADR-2026-10-28-0200).

use super::*;

fn run_source(interp: &mut Interpreter, source: &str) {
    let program = crate::parser::Parser::new(source)
        .expect("parser init")
        .parse_program()
        .expect("parse program");
    assert!(
        matches!(
            interp.run(&program),
            Completion::Normal(_) | Completion::Empty
        ),
        "script threw: {source}"
    );
}

fn major_gc(interp: &mut Interpreter) {
    interp.gc.request();
    interp.gc_safepoint();
}

fn minor_gc(interp: &mut Interpreter) {
    interp.gc.request_minor();
    interp.gc_safepoint();
}

#[test]
fn major_collection_clears_a_weakref_target_after_the_creating_job() {
    let mut interp = Interpreter::new();
    run_source(
        &mut interp,
        "var gone = new WeakRef({}); var held = {}; var kept = new WeakRef(held);",
    );
    major_gc(&mut interp);
    run_source(
        &mut interp,
        "if (gone.deref() !== undefined) throw 1; if (kept.deref() !== held) throw 2;",
    );
}

#[test]
fn weakref_target_stays_alive_for_the_running_job() {
    let mut interp = Interpreter::new();
    run_source(
        &mut interp,
        "var r = new WeakRef({}); $262.gc(); if (r.deref() === undefined) throw 1;
         $262.clearKeptObjects(); $262.gc(); if (r.deref() !== undefined) throw 2;",
    );
}

#[test]
fn minor_collection_never_clears_a_weakref_target() {
    let mut interp = Interpreter::new();
    run_source(&mut interp, "var r = new WeakRef({});");
    minor_gc(&mut interp);
    minor_gc(&mut interp);
    minor_gc(&mut interp);
    run_source(&mut interp, "if (r.deref() === undefined) throw 1;");
    major_gc(&mut interp);
    run_source(&mut interp, "if (r.deref() !== undefined) throw 2;");
}

#[test]
fn nested_job_end_does_not_release_the_outer_jobs_kept_objects() {
    let mut interp = Interpreter::new();
    run_source(
        &mut interp,
        "var r = new WeakRef({});
         Promise.resolve().then(function () {});
         $262.evalScript('1');
         $262.gc();
         if (r.deref() === undefined) throw 1;",
    );
    assert_eq!(interp.job_depth, 0);
}

#[test]
fn registry_runs_its_callback_once_with_the_held_value() {
    let mut interp = Interpreter::new();
    run_source(
        &mut interp,
        "var got = []; var fr = new FinalizationRegistry(function (h) { got.push(h); });
         (function () { fr.register({}, 7); })();",
    );
    major_gc(&mut interp);
    major_gc(&mut interp);
    interp.drain_microtasks();
    run_source(
        &mut interp,
        "if (got.length !== 1 || got[0] !== 7) throw 1;",
    );
}

#[test]
fn registry_with_a_live_target_runs_nothing() {
    let mut interp = Interpreter::new();
    run_source(
        &mut interp,
        "var got = []; var fr = new FinalizationRegistry(function (h) { got.push(h); });
         var target = {}; fr.register(target, 1);",
    );
    major_gc(&mut interp);
    interp.drain_microtasks();
    run_source(&mut interp, "if (got.length !== 0) throw 1;");
}

#[test]
fn dead_unregister_token_no_longer_matches_a_recycled_object() {
    let mut interp = Interpreter::new();
    run_source(
        &mut interp,
        "var fr = new FinalizationRegistry(function () {}); var live = {};
         (function () { fr.register(live, 1, {}); })();",
    );
    major_gc(&mut interp);
    run_source(
        &mut interp,
        "for (var i = 0; i < 64; i++) if (fr.unregister({})) throw 1;",
    );
}

#[test]
fn a_throwing_callback_leaves_the_remaining_cells_for_another_job() {
    let mut interp = Interpreter::new();
    run_source(
        &mut interp,
        "var got = []; var fr = new FinalizationRegistry(function (h) {
           got.push(h); if (h === 1) throw new Error('x'); });
         (function () { fr.register({}, 1); fr.register({}, 2); })();",
    );
    major_gc(&mut interp);
    interp.drain_microtasks();
    run_source(&mut interp, "if (got.join() !== '1,2') throw got.join();");
}

#[test]
fn targets_made_by_script_job_and_timer_are_collected_by_a_later_timer() {
    let mut interp = Interpreter::new();
    run_source(
        &mut interp,
        "var refs = {}, finalized = [], alive = [];
         function make(name) { refs[name] = new WeakRef({}); }
         var fr = new FinalizationRegistry(function (h) { finalized.push(h); });
         make('main');
         setTimeout(function () { make('timer'); }, 1);
         Promise.resolve().then(function () { make('microtask'); });
         (function () { fr.register({}, 'finalizer'); })();
         setTimeout(function () {
           $262.gc(); $262.gc();
           for (var k in refs) if (refs[k].deref() !== undefined) alive.push(k);
         }, 30);",
    );
    run_source(
        &mut interp,
        "if (alive.length !== 0) throw 'alive: ' + alive;
         if (finalized.join() !== 'finalizer') throw 'finalized: ' + finalized;",
    );
}
