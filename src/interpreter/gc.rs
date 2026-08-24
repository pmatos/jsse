use super::*;

const GC_SATURATED_MAJOR_NURSERIES: usize = 1;

/// Allocation-pressure pacing for the garbage collector.
///
/// Owns the accounting that decides *when* to collect: how many objects and
/// bytes have been charged since the last collection, how many off-heap bytes
/// are currently live, and the adaptive byte budget that grows with the live
/// set. The mark/sweep mechanics stay in [`Interpreter::gc_safepoint`]; this is
/// purely the "should we collect now?" heuristic, exercised through the small
/// interface below rather than by poking counters on the interpreter.
pub(crate) struct GcPacer {
    /// Object allocations currently charged to the nursery.
    nursery_alloc_count: usize,
    /// Approximate bytes currently charged to the nursery.
    nursery_bytes: usize,
    /// Heap + off-heap allocation debt since the last major collection.
    major_bytes_since_gc: usize,
    /// Live off-heap bytes currently tracked (e.g. ArrayBuffer backing stores).
    external_bytes: usize,
    /// Adaptive major byte budget; exceeding it requests a full collection.
    major_threshold_bytes: usize,
    /// A minor collection has been requested and not yet performed.
    minor_requested: bool,
    /// A full collection has been requested and not yet performed.
    major_requested: bool,
    /// Dense mutation or high survival made routine minor collection
    /// counterproductive. Run a major after the next nursery budget.
    minor_suppressed: bool,
    /// Consecutive minor collections in which at least 90% of the nursery
    /// survived. Two saturated minors switch back to major pacing.
    high_survival_minors: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CollectionKind {
    Minor,
    Major,
}

impl GcPacer {
    pub(crate) fn new() -> Self {
        GcPacer {
            nursery_alloc_count: 0,
            nursery_bytes: 0,
            major_bytes_since_gc: 0,
            external_bytes: 0,
            major_threshold_bytes: GC_INITIAL_THRESHOLD_BYTES,
            minor_requested: false,
            major_requested: false,
            minor_suppressed: false,
            high_survival_minors: 0,
        }
    }

    /// Charge one object allocation. Reused logical slots still hold a full
    /// live payload and therefore carry full major-collection pressure.
    pub(crate) fn charge_object(&mut self, _reused: bool) {
        self.nursery_alloc_count += 1;
        self.nursery_bytes += GC_OBJECT_OVERHEAD;
        self.major_bytes_since_gc += GC_OBJECT_OVERHEAD;
        let probe_threshold =
            GC_NURSERY_THRESHOLD_BYTES.saturating_mul(GC_SATURATED_MAJOR_NURSERIES);
        if self.minor_suppressed && self.nursery_bytes >= probe_threshold {
            self.major_requested = true;
        } else if !self.minor_suppressed && self.nursery_bytes >= GC_NURSERY_THRESHOLD_BYTES {
            self.minor_requested = true;
        }
        if self.major_bytes_since_gc >= self.major_threshold_bytes {
            self.major_requested = true;
        }
    }

    /// Charge newly tracked off-heap bytes against the budget.
    pub(crate) fn track_external(&mut self, bytes: usize) {
        self.external_bytes += bytes;
        self.major_bytes_since_gc += bytes;
        if self.major_bytes_since_gc >= self.major_threshold_bytes {
            self.major_requested = true;
        }
    }

    /// Release previously tracked off-heap bytes (saturating at zero).
    pub(crate) fn release_external(&mut self, bytes: usize) {
        self.external_bytes = self.external_bytes.saturating_sub(bytes);
    }

    /// Force a collection at the next safepoint.
    pub(crate) fn request(&mut self) {
        self.major_requested = true;
    }

    #[cfg(test)]
    pub(crate) fn request_minor(&mut self) {
        self.minor_requested = true;
    }

    /// Consume the highest-priority pending request at a safepoint.
    pub(crate) fn begin_collection(&mut self) -> Option<CollectionKind> {
        if self.major_requested {
            self.major_requested = false;
            self.minor_requested = false;
            Some(CollectionKind::Major)
        } else if self.minor_requested {
            self.minor_requested = false;
            Some(CollectionKind::Minor)
        } else {
            None
        }
    }

    pub(crate) fn end_minor_collection(&mut self, survived: usize, examined: usize) {
        self.nursery_alloc_count = 0;
        self.nursery_bytes = 0;
        if examined > 0 && survived.saturating_mul(10) >= examined.saturating_mul(9) {
            self.high_survival_minors = self.high_survival_minors.saturating_add(1);
            if self.high_survival_minors >= 2 {
                self.minor_suppressed = true;
            }
        } else {
            self.high_survival_minors = 0;
        }
    }

    /// Pause minor collection after an unproductive scavenge. Nursery objects
    /// remain allocated until the next budget requests a major collection.
    pub(crate) fn suppress_minor_temporarily(&mut self) {
        self.minor_requested = false;
        self.minor_suppressed = true;
        self.nursery_alloc_count = 0;
        self.nursery_bytes = 0;
    }

    /// Re-pace after a full collection: grow the next byte budget from the
    /// surviving live set and reset both allocation cycles.
    pub(crate) fn end_major_collection(&mut self, live_object_count: usize) {
        let live_bytes = live_object_count * GC_OBJECT_OVERHEAD + self.external_bytes;
        let growth_debt = live_bytes.saturating_mul(GC_GROWTH_FACTOR.saturating_sub(1));
        self.major_threshold_bytes = std::cmp::max(GC_MAJOR_MIN_THRESHOLD_BYTES, growth_debt);
        self.major_bytes_since_gc = 0;
        self.nursery_alloc_count = 0;
        self.nursery_bytes = 0;
        self.minor_suppressed = false;
        self.high_survival_minors = 0;
    }

    #[cfg(test)]
    pub(crate) fn is_requested(&self) -> bool {
        self.minor_requested || self.major_requested
    }

    #[cfg(test)]
    pub(crate) fn alloc_count(&self) -> usize {
        self.nursery_alloc_count
    }

    #[cfg(test)]
    pub(crate) fn bytes_since_gc(&self) -> usize {
        self.major_bytes_since_gc
    }

    #[cfg(test)]
    pub(crate) fn external_bytes(&self) -> usize {
        self.external_bytes
    }

    #[cfg(test)]
    pub(crate) fn threshold_bytes(&self) -> usize {
        self.major_threshold_bytes
    }
}

impl Interpreter {
    /// Make an object reference that only exists as a native-closure capture
    /// visible to the tracer, by pinning it on `anchor`'s `gc_native_roots`.
    ///
    /// A `JsFunction::native` closure captures its `JsValue`s inside an
    /// `Rc<dyn Fn>`, which `trace_object_fields` cannot walk. Anything reachable
    /// only that way is free to be collected while the closure is still live —
    /// the closure then calls or reads a dead id, silently. Pinning records the
    /// edge on an object the tracer does visit.
    ///
    /// `anchor` must be an object that stays reachable for at least as long as
    /// the closure needs the value. Usually that is the closure's own function
    /// object, once something traced (a promise reaction list, a property) holds
    /// it; where a value must outlive the individual closure that produced it,
    /// anchor it on the longer-lived object they share instead.
    ///
    /// `borrow_mut` runs the generational write barrier, so an old anchor that
    /// gains a young root is remembered for the next minor collection. Pins only
    /// ever accumulate: there is no unpin, so they are released when the anchor
    /// itself dies.
    pub(crate) fn pin_native_root(&self, anchor: &JsValue, value: &JsValue) {
        if value.as_object_id().is_none() {
            return;
        }
        if let Some(anchor_id) = anchor.as_object_id()
            && let Some(cell) = self.get_object_cell(anchor_id)
        {
            cell.borrow_mut()
                .gc_native_roots
                .get_or_insert_with(Vec::new)
                .push(value.clone());
        }
    }

    /// Allocate a fresh object slot for `data` and return its id. The id is
    /// written to `data.id` inside the arena's `alloc`, so the field is set
    /// exactly once at allocation and never reassigned.
    pub(crate) fn alloc_object(&mut self, mut data: JsObjectData) -> u64 {
        data.shape_id = crate::interpreter::types::fresh_shape_id();
        let (id, was_reuse) = self.objects.alloc(data);
        self.gc.charge_object(was_reuse);
        id
    }

    pub(crate) fn gc_track_external_bytes(&mut self, bytes: usize) {
        self.gc.track_external(bytes);
    }

    pub(crate) fn gc_untrack_external_bytes(&mut self, bytes: usize) {
        self.gc.release_external(bytes);
    }

    /// Precise barrier for mutation seams that know the value being stored.
    ///
    /// A primitive or old value cannot introduce an old-to-young edge, so
    /// these hot paths may use `borrow_mut_untracked` after this check. Other
    /// mutable borrows retain the conservative object-level barrier.
    pub(crate) fn gc_write_barrier_value(
        &self,
        owner: &crate::interpreter::object_arena::ObjectHandle,
        value: &JsValue,
    ) {
        if let Some(object_id) = value.as_object_id()
            && self
                .objects
                .get_cell(object_id)
                .is_some_and(crate::interpreter::object_arena::ObjectHandle::is_young)
        {
            owner.remember_if_old();
        }
    }

    fn enqueue_unmarked<I>(candidates: I, worklist: &mut Vec<u64>, marks: &mut [bool])
    where
        I: IntoIterator<Item = u64>,
    {
        for id in candidates {
            let idx = id as usize;
            if idx < marks.len() && !marks[idx] {
                marks[idx] = true;
                worklist.push(id);
            }
        }
    }

    fn trace_mark_worklist(
        &self,
        worklist: &mut Vec<u64>,
        marks: &mut [bool],
        seen_envs: &mut HashSet<usize>,
    ) {
        let mut children = Vec::new();
        while let Some(id) = worklist.pop() {
            let Some(obj_rc) = self.objects.get(id) else {
                continue;
            };
            let obj = obj_rc.borrow();
            children.clear();
            Self::trace_object_fields(&obj, &mut children, seen_envs);
            Self::enqueue_unmarked(children.drain(..), worklist, marks);
        }
    }

    pub(crate) fn gc_safepoint(&mut self) {
        match self.gc.begin_collection() {
            Some(CollectionKind::Minor) => self.gc_collect_minor(),
            Some(CollectionKind::Major) => self.gc_collect_major(),
            None => {}
        }
    }

    fn collect_gc_roots(&self) -> (Vec<u64>, HashSet<usize>) {
        let mut roots = Vec::new();
        let mut seen_envs = HashSet::new();

        for realm in &self.realms {
            realm.collect_roots(&mut roots, &mut seen_envs);
        }
        if let Some(id) = self.new_target.as_ref().and_then(JsValue::as_object_id) {
            roots.push(id);
        }
        // Module environments are not necessarily reachable from global_env.
        for module in self.module_registry.values() {
            let m = module.borrow();
            Self::collect_env_roots(&m.env, &mut roots, &mut seen_envs);
            for val in m.exports.values() {
                Self::collect_value_roots(val, &mut roots);
            }
            if let Some(ms) = &m.module_source {
                Self::collect_value_roots(ms, &mut roots);
            }
            if let Some((promise, resolve, reject)) = &m.top_level_capability {
                Self::collect_value_roots(promise, &mut roots);
                Self::collect_value_roots(resolve, &mut roots);
                Self::collect_value_roots(reject, &mut roots);
            }
        }
        for module in self.synthetic_module_registry.values() {
            let m = module.borrow();
            Self::collect_env_roots(&m.env, &mut roots, &mut seen_envs);
            for val in m.exports.values() {
                Self::collect_value_roots(val, &mut roots);
            }
            if let Some(ms) = &m.module_source {
                Self::collect_value_roots(ms, &mut roots);
            }
        }
        for env in &self.call_stack_envs {
            Self::collect_env_roots(env, &mut roots, &mut seen_envs);
        }
        for frame in &self.call_stack_frames {
            if frame.func_obj_id != 0 {
                roots.push(frame.func_obj_id);
            }
            match &frame.arguments {
                CallFrameArguments::None => {}
                CallFrameArguments::Materialized(arguments) => {
                    Self::collect_value_roots(arguments, &mut roots);
                }
                CallFrameArguments::Deferred { args, func_env, .. } => {
                    for argument in args.values() {
                        Self::collect_value_roots(argument, &mut roots);
                    }
                    Self::collect_env_roots(func_env, &mut roots, &mut seen_envs);
                }
            }
        }
        roots.extend_from_slice(&self.gc_temp_roots);
        // Values held by active bytecode operand stacks
        roots.extend_from_slice(&self.gc_bytecode_roots);
        // Queued microtasks and armed timers both keep their values alive.
        self.scheduler
            .for_each_root(|val| Self::collect_value_roots(val, &mut roots));
        for val in &self.pending_iter_close {
            Self::collect_value_roots(val, &mut roots);
        }
        for iters in self.generator_inline_iters.values() {
            for val in iters {
                Self::collect_value_roots(val, &mut roots);
            }
        }
        for val in self.iterator_next_cache.values() {
            Self::collect_value_roots(val, &mut roots);
        }
        for afs in self.scheduler.iter_async_function_states() {
            Self::collect_env_roots(&afs.func_env, &mut roots, &mut seen_envs);
            Self::collect_value_roots(&afs.resolve_fn, &mut roots);
            Self::collect_value_roots(&afs.reject_fn, &mut roots);
            if let Some(ref v) = afs.pending_return {
                Self::collect_value_roots(v, &mut roots);
            }
            if let Some(ref v) = afs.saved_finally_exception {
                Self::collect_value_roots(v, &mut roots);
            }
            for loop_state in &afs.for_of_stack {
                Self::collect_env_roots(&loop_state.outer_env, &mut roots, &mut seen_envs);
                if let Some(ref env) = loop_state.iteration_env {
                    Self::collect_env_roots(env, &mut roots, &mut seen_envs);
                }
            }
        }

        (roots, seen_envs)
    }

    fn enqueue_young<I>(&self, candidates: I, worklist: &mut Vec<u64>)
    where
        I: IntoIterator<Item = u64>,
    {
        for id in candidates {
            if self
                .objects
                .get_cell(id)
                .is_some_and(crate::interpreter::object_arena::ObjectHandle::mark_young)
            {
                worklist.push(id);
            }
        }
    }

    fn trace_young_worklist(
        &self,
        worklist: &mut Vec<u64>,
        seen_envs: &mut HashSet<usize>,
        weak_containers: &mut Vec<u64>,
    ) {
        let mut children = Vec::new();
        while let Some(id) = worklist.pop() {
            let Some(handle) = self.objects.get_cell(id) else {
                continue;
            };
            let obj = handle.borrow_untracked();
            if obj.class_name == "WeakMap" || obj.class_name == "WeakSet" {
                weak_containers.push(id);
            }
            children.clear();
            Self::trace_object_fields(&obj, &mut children, seen_envs);
            drop(obj);
            self.enqueue_young(children.drain(..), worklist);
        }
    }

    fn object_requires_persistent_minor_scan(obj: &JsObjectData) -> bool {
        let has_closure_env = matches!(obj.callable, Some(JsFunction::User { .. }));
        has_closure_env
            || matches!(
                obj.kind,
                ObjectKind::Iterator(_) | ObjectKind::Arguments(_) | ObjectKind::ModuleNamespace(_)
            )
    }

    fn object_has_young_weak_edge(&self, obj: &JsObjectData) -> bool {
        if obj.class_name == "WeakMap" {
            obj.map_data().is_some_and(|entries| {
                entries.iter().flatten().any(|(key, value)| {
                    [key, value].iter().any(|candidate| {
                        let Some(object_id) = candidate.as_object_id() else {
                            return false;
                        };
                        self.objects
                            .get_cell(object_id)
                            .is_some_and(|handle| handle.is_young())
                    })
                })
            })
        } else if obj.class_name == "WeakSet" {
            obj.set_data().is_some_and(|entries| {
                entries.iter().flatten().any(|value| {
                    let Some(object_id) = value.as_object_id() else {
                        return false;
                    };
                    self.objects
                        .get_cell(object_id)
                        .is_some_and(|handle| handle.is_young())
                })
            })
        } else {
            false
        }
    }

    fn minor_value_is_live(&self, id: u64) -> bool {
        self.objects
            .get_cell(id)
            .is_some_and(|handle| handle.is_old() || handle.is_young_marked())
    }

    fn remembered_set_is_dense_counts(live: usize, remembered: usize) -> bool {
        live >= GC_NURSERY_THRESHOLD_BYTES / GC_OBJECT_OVERHEAD
            && remembered.saturating_mul(4) >= live.saturating_mul(3)
    }

    fn remembered_set_is_dense(&self) -> bool {
        Self::remembered_set_is_dense_counts(
            self.objects.live_count(),
            self.objects.remembered_len(),
        )
    }

    fn gc_collect_minor(&mut self) {
        // A coarse object barrier can become as expensive as a full trace on
        // mutation-heavy live sets. Preserve the old adaptive major pacing
        // instead of repeatedly running counterproductive near-major minors.
        if self.remembered_set_is_dense() {
            self.gc.suppress_minor_temporarily();
            return;
        }

        let (roots, mut seen_envs) = self.collect_gc_roots();
        let remembered = self.objects.take_remembered();
        let mut worklist = Vec::new();
        let mut weak_containers = Vec::new();
        let mut children = Vec::new();

        // Old roots are already known to survive a minor collection. Only
        // young roots and old objects admitted by the write barrier need work.
        self.enqueue_young(roots, &mut worklist);
        for id in remembered {
            let Some(handle) = self.objects.get_cell(id) else {
                continue;
            };
            if !handle.is_old() {
                continue;
            }
            let obj = handle.borrow_untracked();
            if obj.class_name == "WeakMap" || obj.class_name == "WeakSet" {
                weak_containers.push(id);
            }
            children.clear();
            Self::trace_object_fields(&obj, &mut children, &mut seen_envs);
            let retain = Self::object_requires_persistent_minor_scan(&obj)
                || self.object_has_young_weak_edge(&obj)
                || children.iter().any(|&child| {
                    self.objects
                        .get_cell(child)
                        .is_some_and(|child_handle| child_handle.is_young())
                });
            drop(obj);
            self.enqueue_young(children.drain(..), &mut worklist);
            if retain {
                handle.remember_if_old();
            }
        }
        self.trace_young_worklist(&mut worklist, &mut seen_envs, &mut weak_containers);

        // Minor ephemeron fixpoint. Old keys are conservatively live until a
        // major collection; young keys must be marked through a strong path.
        loop {
            let mut new_marks = false;
            weak_containers.sort_unstable();
            weak_containers.dedup();
            for &id in &weak_containers {
                let Some(handle) = self.objects.get_cell(id) else {
                    continue;
                };
                if handle.is_young() && !handle.is_young_marked() {
                    continue;
                }
                let obj = handle.borrow_untracked();
                if obj.class_name != "WeakMap" {
                    continue;
                }
                if let Some(entries) = obj.map_data() {
                    for entry in entries.iter().flatten() {
                        if let Some(key_id) = entry.0.as_object_id()
                            && self.minor_value_is_live(key_id)
                            && let Some(value_id) = entry.1.as_object_id()
                            && self
                                .objects
                                .get_cell(value_id)
                                .is_some_and(|value_handle| value_handle.mark_young())
                        {
                            worklist.push(value_id);
                            new_marks = true;
                        }
                    }
                }
            }
            self.trace_young_worklist(&mut worklist, &mut seen_envs, &mut weak_containers);
            if !new_marks {
                break;
            }
        }

        // Remove weak entries whose young key/member did not survive. Old
        // values are never reclaimed by this collection.
        weak_containers.sort_unstable();
        weak_containers.dedup();
        for id in weak_containers {
            let Some(handle) = self.objects.get_cell(id) else {
                continue;
            };
            if handle.is_young() && !handle.is_young_marked() {
                continue;
            }
            let mut obj = handle.borrow_mut_untracked();
            if obj.class_name == "WeakMap" {
                if let Some(entries) = obj.map_data_mut() {
                    for entry in entries.iter_mut() {
                        let dead = entry.as_ref().is_some_and(|(key, _)| {
                            key.as_object_id()
                                .is_some_and(|id| !self.minor_value_is_live(id))
                        });
                        if dead {
                            *entry = None;
                        }
                    }
                }
            } else if obj.class_name == "WeakSet"
                && let Some(entries) = obj.set_data_mut()
            {
                for entry in entries.iter_mut() {
                    let dead = entry.as_ref().is_some_and(|value| {
                        value
                            .as_object_id()
                            .is_some_and(|id| !self.minor_value_is_live(id))
                    });
                    if dead {
                        *entry = None;
                    }
                }
            }
        }

        let nursery = self.objects.take_nursery();
        let examined = nursery.len();
        let mut survived = 0;
        let mut survivors = Vec::with_capacity(examined);
        for id in nursery {
            let Some(handle) = self.objects.get(id) else {
                continue;
            };
            if !handle.is_young_marked() {
                self.free_gc_object(id);
            } else {
                survived += 1;
                if handle.age_survivor(GC_PROMOTION_AGE) {
                    handle.promote();
                } else {
                    handle.clear_young_mark();
                    survivors.push(id);
                }
            }
        }
        self.objects.replace_nursery(survivors);
        self.gc.end_minor_collection(survived, examined);
    }

    fn free_gc_object(&mut self, id: u64) {
        if let Some(handle) = self.objects.get_cell(id) {
            let obj = handle.borrow_untracked();
            if let Some(buf_data) = obj.arraybuffer_data()
                && let BufferData::Owned(ref data) = *buf_data.borrow()
            {
                self.gc.release_external(data.len());
            }
        }
        self.objects.free(id);
        self.function_realm_map.remove(&id);
        self.iterator_next_cache.remove(&id);
        self.generator_inline_iters.remove(&id);
    }

    fn gc_collect_major(&mut self) {
        let obj_count = self.objects.capacity() as usize;
        // Reuse the mark bitmap buffer across collections to avoid per-GC
        // allocation churn. clear()+resize(_, false) yields an all-false buffer
        // while keeping the backing capacity.
        let mut marks = std::mem::take(&mut self.gc_marks);
        marks.clear();
        marks.resize(obj_count, false);

        let (roots, mut seen_envs) = self.collect_gc_roots();

        // Mark each object when it is enqueued, not when it is popped. Shared
        // environments and prototypes can expose the same object through many
        // edges; admitting each id once keeps the worklist linear in the live
        // object graph instead of filling it with duplicate entries.
        let mut worklist = Vec::with_capacity(roots.len());
        Self::enqueue_unmarked(roots, &mut worklist, &mut marks);
        self.trace_mark_worklist(&mut worklist, &mut marks, &mut seen_envs);

        // Ephemeron fixpoint: mark WeakMap values whose keys are reachable
        loop {
            let mut new_marks = false;
            for i in 0..obj_count {
                if !marks[i] {
                    continue;
                }
                let obj_rc = match self.objects.get(i as u64) {
                    Some(rc) => rc,
                    None => continue,
                };
                let obj = obj_rc.borrow_untracked();
                if obj.class_name != "WeakMap" {
                    continue;
                }
                if let Some(entries) = obj.map_data() {
                    for entry in entries.iter().flatten() {
                        // Key is reachable — mark the value
                        if let Some(key_id) = entry.0.as_object_id()
                            && (key_id as usize) < obj_count
                            && marks[key_id as usize]
                            && let Some(value_id) = entry.1.as_object_id()
                        {
                            let vid = value_id as usize;
                            if vid < obj_count && !marks[vid] {
                                marks[vid] = true;
                                new_marks = true;
                                worklist.push(value_id);
                            }
                        }
                    }
                }
            }
            // Trace through any newly reachable WeakMap values using the same
            // deduplicated worklist as the main mark phase.
            self.trace_mark_worklist(&mut worklist, &mut marks, &mut seen_envs);
            if !new_marks {
                break;
            }
        }

        // Sweep phase
        for (i, mark) in marks.iter().enumerate().take(obj_count) {
            let id = i as u64;
            let live = self.objects.slot_at(id).is_some_and(|s| s.is_some());
            if !mark && live {
                self.free_gc_object(id);
            }
        }
        // The major sweep leaves no young objects, so discard stale
        // generational tracking before rebuilding the persistent subset.
        self.objects.reset_generations_after_major();

        // Post-sweep: tenure survivors, clear dead weak entries, and keep
        // environment owners visible to future minor collections. Their
        // shared EnvRefs can be mutated without borrowing the owner again.
        for i in 0..obj_count {
            if !marks[i] {
                continue;
            }
            let obj_rc = match self.objects.get(i as u64) {
                Some(rc) => rc,
                None => continue,
            };
            obj_rc.tenure_after_major();
            let mut obj = obj_rc.borrow_mut_untracked();
            if obj.class_name == "WeakMap" {
                if let Some(entries) = obj.map_data_mut() {
                    for entry in entries.iter_mut() {
                        let dead = match entry.as_ref().and_then(|(key, _)| key.as_object_id()) {
                            Some(kid) => (kid as usize) >= obj_count || !marks[kid as usize],
                            None => false,
                        };
                        if dead {
                            *entry = None;
                        }
                    }
                }
            } else if obj.class_name == "WeakSet"
                && let Some(entries) = obj.set_data_mut()
            {
                for entry in entries.iter_mut() {
                    let dead = match entry.as_ref().and_then(JsValue::as_object_id) {
                        Some(vid) => (vid as usize) >= obj_count || !marks[vid as usize],
                        None => false,
                    };
                    if dead {
                        *entry = None;
                    }
                }
            }
            let requires_persistent_scan = Self::object_requires_persistent_minor_scan(&obj);
            drop(obj);
            if requires_persistent_scan {
                obj_rc.remember_if_old();
            }
        }
        let live_count = self.objects.live_count();
        self.gc.end_major_collection(live_count);
        // Return the buffer to the interpreter so its capacity is reused next GC.
        self.gc_marks = marks;
    }

    fn collect_value_roots(val: &JsValue, worklist: &mut Vec<u64>) {
        if let Some(id) = val.as_object_id() {
            worklist.push(id);
        }
    }

    fn trace_object_fields(
        obj: &JsObjectData,
        worklist: &mut Vec<u64>,
        seen_envs: &mut HashSet<usize>,
    ) {
        if let Some(pid) = obj.prototype_id {
            worklist.push(pid);
        }
        for desc in obj.properties.values() {
            if let Some(ref v) = desc.value {
                Self::collect_value_roots(v, worklist);
            }
            if let Some(ref v) = desc.get {
                Self::collect_value_roots(v, worklist);
            }
            if let Some(ref v) = desc.set {
                Self::collect_value_roots(v, worklist);
            }
        }
        if let Some(elems) = obj.array_elements() {
            for v in elems {
                Self::collect_value_roots(v, worklist);
            }
        }
        if let Some(ref v) = obj.primitive_value {
            Self::collect_value_roots(v, worklist);
        }
        for elem in obj.private_fields.values() {
            match elem {
                PrivateElement::Field(v) | PrivateElement::Method(v) => {
                    Self::collect_value_roots(v, worklist);
                }
                PrivateElement::Accessor { get, set } => {
                    if let Some(g) = get {
                        Self::collect_value_roots(g, worklist);
                    }
                    if let Some(s) = set {
                        Self::collect_value_roots(s, worklist);
                    }
                }
            }
        }
        for idef in &obj.class_instance_field_defs {
            if let InstanceFieldDef::Private(def) = idef {
                match def {
                    PrivateFieldDef::Method { value, .. } => {
                        Self::collect_value_roots(value, worklist);
                    }
                    PrivateFieldDef::Accessor { get, set, .. } => {
                        if let Some(g) = get {
                            Self::collect_value_roots(g, worklist);
                        }
                        if let Some(s) = set {
                            Self::collect_value_roots(s, worklist);
                        }
                    }
                    PrivateFieldDef::Field { .. } => {}
                }
            }
        }
        if let Some(ref func) = obj.callable
            && let JsFunction::User { closure, .. } = func
        {
            Self::collect_env_roots(closure, worklist, seen_envs);
        }
        if let Some(ref roots) = obj.gc_native_roots {
            for v in roots {
                Self::collect_value_roots(v, worklist);
            }
        }
        // Kind-specific roots. This is the single point of dispatch — adding a
        // new ObjectKind variant requires updating this match (Rust enforces
        // exhaustiveness), eliminating the "remember to add new prototype fields
        // to maybe_gc()" footgun previously called out in CLAUDE.md.
        use crate::interpreter::types::{IterHelperData, ObjectKind, PromiseState};
        match &obj.kind {
            ObjectKind::Ordinary
            | ObjectKind::RegExp(_)
            | ObjectKind::ArrayBuffer(_)
            | ObjectKind::ShadowRealm(_)
            | ObjectKind::DisposableStack(_)
            | ObjectKind::Temporal(_)
            | ObjectKind::Intl(_)
            | ObjectKind::PrimitiveWrapper(_) => {}
            ObjectKind::Proxy(p) => {
                if let Some(tid) = p.target_id {
                    worklist.push(tid);
                }
                if let Some(hid) = p.handler_id {
                    worklist.push(hid);
                }
            }
            ObjectKind::BoundFunction(b) => {
                Self::collect_value_roots(&b.target, worklist);
                Self::collect_value_roots(&b.this, worklist);
                for v in &b.args {
                    Self::collect_value_roots(v, worklist);
                }
            }
            ObjectKind::WrappedFunction(w) => {
                worklist.push(w.target_id);
            }
            ObjectKind::IterHelper(h) => match h {
                IterHelperData::Delegation { iter, next } => {
                    Self::collect_value_roots(iter, worklist);
                    Self::collect_value_roots(next, worklist);
                }
                IterHelperData::Helper {
                    next,
                    return_closure,
                    ..
                } => {
                    Self::collect_value_roots(next, worklist);
                    Self::collect_value_roots(return_closure, worklist);
                }
            },
            ObjectKind::TypedArray(ta) => {
                if let Some(buf_id) = ta.buffer_object_id {
                    worklist.push(buf_id);
                }
            }
            ObjectKind::DataView(dv) => {
                if let Some(buf_id) = dv.buffer_object_id {
                    worklist.push(buf_id);
                }
            }
            ObjectKind::Promise(pd) => {
                match &pd.state {
                    PromiseState::Fulfilled(v) | PromiseState::Rejected(v) => {
                        Self::collect_value_roots(v, worklist);
                    }
                    PromiseState::Pending => {}
                }
                for reaction in pd
                    .fulfill_reactions
                    .iter()
                    .chain(pd.reject_reactions.iter())
                {
                    if let Some(ref h) = reaction.handler {
                        Self::collect_value_roots(h, worklist);
                    }
                    Self::collect_value_roots(&reaction.resolve, worklist);
                    Self::collect_value_roots(&reaction.reject, worklist);
                    if let Some(pid) = reaction.promise_id {
                        worklist.push(pid);
                    }
                }
            }
            ObjectKind::Map(entries) => {
                // WeakMap entries are visited via the ephemeron fixpoint, not strongly.
                if obj.class_name != "WeakMap" {
                    for entry in entries.iter().flatten() {
                        Self::collect_value_roots(&entry.0, worklist);
                        Self::collect_value_roots(&entry.1, worklist);
                    }
                }
            }
            ObjectKind::Set(entries) => {
                if obj.class_name != "WeakSet" {
                    for val in entries.iter().flatten() {
                        Self::collect_value_roots(val, worklist);
                    }
                }
            }
            ObjectKind::FinalizationRegistry { cells, tokens: _ } => {
                // Cells (target+heldValue) are held WEAKLY via the ephemeron pass;
                // tokens are unregister keys, also weak. No strong roots here.
                for entry in cells.iter().flatten() {
                    Self::collect_value_roots(&entry.1, worklist);
                }
            }
            ObjectKind::Iterator(state) => {
                Self::collect_iterator_state_roots(state, worklist, seen_envs);
            }
            ObjectKind::Arguments(map) => {
                for (env_ref, _) in map.values() {
                    Self::collect_env_roots(env_ref, worklist, seen_envs);
                }
            }
            ObjectKind::Array(_) => {
                // Array elements are visited via the property walk above
                // (array_elements is a separate compact storage; values are also tracked).
            }
            ObjectKind::ModuleNamespace(ns) => {
                Self::collect_env_roots(&ns.env, worklist, seen_envs);
            }
        }
    }

    fn collect_iterator_state_roots(
        state: &IteratorState,
        worklist: &mut Vec<u64>,
        seen_envs: &mut HashSet<usize>,
    ) {
        match state {
            IteratorState::ArrayIterator { array_id, .. } => worklist.push(*array_id),
            IteratorState::TypedArrayIterator { typed_array_id, .. } => {
                worklist.push(*typed_array_id)
            }
            IteratorState::MapIterator { map_id, .. } => worklist.push(*map_id),
            IteratorState::SetIterator { set_id, .. } => worklist.push(*set_id),
            IteratorState::Generator {
                func_env,
                execution_state,
                ..
            }
            | IteratorState::AsyncGenerator {
                func_env,
                execution_state,
                ..
            } => {
                Self::collect_env_roots(func_env, worklist, seen_envs);
                if let GeneratorExecutionState::SuspendedYield { prev_sent, .. } = execution_state {
                    for v in prev_sent {
                        Self::collect_value_roots(v, worklist);
                    }
                }
            }
            IteratorState::StateMachineGenerator {
                func_env,
                delegated_iterator,
                pending_exception,
                pending_return,
                _sent_value,
                ..
            }
            | IteratorState::StateMachineAsyncGenerator {
                func_env,
                delegated_iterator,
                pending_exception,
                pending_return,
                _sent_value,
                ..
            } => {
                Self::collect_env_roots(func_env, worklist, seen_envs);
                Self::collect_value_roots(_sent_value, worklist);
                if let Some(di) = delegated_iterator {
                    Self::collect_value_roots(&di.iterator, worklist);
                    Self::collect_value_roots(&di.next_method, worklist);
                }
                if let Some(v) = pending_exception {
                    Self::collect_value_roots(v, worklist);
                }
                if let Some(v) = pending_return {
                    Self::collect_value_roots(v, worklist);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn collect_env_roots(
        env: &EnvRef,
        worklist: &mut Vec<u64>,
        seen_envs: &mut HashSet<usize>,
    ) {
        let mut current = Some(env.clone());
        while let Some(e) = current {
            let ptr = Rc::as_ptr(&e) as usize;
            if !seen_envs.insert(ptr) {
                break;
            }
            let borrowed = e.borrow();
            for binding in borrowed.bindings.values() {
                Self::collect_value_roots(&binding.value, worklist);
            }
            // The with-target is interned (id-only) — root it explicitly so
            // identifier resolution inside `with(o) { ... }` keeps `o` alive
            // across GC.
            if let Some(ref w) = borrowed.with_object {
                worklist.push(w.obj_id);
            }
            current = borrowed.parent.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(id: u64) -> JsValue {
        JsValue::object(id)
    }

    /// Sort + dedup a worklist so tests can compare against an expected set
    /// without depending on push order or accidental duplicates.
    fn as_set(mut worklist: Vec<u64>) -> Vec<u64> {
        worklist.sort_unstable();
        worklist.dedup();
        worklist
    }

    #[test]
    fn collect_value_roots_pushes_only_objects() {
        let mut worklist = Vec::new();
        Interpreter::collect_value_roots(&obj(42), &mut worklist);
        assert_eq!(worklist, vec![42]);

        let mut worklist = Vec::new();
        Interpreter::collect_value_roots(JsValue::undefined_ref(), &mut worklist);
        Interpreter::collect_value_roots(&JsValue::number(3.0), &mut worklist);
        Interpreter::collect_value_roots(&JsValue::TRUE, &mut worklist);
        assert!(worklist.is_empty());
    }

    #[test]
    fn mark_worklist_enqueues_shared_references_once() {
        let mut marks = vec![false; 5];
        let mut worklist = Vec::new();

        Interpreter::enqueue_unmarked([1, 2, 1, 3, 2, 1, 9], &mut worklist, &mut marks);
        assert_eq!(worklist, vec![1, 2, 3]);

        Interpreter::enqueue_unmarked([2, 4, 3], &mut worklist, &mut marks);
        assert_eq!(worklist, vec![1, 2, 3, 4]);
    }

    #[test]
    fn trace_object_fields_roots_prototype_and_data_properties() {
        let mut data = JsObjectData::new();
        data.prototype_id = Some(7);
        data.properties.insert(
            "x".into(),
            PropertyDescriptor::data(obj(8), true, true, true),
        );
        data.properties.insert(
            "n".into(),
            PropertyDescriptor::data(JsValue::number(1.0), true, true, true),
        );

        let mut worklist = Vec::new();
        Interpreter::trace_object_fields(&data, &mut worklist, &mut HashSet::new());
        assert_eq!(as_set(worklist), vec![7, 8]);
    }

    #[test]
    fn trace_object_fields_roots_accessor_get_and_set() {
        let mut data = JsObjectData::new();
        data.properties.insert(
            "acc".into(),
            PropertyDescriptor::accessor(Some(obj(10)), Some(obj(11)), true, true),
        );

        let mut worklist = Vec::new();
        Interpreter::trace_object_fields(&data, &mut worklist, &mut HashSet::new());
        assert_eq!(as_set(worklist), vec![10, 11]);
    }

    #[test]
    fn trace_object_fields_roots_array_elements_and_native_roots() {
        let mut data = JsObjectData::new();
        data.kind = ObjectKind::Array(vec![obj(20), JsValue::number(0.0), obj(21)]);
        data.gc_native_roots = Some(vec![obj(22)]);

        let mut worklist = Vec::new();
        Interpreter::trace_object_fields(&data, &mut worklist, &mut HashSet::new());
        assert_eq!(as_set(worklist), vec![20, 21, 22]);
    }

    #[test]
    fn collect_env_roots_walks_parent_chain_and_terminates_on_cycle() {
        // (a) child binds "a"=Object(30), parent binds "b"=Object(31) → {30,31}
        let parent = Environment::new(None);
        parent.borrow_mut().bindings.insert(
            "b".to_string(),
            Binding::new(obj(31), BindingKind::Var, true),
        );
        let child = Environment::new(Some(parent.clone()));
        child.borrow_mut().bindings.insert(
            "a".to_string(),
            Binding::new(obj(30), BindingKind::Var, true),
        );

        let mut worklist = Vec::new();
        Interpreter::collect_env_roots(&child, &mut worklist, &mut HashSet::new());
        assert_eq!(as_set(worklist), vec![30, 31]);

        // (b) self-referential env (parent points to itself) binds "c"=Object(32)
        // → terminates without infinite loop, contains 32 exactly once.
        let cyclic = Environment::new(None);
        cyclic.borrow_mut().bindings.insert(
            "c".to_string(),
            Binding::new(obj(32), BindingKind::Var, true),
        );
        cyclic.borrow_mut().parent = Some(cyclic.clone());

        let mut worklist = Vec::new();
        Interpreter::collect_env_roots(&cyclic, &mut worklist, &mut HashSet::new());
        assert_eq!(worklist, vec![32]);
    }

    #[test]
    fn environment_roots_are_scanned_once_per_collection() {
        let env = Environment::new(None);
        env.borrow_mut().bindings.insert(
            "shared".to_string(),
            Binding::new(obj(33), BindingKind::Var, true),
        );
        let mut worklist = Vec::new();
        let mut seen_envs = HashSet::new();

        Interpreter::collect_env_roots(&env, &mut worklist, &mut seen_envs);
        Interpreter::collect_env_roots(&env, &mut worklist, &mut seen_envs);

        assert_eq!(worklist, vec![33]);
    }

    // GcPacer — the allocation-pressure heuristic that decides when to collect.
    // Tested through its public interface; expected budgets are hand-computed
    // literals (independent of the pacer's own arithmetic).

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
        pacer.end_minor_collection(0, nursery_objects);
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
    fn repeated_high_nursery_survival_suppresses_minors_until_major() {
        let mut pacer = GcPacer::new();
        pacer.end_major_collection(100_000);

        pacer.end_minor_collection(90, 100);
        assert!(!pacer.minor_suppressed);
        pacer.end_minor_collection(99, 100);
        assert!(pacer.minor_suppressed);

        for _ in 0..GC_NURSERY_THRESHOLD_BYTES / GC_OBJECT_OVERHEAD {
            pacer.charge_object(true);
        }
        assert_eq!(pacer.begin_collection(), Some(CollectionKind::Major));

        pacer.end_major_collection(100_000);
        assert!(!pacer.minor_suppressed);
        assert_eq!(pacer.high_survival_minors, 0);
    }

    #[test]
    fn dense_remembered_set_requires_a_large_near_major_scan() {
        assert!(!Interpreter::remembered_set_is_dense_counts(8_191, 8_191));
        assert!(!Interpreter::remembered_set_is_dense_counts(10_000, 7_499));
        assert!(Interpreter::remembered_set_is_dense_counts(10_000, 7_500));
    }

    fn tenure_initial_heap(interp: &mut Interpreter) {
        interp.gc.request();
        interp.gc_safepoint();
    }

    #[test]
    fn precise_write_barrier_remembers_only_young_object_values() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let owner = interp.alloc_object(JsObjectData::new());
        interp.gc_temp_roots.push(owner);
        interp.gc.request();
        interp.gc_safepoint();

        let young = interp.alloc_object(JsObjectData::new());
        let owner_handle = interp.objects.get(owner).unwrap();
        assert!(owner_handle.is_old());
        assert!(interp.objects.take_remembered().is_empty());

        interp.gc_write_barrier_value(&owner_handle, &JsValue::number(1.0));
        interp.gc_write_barrier_value(&owner_handle, &obj(owner));
        assert!(interp.objects.take_remembered().is_empty());

        interp.gc_write_barrier_value(&owner_handle, &obj(young));
        assert_eq!(interp.objects.take_remembered(), vec![owner]);
    }

    #[test]
    fn explicit_major_collection_tenures_a_reachable_nursery_object() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let survivor = interp.alloc_object(JsObjectData::new());
        interp.gc_temp_roots.push(survivor);
        assert!(interp.objects.get_cell_expect(survivor).is_young());

        interp.gc.request();
        interp.gc_safepoint();

        assert!(interp.objects.get_cell_expect(survivor).is_old());
    }

    #[test]
    fn major_collection_keeps_environment_owner_remembered() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);

        let env = Environment::new(None);
        let mut owner_data = JsObjectData::new();
        owner_data.kind = ObjectKind::Arguments(HashMap::from([(
            "0".to_string(),
            (env.clone(), "captured".to_string()),
        )]));
        let owner = interp.alloc_object(owner_data);
        interp.gc_temp_roots.push(owner);

        interp.gc.request();
        interp.gc_safepoint();
        assert!(interp.objects.get_cell_expect(owner).is_old());

        let child = interp.alloc_object(JsObjectData::new());
        env.borrow_mut().bindings.insert(
            "captured".to_string(),
            Binding::new(obj(child), BindingKind::Var, true),
        );
        interp.gc.request_minor();
        interp.gc_safepoint();

        assert!(
            interp.objects.get_cell(child).is_some(),
            "an environment mutated after major GC must retain its young object"
        );
    }

    #[test]
    fn minor_collection_reclaims_unreachable_young_object() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let dead = interp.alloc_object(JsObjectData::new());

        interp.gc.request_minor();
        interp.gc_safepoint();

        assert!(interp.objects.get_cell(dead).is_none());
    }

    #[test]
    fn remembered_old_object_keeps_young_child_alive() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let parent = interp.alloc_object(JsObjectData::new());
        interp.gc_temp_roots.push(parent);
        interp.gc.request();
        interp.gc_safepoint();
        assert!(interp.objects.get_cell_expect(parent).is_old());

        let child = interp.alloc_object(JsObjectData::new());
        interp
            .objects
            .get_cell_expect(parent)
            .borrow_mut()
            .insert_value("child".to_string(), obj(child));
        interp.gc.request_minor();
        interp.gc_safepoint();

        assert!(interp.objects.get_cell(child).is_some());
    }

    #[test]
    fn nursery_survivor_promotes_after_two_minor_collections() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let survivor = interp.alloc_object(JsObjectData::new());
        interp.gc_temp_roots.push(survivor);

        interp.gc.request_minor();
        interp.gc_safepoint();
        assert!(interp.objects.get_cell_expect(survivor).is_young());

        interp.gc.request_minor();
        interp.gc_safepoint();
        assert!(interp.objects.get_cell_expect(survivor).is_old());
    }

    #[test]
    fn promoted_parent_is_remembered_until_its_young_child_promotes() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let parent = interp.alloc_object(JsObjectData::new());
        interp.gc_temp_roots.push(parent);

        interp.gc.request_minor();
        interp.gc_safepoint();

        let child = interp.alloc_object(JsObjectData::new());
        interp
            .objects
            .get_cell_expect(parent)
            .borrow_mut()
            .insert_value("child".to_string(), obj(child));
        interp.gc.request_minor();
        interp.gc_safepoint();
        assert!(interp.objects.get_cell_expect(parent).is_old());
        assert!(interp.objects.get_cell_expect(child).is_young());

        interp.gc.request_minor();
        interp.gc_safepoint();
        assert!(
            interp.objects.get_cell(child).is_some(),
            "promotion must retain the old-to-young edge"
        );
    }

    #[test]
    fn old_weakmap_keeps_a_young_value_only_while_its_key_is_live() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);

        let mut weak_map_data = JsObjectData::new();
        weak_map_data.class_name = "WeakMap".to_string();
        weak_map_data.kind = ObjectKind::Map(Vec::new());
        let weak_map = interp.alloc_object(weak_map_data);
        interp.gc_temp_roots.push(weak_map);
        interp.gc.request();
        interp.gc_safepoint();

        let key = interp.alloc_object(JsObjectData::new());
        let value = interp.alloc_object(JsObjectData::new());
        interp.gc_temp_roots.push(key);
        interp
            .objects
            .get_cell_expect(weak_map)
            .borrow_mut()
            .map_data_mut()
            .unwrap()
            .push(Some((obj(key), obj(value))));

        interp.gc.request_minor();
        interp.gc_safepoint();
        assert!(interp.objects.get_cell(value).is_some());

        interp.gc_temp_roots.retain(|&id| id != key);
        interp.gc.request_minor();
        interp.gc_safepoint();
        assert!(interp.objects.get_cell(key).is_none());
        assert!(interp.objects.get_cell(value).is_none());
        assert!(
            interp
                .objects
                .get_cell_expect(weak_map)
                .borrow()
                .map_data()
                .unwrap()[0]
                .is_none()
        );
    }

    #[test]
    fn old_weakset_drops_an_unreachable_young_member() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);

        let mut weak_set_data = JsObjectData::new();
        weak_set_data.class_name = "WeakSet".to_string();
        weak_set_data.kind = ObjectKind::Set(Vec::new());
        let weak_set = interp.alloc_object(weak_set_data);
        interp.gc_temp_roots.push(weak_set);
        interp.gc.request();
        interp.gc_safepoint();

        let member = interp.alloc_object(JsObjectData::new());
        interp
            .objects
            .get_cell_expect(weak_set)
            .borrow_mut()
            .set_data_mut()
            .unwrap()
            .push(Some(obj(member)));

        interp.gc.request_minor();
        interp.gc_safepoint();

        assert!(interp.objects.get_cell(member).is_none());
        assert!(
            interp
                .objects
                .get_cell_expect(weak_set)
                .borrow()
                .set_data()
                .unwrap()[0]
                .is_none()
        );
    }
}
