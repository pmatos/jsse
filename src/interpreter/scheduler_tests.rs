use super::*;
use crate::types::JsValue;

#[test]
fn microtask_queue_round_trip() {
    let mut sched = JobScheduler::default();

    assert!(
        sched.microtask_queue_is_empty(),
        "fresh scheduler must be idle"
    );

    let job: MicrotaskJob = Box::new(|_interp| Completion::Normal(JsValue::UNDEFINED));
    sched.enqueue_microtask((Vec::new(), job));

    assert!(
        !sched.microtask_queue_is_empty(),
        "queue must be non-empty after enqueue"
    );

    let popped = sched.pop_microtask();
    assert!(popped.is_some(), "pop must return the queued job");

    assert!(
        sched.microtask_queue_is_empty(),
        "queue must be empty after the only job is popped"
    );
    assert!(
        sched.pop_microtask().is_none(),
        "pop on empty queue returns None"
    );
}

#[test]
fn microtask_queue_drains_in_fifo_order() {
    // Tag each job via its roots — the roots vector holds a single Number.
    fn job() -> MicrotaskJob {
        Box::new(|_interp| Completion::Normal(JsValue::UNDEFINED))
    }
    fn tag(n: f64) -> Vec<JsValue> {
        vec![JsValue::number(n)]
    }

    let mut sched = JobScheduler::default();
    sched.enqueue_microtask((tag(1.0), job()));
    sched.enqueue_microtask((tag(2.0), job()));
    sched.enqueue_microtask((tag(3.0), job()));

    let popped_tags: Vec<f64> = std::iter::from_fn(|| sched.pop_microtask())
        .map(|task| {
            let mut tags = Vec::new();
            task.for_each_root(|v| tags.push(v.clone()));
            match tags.as_slice() {
                [value] => value.as_number().expect("root must be a Number"),
                _ => panic!("unexpected roots shape"),
            }
        })
        .collect();

    assert_eq!(popped_tags, vec![1.0, 2.0, 3.0]);
}

fn next_request(tag: f64) -> super::super::AsyncGenRequest {
    super::super::AsyncGenRequest {
        kind: super::super::AsyncGenRequestKind::Next,
        value: JsValue::number(tag),
        promise: JsValue::UNDEFINED,
    }
}

#[test]
fn async_gen_queues_are_isolated_per_generator() {
    let mut sched = JobScheduler::default();
    sched
        .async_gen_queue_or_default(1)
        .push_back(next_request(1.0));

    assert!(
        sched.async_gen_queue(2).is_none(),
        "pushing to gen 1 must not create or populate gen 2"
    );
    assert_eq!(
        sched.async_gen_queue(1).map(|q| q.len()),
        Some(1),
        "gen 1 must hold exactly one request"
    );
}

fn collect_roots(sched: &JobScheduler) -> Vec<JsValue> {
    let mut roots = Vec::new();
    sched.for_each_root(|v| roots.push(v.clone()));
    roots
}

#[test]
fn for_each_root_visits_the_generator_and_every_request_field() {
    let mut sched = JobScheduler::default();
    sched
        .async_gen_queue_or_default(7)
        .push_back(super::super::AsyncGenRequest {
            value: JsValue::object(10),
            promise: JsValue::object(11),
            ..next_request(0.0)
        });

    let mut ids: Vec<u64> = collect_roots(&sched)
        .iter()
        .filter_map(|v| v.as_object_id())
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, vec![7, 10, 11]);
}

#[test]
fn async_gen_requests_pop_in_fifo_order() {
    let mut sched = JobScheduler::default();
    let q = sched.async_gen_queue_or_default(42);
    q.push_back(next_request(1.0));
    q.push_back(next_request(2.0));
    q.push_back(next_request(3.0));

    let q = sched
        .async_gen_queue_mut(42)
        .expect("gen 42 must have a queue");
    let tags: Vec<f64> = std::iter::from_fn(|| q.pop_front())
        .map(|r| r.value.as_number().expect("request value must be a Number"))
        .collect();

    assert_eq!(tags, vec![1.0, 2.0, 3.0]);
}

#[test]
fn pending_async_jobs_count_reflects_increments() {
    let sched = JobScheduler::default();
    assert_eq!(sched.pending_async_jobs_count(), 0);
    sched.incr_pending_async_jobs();
    sched.incr_pending_async_jobs();
    assert_eq!(sched.pending_async_jobs_count(), 2);
}

#[test]
fn pending_async_jobs_handle_shares_state_with_scheduler() {
    let sched = JobScheduler::default();
    let handle = sched.pending_async_jobs_handle();
    handle.fetch_add(3, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        sched.pending_async_jobs_count(),
        3,
        "external handle increments must be visible through the scheduler"
    );
}

#[test]
fn pending_async_promise_ids_handle_shares_state_with_scheduler() {
    let sched = JobScheduler::default();
    let handle = sched.pending_async_promise_ids_handle();
    handle.lock().unwrap().insert(42);
    assert!(
        sched.pending_async_promise_ids_lock().contains(&42),
        "external handle updates must be visible through the scheduler"
    );
}

#[test]
fn async_function_ids_are_monotonic_and_unique() {
    let mut sched = JobScheduler::default();
    let ids: Vec<u64> = (0..3).map(|_| sched.alloc_async_function_id()).collect();
    assert_eq!(ids, vec![0, 1, 2]);
}

fn timer_tag(n: f64) -> JsValue {
    JsValue::number(n)
}

/// Collect a due batch the way the event loop does — take the ids, then
/// fire each in turn — and return the callback tags in order.
fn drain_due(q: &mut TimerQueue, now: Instant) -> Vec<f64> {
    q.take_due(now)
        .into_iter()
        .filter_map(|id| q.take_for_firing(id))
        .map(|(cb, _)| cb.as_number().expect("callback tag must be a Number"))
        .collect()
}

#[test]
fn timer_ids_are_monotonic_unique_and_never_zero() {
    let mut q = TimerQueue::default();
    let ids: Vec<u64> = (0..3)
        .map(|i| q.add(timer_tag(i as f64), Vec::new(), Duration::ZERO, false))
        .collect();

    assert_eq!(
        ids,
        vec![1, 2, 3],
        "ids start at 1 so a timer id is always truthy in JS"
    );
}

#[test]
fn due_timers_fire_in_deadline_then_arming_order() {
    let mut q = TimerQueue::default();
    q.add(timer_tag(1.0), Vec::new(), Duration::from_millis(50), false);
    q.add(timer_tag(2.0), Vec::new(), Duration::ZERO, false);
    q.add(timer_tag(3.0), Vec::new(), Duration::ZERO, false);

    let now = Instant::now() + Duration::from_millis(100);
    assert_eq!(
        drain_due(&mut q, now),
        vec![2.0, 3.0, 1.0],
        "same-deadline timers keep arming order; earlier deadlines come first"
    );
    assert!(q.is_empty(), "one-shot timers are removed once they fire");
}

#[test]
fn cleared_timer_never_fires_and_drops_its_roots() {
    let mut q = TimerQueue::default();
    let id = q.add(timer_tag(1.0), Vec::new(), Duration::ZERO, false);
    q.add(timer_tag(2.0), Vec::new(), Duration::ZERO, false);

    q.clear(id);
    assert_eq!(
        q.iter_roots().count(),
        1,
        "a cleared timer must stop rooting its callback"
    );

    assert_eq!(
        drain_due(&mut q, Instant::now() + Duration::from_millis(10)),
        vec![2.0]
    );
}

#[test]
fn clearing_an_unknown_id_is_a_no_op() {
    let mut q = TimerQueue::default();
    q.add(timer_tag(1.0), Vec::new(), Duration::ZERO, false);

    q.clear(0);
    q.clear(999);

    assert_eq!(
        drain_due(&mut q, Instant::now() + Duration::from_millis(10)),
        vec![1.0]
    );
}

#[test]
fn interval_rearms_but_does_not_refire_within_one_batch() {
    let mut q = TimerQueue::default();
    q.add(timer_tag(7.0), Vec::new(), Duration::ZERO, true);

    // A zero-delay interval re-arms to exactly `now`; it must still fire
    // only once per batch, or `take_due` would never terminate.
    let now = Instant::now() + Duration::from_millis(10);
    assert_eq!(drain_due(&mut q, now), vec![7.0]);
    assert!(!q.is_empty(), "an interval stays armed after firing");

    assert_eq!(
        drain_due(&mut q, now + Duration::from_millis(1)),
        vec![7.0],
        "the re-armed interval fires again on the next turn"
    );
}

#[test]
fn cleared_interval_stops_firing() {
    let mut q = TimerQueue::default();
    let id = q.add(timer_tag(7.0), Vec::new(), Duration::ZERO, true);

    let now = Instant::now() + Duration::from_millis(10);
    assert_eq!(drain_due(&mut q, now), vec![7.0]);

    q.clear(id);
    assert!(q.is_empty());
    assert!(
        q.take_due(now + Duration::from_secs(1)).is_empty(),
        "the heap entry left behind by the re-arm must be skipped as stale"
    );
}

#[test]
fn effectively_infinite_delay_never_fires_but_keeps_the_loop_alive() {
    // What `setTimeout(fn, Infinity)` produces. The deadline is
    // representable — hundreds of millions of years out — so the timer is
    // armed normally and simply never comes due.
    let mut q = TimerQueue::default();
    q.add(
        timer_tag(1.0),
        Vec::new(),
        Duration::from_millis(u64::MAX),
        false,
    );

    assert!(!q.is_empty(), "the timer still keeps the event loop alive");
    assert!(
        q.take_due(Instant::now() + Duration::from_secs(60))
            .is_empty(),
        "it must not fire"
    );
}

#[test]
fn unrepresentable_deadline_is_armed_without_overflowing() {
    // A delay past what `Instant` can represent must not panic; the timer
    // holds the loop open but has no deadline to wait on.
    let mut q = TimerQueue::default();
    q.add(timer_tag(1.0), Vec::new(), Duration::MAX, false);

    assert!(!q.is_empty(), "the timer still keeps the event loop alive");
    assert!(q.next_deadline().is_none(), "it has no reachable deadline");
    assert!(
        q.take_due(Instant::now() + Duration::from_secs(60))
            .is_empty()
    );
}

#[test]
fn a_timer_can_be_cancelled_after_its_batch_is_collected() {
    // Node semantics: a timer cleared from inside a sibling's callback in
    // the same tick does not run. The batch is collected as ids, so the
    // sibling is still in the queue and `clear` still reaches it.
    let mut q = TimerQueue::default();
    q.add(timer_tag(1.0), Vec::new(), Duration::ZERO, false);
    let doomed = q.add(timer_tag(2.0), Vec::new(), Duration::ZERO, false);

    let due = q.take_due(Instant::now() + Duration::from_millis(10));
    assert_eq!(due.len(), 2, "both timers are due");

    // Fire the first, and let it cancel the second.
    assert!(q.take_for_firing(due[0]).is_some());
    q.clear(doomed);

    assert!(
        q.take_for_firing(due[1]).is_none(),
        "a timer cancelled mid-batch must not fire"
    );
}

#[test]
fn a_due_timer_stays_rooted_until_it_actually_fires() {
    // Collecting the batch must not drop the GC roots of timers that have
    // not run yet, or a collection during an earlier callback would take
    // them with it.
    let mut q = TimerQueue::default();
    q.add(timer_tag(1.0), Vec::new(), Duration::ZERO, false);
    q.add(timer_tag(2.0), Vec::new(), Duration::ZERO, false);

    let due = q.take_due(Instant::now() + Duration::from_millis(10));
    assert_eq!(
        q.iter_roots().count(),
        2,
        "both callbacks are still rooted while the batch is pending"
    );

    q.take_for_firing(due[0]);
    assert_eq!(
        q.iter_roots().count(),
        1,
        "a timer stops being rooted only once it is taken to run"
    );
}

#[test]
fn arm_then_cancel_churn_does_not_grow_the_index() {
    // The workload from the issue: debounce/throttle arms a far-future
    // timer and cancels it, over and over. Those deadlines are never
    // reached by a pop, so without compaction the index grows for ever.
    let mut q = TimerQueue::default();
    for _ in 0..10_000 {
        let id = q.add(timer_tag(1.0), Vec::new(), Duration::from_secs(3600), false);
        q.clear(id);
    }

    assert!(q.is_empty(), "every timer was cancelled");
    assert!(
        q.index_len() <= 64,
        "stale index entries must be reclaimed, found {}",
        q.index_len()
    );
}

#[test]
fn compaction_preserves_the_live_timers_and_their_order() {
    // Interleave keep/cancel pairs so compaction runs with live timers
    // present, then check every survivor still fires, in order.
    let mut q = TimerQueue::default();
    for i in 0..200u64 {
        let doomed = q.add(
            timer_tag(-1.0),
            Vec::new(),
            Duration::from_secs(3600),
            false,
        );
        q.add(
            timer_tag(i as f64),
            Vec::new(),
            Duration::from_millis(i),
            false,
        );
        q.clear(doomed);
    }

    let fired = drain_due(&mut q, Instant::now() + Duration::from_secs(60));
    assert_eq!(fired.len(), 200, "no live timer may be lost to compaction");
    assert_eq!(
        fired,
        (0..200).map(|i| i as f64).collect::<Vec<_>>(),
        "compaction must not disturb deadline order"
    );
}

#[test]
fn next_deadline_skips_cleared_timers() {
    let mut q = TimerQueue::default();
    let soon = q.add(timer_tag(1.0), Vec::new(), Duration::ZERO, false);
    q.add(timer_tag(2.0), Vec::new(), Duration::from_secs(60), false);

    let with_soon = q.next_deadline().expect("a deadline is armed");
    q.clear(soon);
    let after_clear = q.next_deadline().expect("the later timer remains");

    assert!(
        after_clear > with_soon,
        "clearing the soonest timer must push the reported deadline out"
    );
}

#[test]
fn timer_roots_cover_the_callback_and_its_bound_arguments() {
    let mut q = TimerQueue::default();
    q.add(
        timer_tag(1.0),
        vec![timer_tag(2.0), timer_tag(3.0)],
        Duration::ZERO,
        false,
    );

    let (callback, args) = q.iter_roots().next().expect("one armed timer");
    assert_eq!(callback.as_number(), Some(1.0));
    assert_eq!(
        args.len(),
        2,
        "bound arguments are rooted alongside the callback"
    );
}

#[test]
fn timers_carry_their_bound_arguments_to_the_fire_site() {
    let mut q = TimerQueue::default();
    q.add(timer_tag(1.0), vec![timer_tag(9.0)], Duration::ZERO, false);

    let due = q.take_due(Instant::now() + Duration::from_millis(10));
    assert_eq!(due.len(), 1);
    let (_, args) = q.take_for_firing(due[0]).expect("timer is still armed");
    assert_eq!(args[0].as_number(), Some(9.0));
}

#[test]
fn async_gen_yield_pending_round_trip() {
    let mut sched = JobScheduler::default();
    assert!(
        !sched.is_async_gen_yield_pending(),
        "yield-pending must default to false"
    );

    sched.set_async_gen_yield_pending(true);
    assert!(sched.is_async_gen_yield_pending());

    sched.set_async_gen_yield_pending(false);
    assert!(!sched.is_async_gen_yield_pending());
}
