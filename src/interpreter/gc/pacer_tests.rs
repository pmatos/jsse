use super::{CollectionKind, GcPacer};
use crate::interpreter::types::{
    GC_GROWTH_FACTOR, GC_INITIAL_THRESHOLD_BYTES, GC_NURSERY_THRESHOLD_BYTES, GC_OBJECT_OVERHEAD,
};

// GcPacer — the allocation-pressure heuristic that decides when to collect.
// Tested through its public interface; expected budgets are hand-computed
// literals (independent of the pacer's own arithmetic).

#[test]
fn safepoint_poll_flag_tracks_requests_and_stress_mode() {
    let mut pacer = GcPacer::new();
    pacer.set_stress_period(0);
    assert!(!pacer.poll);

    pacer.request();
    assert!(pacer.poll);
    assert_eq!(pacer.begin_collection(), Some(CollectionKind::Major));
    assert!(!pacer.poll);

    pacer.request_minor();
    assert!(pacer.poll);
    pacer.suppress_minor_temporarily();
    assert!(!pacer.poll);

    pacer.request_minor();
    assert_eq!(pacer.begin_collection(), Some(CollectionKind::Minor));
    assert!(!pacer.poll);

    pacer.set_stress_period(3);
    assert!(pacer.poll);
    pacer.request();
    assert_eq!(pacer.begin_collection(), Some(CollectionKind::Major));
    assert!(pacer.poll, "stress mode keeps every safepoint live");
    pacer.set_stress_period(0);
    assert!(!pacer.poll);
}

#[test]
fn stress_period_zero_never_collects() {
    let mut pacer = GcPacer::new();
    pacer.set_stress_period(0);
    for _ in 0..1000 {
        assert_eq!(pacer.begin_collection(), None);
    }
}

#[test]
fn stress_period_fires_every_nth_safepoint_alternating_major_and_minor() {
    let mut pacer = GcPacer::new();
    pacer.set_stress_period(3);
    let fired: Vec<_> = (1..=12).map(|_| pacer.begin_collection()).collect();
    assert_eq!(
        fired,
        vec![
            None,
            None,
            Some(CollectionKind::Major),
            None,
            None,
            Some(CollectionKind::Minor),
            None,
            None,
            Some(CollectionKind::Major),
            None,
            None,
            Some(CollectionKind::Minor),
        ]
    );
}

#[test]
fn stress_period_one_collects_at_every_safepoint() {
    let mut pacer = GcPacer::new();
    pacer.set_stress_period(1);
    for _ in 0..4 {
        assert!(pacer.begin_collection().is_some());
    }
}

#[test]
fn real_requests_take_priority_over_stress_and_do_not_advance_it() {
    let mut pacer = GcPacer::new();
    pacer.set_stress_period(2);
    assert_eq!(pacer.begin_collection(), None);
    pacer.request();
    assert_eq!(pacer.begin_collection(), Some(CollectionKind::Major));
    assert_eq!(
        pacer.begin_collection(),
        Some(CollectionKind::Major),
        "the pending request did not consume a stress tick"
    );
}

#[test]
fn fresh_pacer_requests_no_collection() {
    let mut pacer = GcPacer::new();
    assert!(!pacer.is_requested());
    assert_eq!(pacer.begin_collection(), None);
}

#[test]
fn nursery_threshold_requests_minor_collection() {
    let mut pacer = GcPacer::new();
    // Raise the major threshold so only nursery pressure can fire.
    pacer.end_major_collection(100_000);
    let nursery_objects = GC_NURSERY_THRESHOLD_BYTES / GC_OBJECT_OVERHEAD;
    for _ in 0..nursery_objects - 1 {
        pacer.charge_object(true);
    }
    assert!(!pacer.is_requested(), "not yet at the nursery threshold");
    pacer.charge_object(true);
    assert_eq!(
        pacer.begin_collection(),
        Some(CollectionKind::Minor),
        "reaching the nursery threshold requests a minor collection"
    );
}

#[test]
fn byte_threshold_requests_collection() {
    let mut pacer = GcPacer::new();
    pacer.track_external(1024);
    assert!(
        !pacer.is_requested(),
        "a small allocation stays under budget"
    );

    let mut pacer = GcPacer::new();
    pacer.track_external(GC_INITIAL_THRESHOLD_BYTES);
    assert!(
        pacer.is_requested(),
        "crossing the byte budget requests a collection"
    );
}

#[test]
fn request_forces_a_collection() {
    let mut pacer = GcPacer::new();
    pacer.request();
    assert!(pacer.is_requested());
    assert_eq!(pacer.begin_collection(), Some(CollectionKind::Major));
}

#[test]
fn release_external_saturates_at_zero() {
    let mut pacer = GcPacer::new();
    pacer.track_external(1000);
    assert_eq!(pacer.external_bytes(), 1000);
    pacer.release_external(4000);
    assert_eq!(pacer.external_bytes(), 0);
}

#[test]
fn minor_collection_resets_nursery_count_when_finished() {
    let mut pacer = GcPacer::new();
    pacer.end_major_collection(100_000);
    let nursery_objects = GC_NURSERY_THRESHOLD_BYTES / GC_OBJECT_OVERHEAD;
    for _ in 0..nursery_objects {
        pacer.charge_object(true);
    }
    assert!(pacer.is_requested());
    assert_eq!(pacer.begin_collection(), Some(CollectionKind::Minor));
    assert_eq!(pacer.alloc_count(), nursery_objects);
    pacer.end_minor_collection();
    assert_eq!(
        pacer.alloc_count(),
        0,
        "the nursery allocation counter resets"
    );
    assert!(!pacer.is_requested());
    assert!(
        pacer.begin_collection().is_none(),
        "the request is consumed, not repeated"
    );
}

#[test]
fn end_major_collection_grows_threshold_from_live_set() {
    // Empty live sets use the 8 MiB major floor.
    let mut pacer = GcPacer::new();
    pacer.end_major_collection(0);
    assert_eq!(pacer.threshold_bytes(), 8_388_608);

    // Growth factor 2 permits one additional live-set worth of debt:
    // 100,000 live objects × 512B overhead = 51,200,000.
    let mut pacer = GcPacer::new();
    pacer.end_major_collection(100_000);
    assert_eq!(pacer.threshold_bytes(), 51_200_000);

    // Tracked off-heap bytes feed the live-set estimate:
    // (0 objects + 10,000,000 external) × one live-set debt = 10,000,000.
    let mut pacer = GcPacer::new();
    pacer.track_external(10_000_000);
    pacer.end_major_collection(0);
    assert_eq!(pacer.threshold_bytes(), 10_000_000);
}

#[test]
fn end_major_collection_resets_byte_counter() {
    let mut pacer = GcPacer::new();
    pacer.track_external(GC_INITIAL_THRESHOLD_BYTES);
    assert_eq!(pacer.bytes_since_gc(), GC_INITIAL_THRESHOLD_BYTES);
    pacer.begin_collection();
    pacer.end_major_collection(0);
    assert_eq!(pacer.bytes_since_gc(), 0);
}

#[test]
fn major_request_takes_priority_over_minor_request() {
    let mut pacer = GcPacer::new();
    pacer.request_minor();
    pacer.request();
    assert_eq!(pacer.begin_collection(), Some(CollectionKind::Major));
    assert_eq!(pacer.begin_collection(), None);
}

#[test]
fn suppressing_minor_collection_preserves_debt_and_requests_major() {
    let mut pacer = GcPacer::new();
    pacer.end_major_collection(100_000);
    let nursery_objects = GC_NURSERY_THRESHOLD_BYTES / GC_OBJECT_OVERHEAD;
    for _ in 0..nursery_objects {
        pacer.charge_object(true);
    }
    assert_eq!(pacer.begin_collection(), Some(CollectionKind::Minor));
    let debt = pacer.bytes_since_gc();

    pacer.suppress_minor_temporarily();
    assert_eq!(pacer.alloc_count(), 0);
    for _ in 0..nursery_objects - 1 {
        pacer.charge_object(true);
    }
    assert_eq!(pacer.begin_collection(), None);
    pacer.charge_object(true);

    assert!(pacer.bytes_since_gc() > debt);
    assert_eq!(pacer.begin_collection(), Some(CollectionKind::Major));
}

#[test]
fn high_nursery_survival_keeps_minor_pacing_and_major_budget() {
    // #908: saturated minors used to force a major every nursery, tracing
    // the whole growing heap per 8,192 allocations. Majors now come only
    // from the live-set-proportional budget.
    let mut pacer = GcPacer::new();
    pacer.end_major_collection(100_000);
    let nursery_objects = GC_NURSERY_THRESHOLD_BYTES / GC_OBJECT_OVERHEAD;
    let mut majors = 0;
    let mut minors = 0;
    while majors == 0 {
        pacer.charge_object(false);
        match pacer.begin_collection() {
            Some(CollectionKind::Minor) => {
                minors += 1;
                pacer.end_minor_collection();
            }
            Some(CollectionKind::Major) => majors += 1,
            None => {}
        }
    }
    assert!(!pacer.minor_suppressed);
    let budget_objects = 100_000 * (GC_GROWTH_FACTOR - 1);
    assert!(
        minors >= budget_objects / nursery_objects - 1,
        "the major waits for the live-set budget, got {minors} minors first"
    );
}
