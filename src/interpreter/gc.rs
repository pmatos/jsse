use super::*;

pub(crate) mod env_registry;
mod module_roots;
mod pacer;

#[cfg(feature = "perf-counters")]
use super::perf_counters::gc_minor as profile;
pub(crate) use pacer::{CollectionKind, GcPacer};

#[path = "gc_rooted_slots.rs"]
mod rooted_slots;
pub(crate) use rooted_slots::RootedSlots;

fn object_ids<'a>(values: impl IntoIterator<Item = &'a JsValue>) -> impl Iterator<Item = u64> {
    values.into_iter().filter_map(JsValue::as_object_id)
}

/// The nursery as a bitmap indexed by object id, filled at the start of each
/// minor collection and cleared (in O(nursery)) at its end, so the buffers
/// persist across collections. Scanning a large remembered Array asks one
/// cache-resident bit per element whether it references a young object,
/// instead of loading each referent's header.
#[derive(Default)]
pub(crate) struct YoungIds {
    bits: Vec<u64>,
    /// Nursery objects whose traced fields referenced a nursery object.
    edge_bits: Vec<u64>,
}

impl YoungIds {
    fn fill(&mut self, nursery: &[u64], capacity: u64) {
        let words = (capacity as usize).div_ceil(64);
        if self.bits.len() < words {
            self.bits.resize(words, 0);
            self.edge_bits.resize(words, 0);
        }
        for &id in nursery {
            self.bits[(id / 64) as usize] |= 1 << (id % 64);
        }
    }

    fn clear(&mut self, nursery: &[u64]) {
        for &id in nursery {
            self.bits[(id / 64) as usize] &= !(1 << (id % 64));
            self.edge_bits[(id / 64) as usize] &= !(1 << (id % 64));
        }
    }

    /// Record that the nursery object `id` references a nursery object.
    #[inline]
    fn note_young_edge(&mut self, id: u64) {
        self.edge_bits[(id / 64) as usize] |= 1 << (id % 64);
    }

    #[inline]
    fn has_young_edge(&self, id: u64) -> bool {
        self.edge_bits[(id / 64) as usize] & (1 << (id % 64)) != 0
    }

    #[inline]
    fn contains(&self, id: u64) -> bool {
        self.bits
            .get((id / 64) as usize)
            .is_some_and(|word| word & (1 << (id % 64)) != 0)
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
    ///
    /// That makes this the wrong tool for a capture that is *replaced* over the
    /// anchor's lifetime — pinning each replacement retains every superseded
    /// value. Pin one arena-backed container instead and mutate its slots in
    /// place; [`RootedSlots`] is that container, and `CONTEXT.md` calls the
    /// shape a Rooted Slot.
    pub(crate) fn pin_native_root(&self, anchor: &JsValue, value: &JsValue) {
        if value.as_object_id().is_none() {
            return;
        }
        if let Some(anchor_id) = anchor.as_object_id()
            && let Some(cell) = self.get_object_cell(anchor_id)
        {
            cell.borrow_mut().gc_native_roots_mut().push(value.clone());
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
        if owner.is_old()
            && let Some(object_id) = value.as_object_id()
            && self
                .objects
                .get_cell(object_id)
                .is_some_and(crate::interpreter::object_arena::ObjectHandle::is_young)
        {
            owner.remember_if_old();
        }
    }

    /// Barrier for a single-slot store of `value` into element `index` of an
    /// Array, run before the store. Unlike [`Self::gc_write_barrier_value`] it
    /// remembers only the card holding `index`, so a minor collection
    /// re-scans that card instead of the whole element vector. The store
    /// itself must not go through `array_elements_mut` (which would dirty
    /// every card again); use `JsObjectData::overwrite_array_element` or
    /// `apply_dense_array_set`.
    pub(crate) fn gc_write_barrier_element(
        &self,
        owner: &crate::interpreter::object_arena::ObjectHandle,
        index: usize,
        value: &JsValue,
    ) {
        if let Some(object_id) = value.as_object_id()
            && owner.is_old()
            && self
                .objects
                .get_cell(object_id)
                .is_some_and(crate::interpreter::object_arena::ObjectHandle::is_young)
        {
            if owner.mark_element_card(index) {
                owner.remember_elements_if_old();
            } else {
                owner.remember_all_if_old();
            }
        }
    }

    /// Barrier for storing an entry referencing `values` into a strong Map or
    /// Set `owner`, run before the store. Returns whether the entry
    /// references a nursery object while `owner` is old, in which case the
    /// owner is queued for the next minor collection and the caller must
    /// store through `KeyedEntries::upsert_marking(.., true)` so that only the
    /// slot's card is re-scanned.
    pub(crate) fn gc_write_barrier_entry(
        &self,
        owner: &crate::interpreter::object_arena::ObjectHandle,
        values: &[&JsValue],
    ) -> bool {
        if !owner.is_old() {
            return false;
        }
        let young = values.iter().any(|value| {
            value.as_object_id().is_some_and(|id| {
                self.objects
                    .get_cell(id)
                    .is_some_and(crate::interpreter::object_arena::ObjectHandle::is_young)
            })
        });
        if young {
            owner.remember_elements_if_old();
        }
        young
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
        seen_envs: &mut EnvVisits,
        weak_maps: &mut Vec<u64>,
    ) {
        let mut children = Vec::new();
        while let Some(id) = worklist.pop() {
            let Some(obj_rc) = self.objects.get_cell(id) else {
                continue;
            };
            let obj = obj_rc.borrow();
            if obj.is_weak_map() {
                weak_maps.push(id);
            }
            children.clear();
            Self::trace_object_fields(&obj, &mut children, seen_envs);
            Self::enqueue_unmarked(children.drain(..), worklist, marks);
        }
    }

    pub(crate) fn gc_safepoint(&mut self) {
        // The clock starts inside the collecting arms: an unconditional
        // `Instant::now()` would run on every loop backedge (#537).
        match self.gc.begin_collection() {
            Some(CollectionKind::Minor) => {
                #[cfg(feature = "perf-counters")]
                let start = std::time::Instant::now();
                self.gc_collect_minor();
                self.gc.retune_nursery(self.env_registry.len());
                #[cfg(feature = "perf-counters")]
                {
                    let nanos = start.elapsed().as_nanos();
                    self.perf.gc_minor += 1;
                    self.perf.gc_nanos += nanos;
                    self.perf.gc_minor_nanos += nanos;
                }
            }
            Some(CollectionKind::Major) => {
                #[cfg(feature = "perf-counters")]
                let start = std::time::Instant::now();
                self.gc_collect_major();
                self.gc.retune_nursery(self.env_registry.len());
                #[cfg(feature = "perf-counters")]
                {
                    self.perf.gc_major += 1;
                    self.perf.gc_nanos += start.elapsed().as_nanos();
                }
            }
            None => {}
        }
    }

    fn collect_gc_roots(&mut self, minor: bool) -> (Vec<u64>, EnvVisits) {
        let mut roots = Vec::new();
        let mut seen_envs = EnvVisits::new(minor);

        for realm in &self.realms {
            realm.collect_roots(&mut roots, &mut seen_envs);
        }
        if let Some(id) = self.new_target.as_ref().and_then(JsValue::as_object_id) {
            roots.push(id);
        }
        self.collect_module_roots(&mut roots, &mut seen_envs);
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
                CallFrameArguments::Deferred { args, .. } => {
                    for argument in args.values() {
                        Self::collect_value_roots(argument, &mut roots);
                    }
                }
            }
        }
        roots.extend_from_slice(self.gc_temp_roots.as_slice());
        roots.extend(self.kept_alive.iter().copied());
        // Active bytecode operand stacks
        roots.extend_from_slice(self.gc_bytecode_roots.as_slice());
        // Microtasks, async-generator requests and timers.
        self.scheduler
            .for_each_root(|val| Self::collect_value_roots(val, &mut roots));
        // Promises a host worker settles later, reachable only through its
        // resolving function.
        roots.extend(self.scheduler.pending_async_promise_ids_lock().iter());
        for val in &self.pending_iter_close {
            Self::collect_value_roots(val, &mut roots);
        }
        self.collect_node_throw_roots(&mut roots);
        for iters in self.generator_inline_iters.values() {
            for val in iters {
                Self::collect_value_roots(val, &mut roots);
            }
        }
        for for_of_stack in self.generator_for_of_stacks.values() {
            Self::collect_for_of_stack_roots(for_of_stack, &mut roots, &mut seen_envs);
        }
        for scope_stack in self.generator_scope_stacks.values() {
            Self::collect_scope_stack_roots(scope_stack, &mut roots, &mut seen_envs);
        }
        for (generator_id, disposal) in &self.generator_pending_dispose {
            roots.push(*generator_id);
            disposal.for_each_value(|v| Self::collect_value_roots(v, &mut roots));
        }
        for val in self.iterator_next_cache.values() {
            Self::collect_value_roots(val, &mut roots);
        }
        if let Some((value, _)) = &self.raw_iter_step {
            Self::collect_value_roots(value, &mut roots);
        }
        for disposal in self.scheduler.iter_async_disposals() {
            disposal.for_each_value(|v| Self::collect_value_roots(v, &mut roots));
        }
        for afs in self.scheduler.iter_async_function_states() {
            Self::collect_env_roots(&afs.func_env, &mut roots, &mut seen_envs);
            afs.resolvers
                .for_each_value(|v| Self::collect_value_roots(v, &mut roots));
            if let Some(ref pending) = afs.pending_dispose {
                pending
                    .cursor
                    .for_each_value(|v| Self::collect_value_roots(v, &mut roots));
            }
            for try_info in &afs.try_stack {
                match &try_info.pending_completion {
                    Some(PendingCompletion::Return(v) | PendingCompletion::Throw(v)) => {
                        Self::collect_value_roots(v, &mut roots);
                    }
                    Some(PendingCompletion::LoopControl(_)) | None => {}
                }
            }
            Self::collect_for_of_stack_roots(&afs.for_of_stack, &mut roots, &mut seen_envs);
            Self::collect_scope_stack_roots(&afs.scope_stack, &mut roots, &mut seen_envs);
        }
        self.collect_async_activation_roots(&mut roots, &mut seen_envs);

        // A minor collection's only view of old environments written since
        // the last collection; a major one scans everything.
        #[cfg(feature = "perf-counters")]
        let registry_start = std::time::Instant::now();
        #[cfg(feature = "perf-counters")]
        if minor {
            self.perf.gc_minor_profile.sizes[profile::REGISTRY_TOTAL] +=
                self.env_registry.len() as u64;
        }
        if minor {
            for env in self.env_registry.take_dirty() {
                let clean = env.try_borrow().is_ok_and(|env| env.is_clean());
                if !clean {
                    Self::collect_env_roots(&env, &mut roots, &mut seen_envs);
                }
            }
        } else {
            self.env_registry.begin_major();
        }
        #[cfg(feature = "perf-counters")]
        if minor {
            self.perf
                .gc_minor_profile
                .lap(profile::REGISTRY, registry_start);
        }

        (roots, seen_envs)
    }

    /// Mark and queue every nursery object among `candidates`. Returns
    /// whether there was one. The nursery bitmap answers "is it young?"
    /// without loading the referent, which for an old child (every object's
    /// prototype, say) would be a wasted cache line.
    fn enqueue_young<I>(&self, candidates: I, worklist: &mut Vec<u64>, young: &YoungIds) -> bool
    where
        I: IntoIterator<Item = u64>,
    {
        let mut any_young = false;
        for id in candidates {
            if young.contains(id) {
                any_young = true;
                if let Some(handle) = self.objects.get_cell(id)
                    && handle.mark_young()
                {
                    worklist.push(id);
                }
            } else {
                debug_assert!(
                    self.objects
                        .get_cell(id)
                        .is_none_or(|handle| !handle.is_young()),
                    "young object {id} missing from the nursery list"
                );
            }
        }
        any_young
    }

    fn trace_young_worklist(
        &self,
        worklist: &mut Vec<u64>,
        seen_envs: &mut EnvVisits,
        weak_containers: &mut Vec<u64>,
        young: &mut YoungIds,
    ) {
        let mut children = Vec::new();
        while let Some(id) = worklist.pop() {
            let Some(handle) = self.objects.get_cell(id) else {
                continue;
            };
            let obj = handle.borrow_untracked();
            if obj.is_weak_container() {
                weak_containers.push(id);
            }
            children.clear();
            Self::trace_object_fields(&obj, &mut children, seen_envs);
            Self::trace_weak_slots_strongly(&obj, &mut children);
            drop(obj);
            if self.enqueue_young(children.drain(..), worklist, young) {
                young.note_young_edge(id);
            }
        }
    }

    fn object_requires_persistent_minor_scan(obj: &JsObjectData) -> bool {
        matches!(
            obj.kind,
            ObjectKind::Iterator(_) | ObjectKind::Arguments(_) | ObjectKind::ModuleNamespace(_)
        )
    }

    /// Kinds whose edges into the nursery the tracer cannot see, so a promoted
    /// object of one must always be re-scanned.
    fn has_untraceable_young_edges(
        handle: &crate::interpreter::object_arena::ObjectHandle,
    ) -> bool {
        let obj = handle.borrow_untracked();
        obj.is_weak_container() || Self::object_requires_persistent_minor_scan(&obj)
    }

    fn object_has_young_weak_edge(&self, obj: &JsObjectData) -> bool {
        if obj.is_weak_map() {
            obj.map_data().is_some_and(|entries| {
                entries.iter().any(|(key, value)| {
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
        } else if obj.is_weak_set() {
            obj.set_data().is_some_and(|entries| {
                entries.iter().any(|value| {
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

    /// Re-scan the bulk storage of a remembered object that has card tracking
    /// (an Array's elements, a strong Map's or Set's entries): its dirty
    /// cards, or all of them while the bitmap is absent. Marks every nursery
    /// object found and reports whether any of it still references one
    /// (for storage too small to track by card), plus whether the storage is
    /// large enough to track by card, in which case `retained_cards` lists
    /// the cards that must stay dirty.
    fn scan_remembered_bulk(
        &self,
        obj: &JsObjectData,
        young: &YoungIds,
        worklist: &mut Vec<u64>,
        retained_cards: &mut Vec<usize>,
    ) -> (bool, bool) {
        use crate::interpreter::types::ObjectKind;
        match &obj.kind {
            ObjectKind::Array(_) => {
                let Some((elements, cards)) = obj.array_elements_and_cards() else {
                    return (false, false);
                };
                self.scan_cards(elements.len(), cards, retained_cards, |start, end| {
                    self.enqueue_young(object_ids(&elements[start..end]), worklist, young)
                })
            }
            ObjectKind::Map(entries) if !obj.is_weak_map() => self.scan_cards(
                entries.slot_count(),
                entries.dirty_card_bits(),
                retained_cards,
                |start, end| {
                    let mut any = false;
                    for (key, value) in entries.live_in_slots(start, end) {
                        any |= self.enqueue_young(
                            key.as_object_id().into_iter().chain(value.as_object_id()),
                            worklist,
                            young,
                        );
                    }
                    any
                },
            ),
            ObjectKind::Set(entries) if !obj.is_weak_set() => self.scan_cards(
                entries.slot_count(),
                entries.dirty_card_bits(),
                retained_cards,
                |start, end| {
                    self.enqueue_young(
                        object_ids(entries.live_in_slots(start, end)),
                        worklist,
                        young,
                    )
                },
            ),
            _ => (false, false),
        }
    }

    /// Run `scan_range(start, end)` over the cards of a `len`-slot store
    /// that are dirty (all of them for `cards == None`); it reports whether
    /// the range still holds a nursery reference.
    fn scan_cards(
        &self,
        len: usize,
        cards: Option<&[u64]>,
        retained_cards: &mut Vec<usize>,
        mut scan_range: impl FnMut(usize, usize) -> bool,
    ) -> (bool, bool) {
        use crate::interpreter::types::CARD_ELEMENTS;
        if len <= CARD_ELEMENTS {
            return (scan_range(0, len), false);
        }
        let card_count = len.div_ceil(CARD_ELEMENTS);
        let mut scan_card = |card: usize| {
            let start = card * CARD_ELEMENTS;
            let end = (start + CARD_ELEMENTS).min(len);
            if scan_range(start, end) {
                retained_cards.push(card);
            }
        };
        match cards {
            None => (0..card_count).for_each(&mut scan_card),
            Some(bits) => {
                for (word_idx, &word) in bits.iter().enumerate() {
                    let mut word = word;
                    while word != 0 {
                        let card = word_idx * 64 + word.trailing_zeros() as usize;
                        word &= word - 1;
                        if card < card_count {
                            scan_card(card);
                        }
                    }
                }
            }
        }
        (false, true)
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

        #[cfg(feature = "perf-counters")]
        let mut phase_clock = std::time::Instant::now();
        #[cfg(feature = "perf-counters")]
        let registry_before = self.perf.gc_minor_profile.phase_nanos[profile::REGISTRY];
        let (roots, mut seen_envs) = self.collect_gc_roots(true);
        #[cfg(feature = "perf-counters")]
        {
            phase_clock = self
                .perf
                .gc_minor_profile
                .lap_roots(phase_clock, registry_before);
        }
        let remembered = self.objects.take_remembered();
        #[cfg(feature = "perf-counters")]
        {
            self.perf.gc_minor_profile.sizes[profile::REMEMBERED_TOTAL] += remembered.len() as u64;
            self.perf.gc_minor_profile.sizes[profile::NURSERY_TOTAL] +=
                self.objects.nursery_ids().len() as u64;
        }
        let mut young = std::mem::take(&mut self.gc_young);
        young.fill(self.objects.nursery_ids(), self.objects.capacity());
        let mut worklist = Vec::new();
        let mut weak_containers = Vec::new();
        let mut children = Vec::new();
        let mut retained_cards = Vec::new();

        // Old roots are already known to survive a minor collection. Only
        // young roots and old objects admitted by the write barrier need work.
        self.enqueue_young(roots, &mut worklist, &young);
        for (id, state) in remembered {
            let Some(handle) = self.objects.get_cell(id) else {
                continue;
            };
            if !handle.is_old() {
                continue;
            }
            if state.all_elements {
                handle.borrow_mut_untracked().mark_all_cards_dirty();
            }
            let obj = handle.borrow_untracked();
            // Everything but a card-tracked object's dirty cards is scanned
            // unless the barrier knew only a card changed.
            let scan_fields = state.fields || !obj.has_card_tracking();
            if obj.is_weak_container() {
                weak_containers.push(id);
            }
            children.clear();
            let mut retain = false;
            if scan_fields {
                Self::trace_object_fields_in(&obj, &mut children, &mut seen_envs, false);
                Self::trace_weak_slots_strongly(&obj, &mut children);
                retain = Self::object_requires_persistent_minor_scan(&obj)
                    || self.object_has_young_weak_edge(&obj);
            }
            retained_cards.clear();
            let (elements_young, card_mode) =
                self.scan_remembered_bulk(&obj, &young, &mut worklist, &mut retained_cards);
            drop(obj);
            retain |= self.enqueue_young(children.drain(..), &mut worklist, &young);
            if retain {
                handle.remember_if_old();
            }
            if card_mode {
                handle
                    .borrow_mut_untracked()
                    .set_dirty_cards(&retained_cards);
            }
            if elements_young || !retained_cards.is_empty() {
                handle.remember_elements_if_old();
            }
        }
        #[cfg(feature = "perf-counters")]
        {
            phase_clock = self
                .perf
                .gc_minor_profile
                .lap(profile::REMEMBERED, phase_clock);
        }
        self.trace_young_worklist(
            &mut worklist,
            &mut seen_envs,
            &mut weak_containers,
            &mut young,
        );

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
                if !obj.is_weak_map() {
                    continue;
                }
                if let Some(entries) = obj.map_data() {
                    for entry in entries.iter() {
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
            self.trace_young_worklist(
                &mut worklist,
                &mut seen_envs,
                &mut weak_containers,
                &mut young,
            );
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
            if obj.is_weak_map() {
                if let Some(entries) = obj.map_data_mut() {
                    entries.retain(|(key, _)| {
                        key.as_object_id()
                            .is_none_or(|id| self.minor_value_is_live(id))
                    });
                }
            } else if obj.is_weak_set()
                && let Some(entries) = obj.set_data_mut()
            {
                entries.retain(|value| {
                    value
                        .as_object_id()
                        .is_none_or(|id| self.minor_value_is_live(id))
                });
            }
        }

        // Walks the whole heap, so only while it is small enough for that to
        // be cheap next to the collection itself.
        #[cfg(debug_assertions)]
        if self.objects.live_count() <= 100_000 {
            self.debug_assert_old_arrays_reach_only_marked_young();
            self.debug_assert_old_closure_envs_are_tracked(seen_envs.epoch());
        }

        #[cfg(feature = "perf-counters")]
        {
            phase_clock = self.perf.gc_minor_profile.lap(profile::TRACE, phase_clock);
        }
        let nursery = self.objects.take_nursery();
        let mut survivors = Vec::with_capacity(nursery.len());
        for &id in &nursery {
            let Some(handle) = self.objects.get(id) else {
                continue;
            };
            if !handle.is_young_marked() {
                self.free_gc_object(id);
            } else if handle.age_survivor(GC_PROMOTION_AGE) {
                handle.promote();
                if young.has_young_edge(id) || Self::has_untraceable_young_edges(&handle) {
                    handle.remember_if_old();
                }
            } else {
                handle.clear_young_mark();
                survivors.push(id);
            }
        }
        young.clear(&nursery);
        self.gc_young = young;
        #[cfg(feature = "perf-counters")]
        {
            phase_clock = self.perf.gc_minor_profile.lap(profile::SWEEP, phase_clock);
            self.perf.gc_minor_profile.sizes[profile::SURVIVORS_TOTAL] += survivors.len() as u64;
            self.perf.gc_minor_profile.sizes[profile::SCANNED_ENVS_TOTAL] +=
                seen_envs.scanned.len() as u64;
        }
        self.objects.replace_nursery(survivors);
        self.settle_scanned_envs(&mut seen_envs);
        #[cfg(feature = "perf-counters")]
        {
            self.perf.gc_minor_profile.lap(profile::SETTLE, phase_clock);
        }
        self.gc.end_minor_collection();
        #[cfg(feature = "perf-counters")]
        self.perf.note_env_visits(&seen_envs);
    }

    /// Barrier check for closure environments, run once marking is complete:
    /// every environment an old closure reaches must have been scanned by this
    /// collection or be clean, and a clean one may reference no nursery object.
    /// A store that skipped `BindingMap`'s mutators, or a scan that left a
    /// young reference behind a clean mark, fails here.
    #[cfg(debug_assertions)]
    fn debug_assert_old_closure_envs_are_tracked(&self, epoch: u64) {
        let mut checked = HashSet::new();
        for id in 0..self.objects.capacity() {
            let Some(handle) = self.objects.get_cell(id) else {
                continue;
            };
            if !handle.is_old() {
                continue;
            }
            let obj = handle.borrow_untracked();
            let Some(JsFunction::User { closure, .. }) = obj.callable.as_deref() else {
                continue;
            };
            let mut current = Some(closure.clone());
            while let Some(env) = current {
                if !checked.insert(Rc::as_ptr(&env) as usize) {
                    break;
                }
                let borrowed = env.borrow();
                if borrowed.is_clean() {
                    borrowed.for_each_root_id(|child| {
                        assert!(
                            self.objects
                                .get_cell(child)
                                .is_none_or(|child_handle| !child_handle.is_young()),
                            "clean environment of old closure {id} references nursery object \
                             {child}: a write skipped the environment barrier"
                        );
                    });
                } else {
                    assert_eq!(
                        borrowed.gc_epoch(),
                        epoch,
                        "environment of old closure {id} is neither clean nor scanned by this \
                         collection"
                    );
                }
                current = borrowed.parent.clone();
            }
        }
    }

    /// Barrier check for the minor collector, run once marking is complete: a
    /// nursery object that an old Array's element (or strong Map or Set
    /// entry) still references but that
    /// nothing marked is about to be freed under it. That means a store
    /// reached the elements without remembering their card.
    #[cfg(debug_assertions)]
    fn debug_assert_old_arrays_reach_only_marked_young(&self) {
        use crate::interpreter::types::ObjectKind;
        let check = |id: u64, what: &str, index: usize, value: &JsValue| {
            if let Some(child) = value.as_object_id()
                && let Some(child_handle) = self.objects.get_cell(child)
            {
                assert!(
                    !child_handle.is_young() || child_handle.is_young_marked(),
                    "old {what} {id} entry {index} references unmarked nursery object \
                     {child}: a store skipped the write barrier"
                );
            }
        };
        for id in 0..self.objects.capacity() {
            let Some(handle) = self.objects.get_cell(id) else {
                continue;
            };
            if !handle.is_old() {
                continue;
            }
            let obj = handle.borrow_untracked();
            if let Some(elements) = obj.array_elements() {
                for (index, value) in elements.iter().enumerate() {
                    check(id, "Array", index, value);
                }
                continue;
            }
            if !obj.has_card_tracking() {
                continue;
            }
            match &obj.kind {
                ObjectKind::Map(entries) => {
                    for (index, (key, value)) in entries.iter().enumerate() {
                        check(id, "Map", index, key);
                        check(id, "Map", index, value);
                    }
                }
                ObjectKind::Set(entries) => {
                    for (index, value) in entries.iter().enumerate() {
                        check(id, "Set", index, value);
                    }
                }
                _ => {}
            }
        }
    }

    fn free_gc_object(&mut self, id: u64) {
        // `ObjectArena` recycles logical ids (its `free_list: Vec<u64>`), so a
        // cached label keyed by id must die with the object or the next
        // function to receive that id inherits this one's name — merging its
        // work and bail reason into the wrong BODY row (#537 review).
        #[cfg(feature = "perf-counters")]
        self.perf_name_cache.remove(&id);
        if let Some(handle) = self.objects.get_cell(id) {
            let obj = handle.borrow_untracked();
            if let Some(buf_data) = obj.arraybuffer_data()
                && let BufferData::Owned(ref data) = *buf_data.borrow()
            {
                self.gc.release_external(data.len());
            }
        }
        self.objects.free(id);
        if !self.iterator_next_cache.is_empty() {
            self.iterator_next_cache.remove(&id);
        }
        if !self.generator_inline_iters.is_empty() {
            self.generator_inline_iters.remove(&id);
        }
        if !self.generator_for_of_stacks.is_empty() {
            self.generator_for_of_stacks.remove(&id);
        }
        if !self.generator_scope_stacks.is_empty() {
            self.generator_scope_stacks.remove(&id);
        }
        self.scheduler.remove_async_gen_queue(id);
        if !self.generator_pending_dispose.is_empty() {
            self.generator_pending_dispose.remove(&id);
        }
    }

    fn gc_collect_major(&mut self) {
        let obj_count = self.objects.capacity() as usize;
        // Reuse the mark bitmap buffer across collections to avoid per-GC
        // allocation churn. clear()+resize(_, false) yields an all-false buffer
        // while keeping the backing capacity.
        let mut marks = std::mem::take(&mut self.gc_marks);
        marks.clear();
        marks.resize(obj_count, false);

        let (roots, mut seen_envs) = self.collect_gc_roots(false);

        // Mark each object when it is enqueued, not when it is popped. Shared
        // environments and prototypes can expose the same object through many
        // edges; admitting each id once keeps the worklist linear in the live
        // object graph instead of filling it with duplicate entries.
        let mut worklist = Vec::with_capacity(roots.len());
        let mut weak_maps = Vec::new();
        Self::enqueue_unmarked(roots, &mut worklist, &mut marks);
        self.trace_mark_worklist(&mut worklist, &mut marks, &mut seen_envs, &mut weak_maps);

        // Ephemeron fixpoint over the WeakMaps the mark phase reached.
        while !weak_maps.is_empty() {
            let mut new_marks = false;
            let scanned = weak_maps.len();
            for &weak_map_id in &weak_maps {
                let Some(obj_rc) = self.objects.get_cell(weak_map_id) else {
                    continue;
                };
                let obj = obj_rc.borrow_untracked();
                if let Some(entries) = obj.map_data() {
                    for entry in entries.iter() {
                        // Key is reachable: mark the value
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
            // Tracing can reach further WeakMaps, which the next round scans.
            self.trace_mark_worklist(&mut worklist, &mut marks, &mut seen_envs, &mut weak_maps);
            if !new_marks && scanned == weak_maps.len() {
                break;
            }
        }

        // Before the sweep, so its `remember_if_old` calls are kept.
        self.objects.reset_generations_after_major();

        // One pass: free the unmarked; tenure survivors, drop their dead weak
        // entries and keep environment owners visible to minor collections.
        let mut cleanup_registries = Vec::new();
        for i in 0..obj_count {
            let id = i as u64;
            let Some(obj_rc) = self.objects.get(id) else {
                continue;
            };
            if !marks[i] {
                drop(obj_rc);
                self.free_gc_object(id);
                continue;
            }
            obj_rc.tenure_after_major();
            obj_rc.reset_cards_after_major();
            let (weak_slots, weak_container) = {
                let obj = obj_rc.borrow_untracked();
                (obj.has_weak_slots(), obj.is_weak_container())
            };
            if weak_slots && Self::clear_dead_weak_slots(&mut obj_rc.borrow_mut_untracked(), &marks)
            {
                cleanup_registries.push(id);
            }
            if weak_container {
                let mut obj = obj_rc.borrow_mut_untracked();
                if obj.is_weak_map() {
                    if let Some(entries) = obj.map_data_mut() {
                        entries.retain(|(key, _)| match key.as_object_id() {
                            Some(kid) => (kid as usize) < obj_count && marks[kid as usize],
                            None => true,
                        });
                    }
                } else if let Some(entries) = obj.set_data_mut() {
                    entries.retain(|value| match value.as_object_id() {
                        Some(vid) => (vid as usize) < obj_count && marks[vid as usize],
                        None => true,
                    });
                }
            }
            let requires_persistent_scan =
                Self::object_requires_persistent_minor_scan(&obj_rc.borrow_untracked());
            if requires_persistent_scan {
                obj_rc.remember_if_old();
            }
        }
        for registry_id in cleanup_registries {
            self.enqueue_finalization_cleanup(registry_id);
        }
        let live_count = self.objects.live_count();
        self.gc.end_major_collection(live_count);
        crate::thp::after_major_collection();
        // Return the buffer to the interpreter so its capacity is reused next GC.
        self.gc_marks = marks;
        // Everything that survived is old now: every environment walked is clean.
        for env in std::mem::take(&mut seen_envs.scanned) {
            self.env_registry.register(&env);
            env.borrow().mark_clean();
        }
        self.env_registry.end_major();
        #[cfg(feature = "perf-counters")]
        self.perf.note_env_visits(&seen_envs);
    }

    fn collect_value_roots(val: &JsValue, worklist: &mut Vec<u64>) {
        if let Some(id) = val.as_object_id() {
            worklist.push(id);
        }
    }

    fn trace_object_fields(obj: &JsObjectData, worklist: &mut Vec<u64>, seen_envs: &mut EnvVisits) {
        Self::trace_object_fields_in(obj, worklist, seen_envs, true);
    }

    /// `trace_object_fields`, optionally leaving out an Array's element
    /// vector or a strong Map's or Set's entries, which a minor collection
    /// scans card by card instead.
    fn trace_object_fields_in(
        obj: &JsObjectData,
        worklist: &mut Vec<u64>,
        seen_envs: &mut EnvVisits,
        bulk: bool,
    ) {
        if let Some(pid) = obj.proto_id() {
            worklist.push(pid);
        }
        for desc in obj.properties.values() {
            if let Some(v) = desc.value() {
                Self::collect_value_roots(v, worklist);
            }
            if let Some(v) = desc.getter() {
                Self::collect_value_roots(v, worklist);
            }
            if let Some(v) = desc.setter() {
                Self::collect_value_roots(v, worklist);
            }
        }
        if bulk && let Some(elems) = obj.array_elements() {
            for v in elems {
                Self::collect_value_roots(v, worklist);
            }
        }
        if let Some(v) = obj.primitive_value() {
            Self::collect_value_roots(v, worklist);
        }
        for elem in obj.private_field_values() {
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
        for idef in obj.instance_field_defs() {
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
        if let Some(JsFunction::User { closure, .. }) = obj.callable.as_deref() {
            Self::collect_env_roots(closure, worklist, seen_envs);
        }
        for v in obj.gc_native_roots() {
            Self::collect_value_roots(v, worklist);
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
            | ObjectKind::Temporal(_)
            | ObjectKind::Intl(_)
            | ObjectKind::PrimitiveWrapper(_) => {}
            ObjectKind::DisposableStack(d) => {
                for resource in &d.stack {
                    Self::collect_value_roots(&resource.value, worklist);
                    Self::collect_value_roots(&resource.dispose_method, worklist);
                }
            }
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
            ObjectKind::IterHelper(h) => match &**h {
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
                for (resolve, reject) in &pd.resolving_functions {
                    Self::collect_value_roots(resolve, worklist);
                    Self::collect_value_roots(reject, worklist);
                }
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
                    reaction
                        .target
                        .for_each_value(|v| Self::collect_value_roots(v, worklist));
                    if let Some(pid) = reaction.target.promise_id() {
                        worklist.push(pid);
                    }
                }
            }
            ObjectKind::Map(entries) => {
                // WeakMap entries are visited via the ephemeron fixpoint, not strongly.
                if bulk && !obj.is_weak_map() {
                    for entry in entries.iter() {
                        Self::collect_value_roots(&entry.0, worklist);
                        Self::collect_value_roots(&entry.1, worklist);
                    }
                }
            }
            ObjectKind::Set(entries) => {
                if bulk && !obj.is_weak_set() {
                    for val in entries.iter() {
                        Self::collect_value_roots(val, worklist);
                    }
                }
            }
            ObjectKind::FinalizationRegistry(data) => {
                // Held values are strong; targets and tokens are weak slots,
                // see `trace_weak_slots_strongly` and `clear_dead_weak_slots`.
                for entry in data.cells.iter().flatten() {
                    Self::collect_value_roots(&entry.1, worklist);
                }
            }
            ObjectKind::WeakRef(_) => {}
            ObjectKind::Iterator(state) => {
                Self::collect_iterator_state_roots(state, worklist, seen_envs);
            }
            ObjectKind::Arguments(map) => {
                Self::collect_env_roots(map.env(), worklist, seen_envs);
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
        seen_envs: &mut EnvVisits,
    ) {
        match state {
            IteratorState::ArrayIterator { array_id, .. } => worklist.push(*array_id),
            IteratorState::TypedArrayIterator { typed_array_id, .. } => {
                worklist.push(*typed_array_id)
            }
            IteratorState::ForInEnumerator { obj_id, .. } => worklist.extend(*obj_id),
            IteratorState::MapIterator { map_id, .. } => worklist.push(*map_id),
            IteratorState::SetIterator { set_id, .. } => worklist.push(*set_id),
            IteratorState::RegExpStringIterator { matcher_id, .. } => worklist.push(*matcher_id),
            IteratorState::Generator {
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
            IteratorState::BytecodeGenerator(generator) => {
                Self::collect_env_roots(&generator.code.func_env, worklist, seen_envs);
                if let Some(frame) = &generator.frame {
                    for value in &frame.stack {
                        Self::collect_value_roots(value, worklist);
                    }
                    for env in frame.environments() {
                        Self::collect_env_roots(env, worklist, seen_envs);
                    }
                    worklist.extend(frame.with_objects());
                }
            }
            IteratorState::StateMachineGenerator {
                func_env,
                delegated_iterator,
                pending_exception,
                pending_return,
                try_stack,
                _sent_value,
                ..
            }
            | IteratorState::StateMachineAsyncGenerator {
                func_env,
                delegated_iterator,
                pending_exception,
                pending_return,
                try_stack,
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
                for try_info in try_stack {
                    match &try_info.pending_completion {
                        Some(PendingCompletion::Return(v) | PendingCompletion::Throw(v)) => {
                            Self::collect_value_roots(v, worklist);
                        }
                        Some(PendingCompletion::LoopControl(_)) | None => {}
                    }
                }
            }
            _ => {}
        }
    }

    /// Roots the environments a suspended driver's for-of stack still holds.
    /// Shared by the async-function states and the generator side table so a
    /// new field on `ForOfLoopState` cannot be traced in one and missed in the
    /// other.
    fn collect_for_of_stack_roots(
        for_of_stack: &[ForOfLoopState],
        worklist: &mut Vec<u64>,
        seen_envs: &mut EnvVisits,
    ) {
        for loop_state in for_of_stack {
            Self::collect_env_roots(&loop_state.outer_env, worklist, seen_envs);
            if let Some(ref env) = loop_state.iteration_env {
                Self::collect_env_roots(env, worklist, seen_envs);
            }
        }
    }

    /// Roots the environments a suspended driver's lexical scope stack still
    /// holds (`AsyncFunctionState::scope_stack`, and the generator side
    /// table's equivalent) — the same pattern as `collect_for_of_stack_roots`,
    /// generalized alongside it.
    fn collect_scope_stack_roots(
        scope_stack: &[ScopeFrame],
        worklist: &mut Vec<u64>,
        seen_envs: &mut EnvVisits,
    ) {
        for frame in scope_stack {
            Self::collect_env_roots(&frame.env, worklist, seen_envs);
        }
    }

    /// Roots the values `env` and its parents reference. A minor collection
    /// leaves a clean environment's bindings out: it holds no nursery object
    /// (`BindingMap::is_clean`), so every object it references is old and survives anyway.
    pub(crate) fn collect_env_roots(
        env: &EnvRef,
        worklist: &mut Vec<u64>,
        seen_envs: &mut EnvVisits,
    ) {
        let mut current = Some(env.clone());
        while let Some(e) = current {
            let borrowed = e.borrow();
            if !seen_envs.first_visit(&borrowed) {
                break;
            }
            let parent = borrowed.parent.clone();
            if seen_envs.minor && borrowed.is_clean() {
                debug_assert!(
                    borrowed.is_registered(),
                    "a clean environment is registered"
                );
                #[cfg(feature = "perf-counters")]
                {
                    seen_envs.clean_skips += 1;
                }
            } else {
                borrowed.for_each_root_id(|id| worklist.push(id));
                drop(borrowed);
                seen_envs.scanned.push(e);
            }
            current = parent;
        }
    }

    /// Whether `env` still references an object of the nursery.
    fn env_references_nursery(&self, env: &EnvRef) -> bool {
        env.borrow().any_root_id(|id| {
            self.objects
                .get_cell(id)
                .is_some_and(|handle| handle.is_young())
        })
    }

    /// After a collection: an environment it scanned is clean unless it still
    /// holds a nursery object, in which case the next minor collection scans it
    /// again.
    fn settle_scanned_envs(&mut self, seen_envs: &mut EnvVisits) {
        for env in std::mem::take(&mut seen_envs.scanned) {
            self.env_registry.register(&env);
            if self.env_references_nursery(&env) {
                env.borrow().bindings.rearm();
            } else {
                env.borrow().mark_clean();
            }
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
        data.set_proto(Some(7));
        data.properties.insert(
            "x".into(),
            PropertyDescriptor::data(obj(8), true, true, true),
        );
        data.properties.insert(
            "n".into(),
            PropertyDescriptor::data(JsValue::number(1.0), true, true, true),
        );

        let mut worklist = Vec::new();
        Interpreter::trace_object_fields(&data, &mut worklist, &mut EnvVisits::new(false));
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
        Interpreter::trace_object_fields(&data, &mut worklist, &mut EnvVisits::new(false));
        assert_eq!(as_set(worklist), vec![10, 11]);
    }

    #[test]
    fn trace_object_fields_roots_array_elements_and_native_roots() {
        let mut data = JsObjectData::new();
        data.kind = ObjectKind::Array(ArrayData::new(vec![obj(20), JsValue::number(0.0), obj(21)]));
        data.gc_native_roots_mut().push(obj(22));

        let mut worklist = Vec::new();
        Interpreter::trace_object_fields(&data, &mut worklist, &mut EnvVisits::new(false));
        assert_eq!(as_set(worklist), vec![20, 21, 22]);
    }

    #[test]
    fn trace_object_fields_roots_disposable_stack_resources() {
        let mut data = JsObjectData::new();
        data.kind = ObjectKind::DisposableStack(Box::new(DisposableStackData {
            stack: vec![DisposableResource {
                value: obj(30),
                hint: DisposeHint::Sync,
                dispose_method: obj(31),
            }],
            disposed: false,
        }));

        let mut worklist = Vec::new();
        Interpreter::trace_object_fields(&data, &mut worklist, &mut EnvVisits::new(false));
        assert_eq!(as_set(worklist), vec![30, 31]);
    }

    #[test]
    fn collect_env_roots_walks_parent_chain_and_terminates_on_cycle() {
        // (a) child binds "a"=Object(30), parent binds "b"=Object(31) → {30,31}
        let parent = Environment::new(None);
        parent
            .borrow_mut()
            .bindings
            .insert("b", Binding::new(obj(31), BindingKind::Var, true));
        let child = Environment::new(Some(parent.clone()));
        child
            .borrow_mut()
            .bindings
            .insert("a", Binding::new(obj(30), BindingKind::Var, true));

        let mut worklist = Vec::new();
        Interpreter::collect_env_roots(&child, &mut worklist, &mut EnvVisits::new(false));
        assert_eq!(as_set(worklist), vec![30, 31]);

        // (b) self-referential env (parent points to itself) binds "c"=Object(32)
        // → terminates without infinite loop, contains 32 exactly once.
        let cyclic = Environment::new(None);
        cyclic
            .borrow_mut()
            .bindings
            .insert("c", Binding::new(obj(32), BindingKind::Var, true));
        cyclic.borrow_mut().parent = Some(cyclic.clone());

        let mut worklist = Vec::new();
        Interpreter::collect_env_roots(&cyclic, &mut worklist, &mut EnvVisits::new(false));
        assert_eq!(worklist, vec![32]);
    }

    #[test]
    fn collect_env_roots_marks_the_this_binding() {
        let env = Environment::new(None);
        env.borrow_mut().set_this(obj(34));
        let child = Environment::new(Some(env));

        let mut worklist = Vec::new();
        Interpreter::collect_env_roots(&child, &mut worklist, &mut EnvVisits::new(false));
        assert_eq!(worklist, vec![34]);
    }

    #[test]
    fn collect_env_roots_marks_the_new_target() {
        let env = Environment::new(None);
        env.borrow_mut().bind_this_for_call(obj(34), obj(35));
        let child = Environment::new(Some(env));

        let mut worklist = Vec::new();
        Interpreter::collect_env_roots(&child, &mut worklist, &mut EnvVisits::new(false));
        worklist.sort_unstable();
        assert_eq!(worklist, vec![34, 35]);
    }

    #[test]
    fn environment_roots_are_scanned_once_per_collection() {
        let env = Environment::new(None);
        env.borrow_mut()
            .bindings
            .insert("shared", Binding::new(obj(33), BindingKind::Var, true));
        let mut worklist = Vec::new();
        let mut seen_envs = EnvVisits::new(false);

        Interpreter::collect_env_roots(&env, &mut worklist, &mut seen_envs);
        Interpreter::collect_env_roots(&env, &mut worklist, &mut seen_envs);

        assert_eq!(worklist, vec![33]);
    }

    #[test]
    fn a_new_collection_scans_an_environment_an_earlier_one_stamped() {
        let env = Environment::new(None);
        env.borrow_mut()
            .bindings
            .insert("held", Binding::new(obj(36), BindingKind::Var, true));

        for _ in 0..3 {
            let mut worklist = Vec::new();
            Interpreter::collect_env_roots(&env, &mut worklist, &mut EnvVisits::new(false));
            assert_eq!(worklist, vec![36]);
        }
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
        interp.gc_root_id(owner);
        interp.gc.request();
        interp.gc_safepoint();

        let young = interp.alloc_object(JsObjectData::new());
        let owner_handle = interp.objects.get(owner).unwrap();
        assert!(owner_handle.is_old());
        assert!(interp.objects.take_remembered_ids().is_empty());

        interp.gc_write_barrier_value(&owner_handle, &JsValue::number(1.0));
        interp.gc_write_barrier_value(&owner_handle, &obj(owner));
        assert!(interp.objects.take_remembered_ids().is_empty());

        interp.gc_write_barrier_value(&owner_handle, &obj(young));
        assert_eq!(interp.objects.take_remembered_ids(), vec![owner]);
    }

    #[test]
    fn explicit_major_collection_tenures_a_reachable_nursery_object() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let survivor = interp.alloc_object(JsObjectData::new());
        interp.gc_root_id(survivor);
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
        owner_data.kind = ObjectKind::Arguments(Box::new(
            ParameterMap::new(
                env.clone(),
                Rc::from([Atom::intern("captured")]),
                &[true],
                1,
            )
            .expect("one mapped parameter"),
        ));
        let owner = interp.alloc_object(owner_data);
        interp.gc_root_id(owner);

        interp.gc.request();
        interp.gc_safepoint();
        assert!(interp.objects.get_cell_expect(owner).is_old());

        let child = interp.alloc_object(JsObjectData::new());
        env.borrow_mut()
            .bindings
            .insert("captured", Binding::new(obj(child), BindingKind::Var, true));
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

    fn enqueue_request(interp: &mut Interpreter, gen_id: u64, promise: u64) {
        interp
            .scheduler
            .async_gen_queue_or_default(gen_id)
            .push_back(crate::interpreter::AsyncGenRequest {
                kind: crate::interpreter::AsyncGenRequestKind::Next,
                value: JsValue::UNDEFINED,
                promise: obj(promise),
            });
    }

    #[test]
    fn pending_async_generator_request_keeps_generator_and_promise_alive() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let generator = interp.alloc_object(JsObjectData::new());
        let promise = interp.alloc_object(JsObjectData::new());
        enqueue_request(&mut interp, generator, promise);

        interp.gc.request();
        interp.gc_safepoint();

        assert!(interp.objects.get_cell(generator).is_some());
        assert!(interp.objects.get_cell(promise).is_some());
    }

    #[test]
    fn freed_async_generator_drops_its_request_queue() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let generator = interp.alloc_object(JsObjectData::new());
        interp.scheduler.async_gen_queue_or_default(generator);

        interp.gc.request();
        interp.gc_safepoint();

        assert!(interp.objects.get_cell(generator).is_none());
        assert!(interp.scheduler.async_gen_queue(generator).is_none());
    }

    #[test]
    fn remembered_old_object_keeps_young_child_alive() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let parent = interp.alloc_object(JsObjectData::new());
        interp.gc_root_id(parent);
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
        interp.gc_root_id(survivor);

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
        interp.gc_root_id(parent);

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
        weak_map_data.set_class("WeakMap");
        weak_map_data.kind = ObjectKind::Map(Box::default());
        let weak_map = interp.alloc_object(weak_map_data);
        interp.gc_root_id(weak_map);
        interp.gc.request();
        interp.gc_safepoint();

        let key = interp.alloc_object(JsObjectData::new());
        let value = interp.alloc_object(JsObjectData::new());
        interp.gc_root_id(key);
        interp
            .objects
            .get_cell_expect(weak_map)
            .borrow_mut()
            .map_data_mut()
            .unwrap()
            .upsert((obj(key), obj(value)), |_, _| {});

        interp.gc.request_minor();
        interp.gc_safepoint();
        assert!(interp.objects.get_cell(value).is_some());

        interp.gc_unroot_id(key);
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
                .unwrap()
                .live_len()
                == 0
        );
    }

    #[test]
    fn old_weakset_drops_an_unreachable_young_member() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);

        let mut weak_set_data = JsObjectData::new();
        weak_set_data.set_class("WeakSet");
        weak_set_data.kind = ObjectKind::Set(Box::default());
        let weak_set = interp.alloc_object(weak_set_data);
        interp.gc_root_id(weak_set);
        interp.gc.request();
        interp.gc_safepoint();

        let member = interp.alloc_object(JsObjectData::new());
        interp
            .objects
            .get_cell_expect(weak_set)
            .borrow_mut()
            .set_data_mut()
            .unwrap()
            .upsert(obj(member), |_, _| {});

        interp.gc.request_minor();
        interp.gc_safepoint();

        assert!(interp.objects.get_cell(member).is_none());
        assert!(
            interp
                .objects
                .get_cell_expect(weak_set)
                .borrow()
                .set_data()
                .unwrap()
                .live_len()
                == 0
        );
    }

    fn major_gc(interp: &mut Interpreter) {
        interp.gc.request();
        interp.gc_safepoint();
    }

    fn rooted_anchor(interp: &mut Interpreter) -> JsValue {
        let anchor = interp.alloc_object(JsObjectData::new());
        interp.gc_root_id(anchor);
        obj(anchor)
    }

    fn fresh_object(interp: &mut Interpreter) -> u64 {
        interp.alloc_object(JsObjectData::new())
    }

    fn slot_ids(slots: &RootedSlots, interp: &Interpreter) -> Vec<Option<u64>> {
        slots
            .snapshot(interp)
            .iter()
            .map(|v| v.as_object_id())
            .collect()
    }

    #[test]
    fn rooted_slots_values_survive_major_gc_when_only_the_container_is_pinned() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let anchor = rooted_anchor(&mut interp);
        let slots = RootedSlots::new(&mut interp, 2);
        slots.pin_on(&interp, &anchor);
        let first = fresh_object(&mut interp);
        let second = fresh_object(&mut interp);
        slots.set(&interp, 0, obj(first));
        slots.set(&interp, 1, obj(second));

        major_gc(&mut interp);

        assert!(interp.objects.get_cell(first).is_some());
        assert!(interp.objects.get_cell(second).is_some());
        assert_eq!(slots.get(&interp, 0).as_object_id(), Some(first));
        assert_eq!(slots.get(&interp, 1).as_object_id(), Some(second));
    }

    #[test]
    fn rooted_slots_set_releases_the_superseded_value() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let anchor = rooted_anchor(&mut interp);
        let slots = RootedSlots::new(&mut interp, 1);
        slots.pin_on(&interp, &anchor);
        let superseded = fresh_object(&mut interp);
        let current = fresh_object(&mut interp);
        slots.set(&interp, 0, obj(superseded));
        slots.set(&interp, 0, obj(current));

        major_gc(&mut interp);

        assert!(
            interp.objects.get_cell(superseded).is_none(),
            "a replaced slot value must not be retained"
        );
        assert!(interp.objects.get_cell(current).is_some());
    }

    #[test]
    fn rooted_slots_push_len_and_snapshot_round_trip_across_gc() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let anchor = rooted_anchor(&mut interp);
        let slots = RootedSlots::new(&mut interp, 0);
        slots.pin_on(&interp, &anchor);
        assert_eq!(slots.len(&interp), 0);

        let first = fresh_object(&mut interp);
        let second = fresh_object(&mut interp);
        slots.push(&interp, obj(first));
        slots.push(&interp, JsValue::UNDEFINED);
        slots.set(&interp, 1, obj(second));

        major_gc(&mut interp);

        let third = fresh_object(&mut interp);
        slots.push(&interp, obj(third));
        major_gc(&mut interp);

        assert_eq!(slots.len(&interp), 3);
        assert_eq!(
            slot_ids(&slots, &interp),
            vec![Some(first), Some(second), Some(third)]
        );
        for id in [first, second, third] {
            assert!(interp.objects.get_cell(id).is_some());
        }
    }

    #[test]
    fn rooted_slots_unpinned_container_is_collected_with_its_values() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let slots = RootedSlots::new(&mut interp, 1);
        let value = fresh_object(&mut interp);
        slots.set(&interp, 0, obj(value));
        let container = slots.root().as_object_id().unwrap();

        major_gc(&mut interp);

        assert!(interp.objects.get_cell(container).is_none());
        assert!(interp.objects.get_cell(value).is_none());
    }

    #[test]
    fn rooted_slots_write_into_an_old_container_is_remembered() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let anchor = rooted_anchor(&mut interp);
        let slots = RootedSlots::new(&mut interp, 1);
        slots.pin_on(&interp, &anchor);
        major_gc(&mut interp);
        let container = slots.root().as_object_id().unwrap();
        assert!(interp.objects.get_cell_expect(container).is_old());
        interp.objects.take_remembered_ids();

        let young = fresh_object(&mut interp);
        slots.set(&interp, 0, obj(young));

        assert_eq!(interp.objects.take_remembered_ids(), vec![container]);
    }

    #[test]
    fn rooted_slots_old_container_keeps_a_young_value_across_minor_gc() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let anchor = rooted_anchor(&mut interp);
        let slots = RootedSlots::new(&mut interp, 1);
        slots.pin_on(&interp, &anchor);
        major_gc(&mut interp);

        let young = fresh_object(&mut interp);
        slots.set(&interp, 0, obj(young));
        let pushed = fresh_object(&mut interp);
        slots.push(&interp, obj(pushed));
        interp.gc.request_minor();
        interp.gc_safepoint();

        assert!(interp.objects.get_cell(young).is_some());
        assert!(interp.objects.get_cell(pushed).is_some());
    }

    fn bind_in_global_env(interp: &mut Interpreter, name: &str, value: u64) {
        let env = interp.realm().global_env.clone();
        env.borrow_mut()
            .bindings
            .insert(name, Binding::new(obj(value), BindingKind::Var, true));
    }

    #[test]
    fn a_major_collection_marks_the_environments_it_walked_clean() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let env = interp.realm().global_env.clone();
        assert!(env.borrow().is_clean());

        let held = fresh_object(&mut interp);
        bind_in_global_env(&mut interp, "held", held);
        assert!(!env.borrow().is_clean());

        major_gc(&mut interp);
        assert!(env.borrow().is_clean());
        assert!(interp.objects.get_cell_expect(held).is_old());
    }

    #[test]
    fn a_young_value_stored_into_a_clean_environment_survives_minor_collections() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let env = interp.realm().global_env.clone();
        assert!(env.borrow().is_clean());

        let held = fresh_object(&mut interp);
        bind_in_global_env(&mut interp, "held", held);
        minor_gc(&mut interp);
        assert!(interp.objects.get_cell(held).is_some());
        assert!(
            !env.borrow().is_clean(),
            "the environment still references a nursery object"
        );

        minor_gc(&mut interp);
        assert!(interp.objects.get_cell_expect(held).is_old());
        assert!(
            env.borrow().is_clean(),
            "a scan that finds only old objects marks the environment clean"
        );

        let replacement = fresh_object(&mut interp);
        bind_in_global_env(&mut interp, "held", replacement);
        assert!(!env.borrow().is_clean());
        minor_gc(&mut interp);
        minor_gc(&mut interp);
        minor_gc(&mut interp);
        assert!(interp.objects.get_cell_expect(replacement).is_old());
        assert!(env.borrow().is_clean());
    }

    #[test]
    fn a_registered_dirty_environment_is_scanned_together_with_its_parents() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let held = fresh_object(&mut interp);
        let parent = Environment::new(Some(interp.realm().global_env.clone()));
        parent
            .borrow_mut()
            .bindings
            .insert("held", Binding::new(obj(held), BindingKind::Var, true));
        let child = Environment::new(Some(parent.clone()));
        child.borrow_mut().bindings.insert(
            "own",
            Binding::new(JsValue::number(1.0), BindingKind::Var, true),
        );
        interp.env_registry.register(&child);
        child.borrow().bindings.rearm();

        let (mut roots, mut seen_envs) = interp.collect_gc_roots(true);
        // A young closure over `child` reaches it again in the same collection.
        Interpreter::collect_env_roots(&child, &mut roots, &mut seen_envs);

        assert!(
            roots.contains(&held),
            "the unregistered parent's young value must be rooted"
        );
    }

    #[test]
    fn every_clean_environment_is_registered() {
        let mut interp = Interpreter::new();
        run_source(
            &mut interp,
            "var cell = (function () {
                 var held = { tag: 0 };
                 return function () { return held; };
             })();",
        );
        major_gc(&mut interp);
        let registered = interp.env_registry.registered();
        assert!(registered.iter().any(|env| env.borrow().is_clean()));
        assert!(registered.iter().all(|env| env.borrow().is_registered()));
    }

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
            "script threw"
        );
    }

    #[test]
    fn an_old_closure_is_not_rescanned_while_its_environment_is_clean() {
        let mut interp = Interpreter::new();
        run_source(
            &mut interp,
            "var cell = (function () {
                 var held = { tag: 0 };
                 return { get: function () { return held; }, set: function (v) { held = v; } };
             })();",
        );
        major_gc(&mut interp);
        minor_gc(&mut interp);
        let remembered_closures = interp
            .objects
            .take_remembered_ids()
            .into_iter()
            .filter(|&id| {
                matches!(
                    interp
                        .objects
                        .get_cell_expect(id)
                        .borrow()
                        .callable
                        .as_deref(),
                    Some(JsFunction::User { .. })
                )
            })
            .count();
        assert_eq!(
            remembered_closures, 0,
            "an old closure over a clean environment needs no persistent scan"
        );

        run_source(&mut interp, "cell.set({ tag: 1 });");
        for _ in 0..3 {
            minor_gc(&mut interp);
        }
        run_source(
            &mut interp,
            "if (cell.get().tag !== 1) throw new Error('stored object was collected');",
        );
    }

    use crate::interpreter::types::CARD_ELEMENTS;

    fn minor_gc(interp: &mut Interpreter) {
        interp.gc.request_minor();
        interp.gc_safepoint();
    }

    /// An old, rooted Array of `cards` cards that a major collection has put
    /// into card tracking with every card clean.
    fn tenured_array(interp: &mut Interpreter, cards: usize) -> (u64, JsValue) {
        let array = interp.create_array(vec![JsValue::number(0.0); cards * CARD_ELEMENTS]);
        let id = array.as_object_id().unwrap();
        interp.gc_root_id(id);
        major_gc(interp);
        assert!(interp.objects.get_cell_expect(id).is_old());
        assert_eq!(dirty_cards(interp, id), Some(vec![]));
        (id, array)
    }

    /// The set cards of an Array's bitmap, or `None` while every card is dirty.
    fn dirty_cards(interp: &Interpreter, array: u64) -> Option<Vec<usize>> {
        let handle = interp.objects.get_cell_expect(array);
        let obj = handle.borrow_untracked();
        let (_, bits) = obj.array_elements_and_cards().unwrap();
        set_card_list(bits)
    }

    #[test]
    fn element_store_marks_only_its_card_and_keeps_the_young_value_alive() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let (array_id, array) = tenured_array(&mut interp, 8);
        let child = fresh_object(&mut interp);

        assert!(interp.numeric_index_fast_set(&array, (6 * CARD_ELEMENTS + 1) as f64, &obj(child)));
        assert_eq!(dirty_cards(&interp, array_id), Some(vec![6]));

        minor_gc(&mut interp);
        assert!(interp.objects.get_cell(child).is_some());
        assert_eq!(dirty_cards(&interp, array_id), Some(vec![6]));

        // The child promotes after the second survival, the third scan finds
        // the card clean, and the array stops being remembered.
        minor_gc(&mut interp);
        assert!(interp.objects.get_cell_expect(child).is_old());
        minor_gc(&mut interp);
        assert_eq!(dirty_cards(&interp, array_id), Some(vec![]));
        assert_eq!(interp.objects.remembered_len(), 0);
        assert!(interp.objects.get_cell(child).is_some());
    }

    #[test]
    fn element_store_of_an_old_value_dirties_no_card() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let (array_id, array) = tenured_array(&mut interp, 4);
        let old = fresh_object(&mut interp);
        interp.gc_root_id(old);
        major_gc(&mut interp);

        assert!(interp.numeric_index_fast_set(&array, 3.0, &obj(old)));
        assert!(interp.numeric_index_fast_set(&array, 5.0, &JsValue::number(1.0)));
        assert_eq!(dirty_cards(&interp, array_id), Some(vec![]));
        assert_eq!(interp.objects.remembered_len(), 0);
    }

    #[test]
    fn append_past_the_card_bitmap_marks_a_new_card() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let (array_id, array) = tenured_array(&mut interp, 2);
        let child = fresh_object(&mut interp);

        assert!(interp.numeric_index_fast_set(&array, (2 * CARD_ELEMENTS) as f64, &obj(child)));
        assert_eq!(dirty_cards(&interp, array_id), Some(vec![2]));
        minor_gc(&mut interp);
        assert!(interp.objects.get_cell(child).is_some());
    }

    #[test]
    fn moving_a_young_element_to_another_card_keeps_it_reachable() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let (array_id, array) = tenured_array(&mut interp, 8);
        let child = fresh_object(&mut interp);
        assert!(interp.numeric_index_fast_set(&array, 1.0, &obj(child)));
        assert_eq!(dirty_cards(&interp, array_id), Some(vec![0]));

        // A shift, splice or sort moves elements without naming their cards,
        // so the bitmap must give way to a full scan.
        interp.with_array_elements_mut(&array, |elements| {
            elements.swap(1, 7 * CARD_ELEMENTS);
        });
        assert_eq!(dirty_cards(&interp, array_id), None);

        minor_gc(&mut interp);
        assert!(interp.objects.get_cell(child).is_some());
        assert_eq!(dirty_cards(&interp, array_id), Some(vec![7]));
    }

    #[test]
    fn named_property_write_on_a_card_tracked_array_is_scanned() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let (array_id, _) = tenured_array(&mut interp, 4);
        let child = fresh_object(&mut interp);

        interp
            .objects
            .get_cell_expect(array_id)
            .borrow_mut()
            .insert_value("tag".to_string(), obj(child));

        minor_gc(&mut interp);
        assert!(interp.objects.get_cell(child).is_some());
        assert_eq!(dirty_cards(&interp, array_id), Some(vec![]));
    }

    #[test]
    fn element_barrier_while_the_array_is_borrowed_rescans_every_element() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let (array_id, _) = tenured_array(&mut interp, 4);
        let child = fresh_object(&mut interp);
        let handle = interp.objects.get_cell_expect(array_id);

        {
            let _held = handle.borrow_mut_untracked();
            interp.gc_write_barrier_element(handle, 3 * CARD_ELEMENTS, &obj(child));
        }
        assert_eq!(dirty_cards(&interp, array_id), Some(vec![]));
        assert!(
            handle
                .borrow_mut_untracked()
                .overwrite_array_element(3 * CARD_ELEMENTS, &obj(child))
        );

        minor_gc(&mut interp);
        assert!(interp.objects.get_cell(child).is_some());
    }

    #[test]
    fn card_write_after_a_coarse_write_keeps_both_edges() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let (array_id, array) = tenured_array(&mut interp, 4);
        let by_card = fresh_object(&mut interp);
        let by_property = fresh_object(&mut interp);

        assert!(interp.numeric_index_fast_set(&array, 2.0, &obj(by_card)));
        interp
            .objects
            .get_cell_expect(array_id)
            .borrow_mut()
            .insert_value("tag".to_string(), obj(by_property));

        minor_gc(&mut interp);
        assert!(interp.objects.get_cell(by_card).is_some());
        assert!(interp.objects.get_cell(by_property).is_some());
    }

    #[test]
    fn promoted_object_without_a_young_edge_is_not_remembered() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let lone = fresh_object(&mut interp);
        interp.gc_root_id(lone);

        minor_gc(&mut interp);
        minor_gc(&mut interp);
        assert!(interp.objects.get_cell_expect(lone).is_old());
        assert_eq!(interp.objects.remembered_len(), 0);
    }

    #[test]
    fn promoted_object_with_a_young_edge_is_remembered() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let parent = fresh_object(&mut interp);
        interp.gc_root_id(parent);
        minor_gc(&mut interp);

        let child = fresh_object(&mut interp);
        interp
            .objects
            .get_cell_expect(parent)
            .borrow_mut()
            .insert_value("child".to_string(), obj(child));
        minor_gc(&mut interp);
        assert!(interp.objects.get_cell_expect(parent).is_old());
        assert!(interp.objects.get_cell_expect(child).is_young());
        assert_eq!(interp.objects.remembered_len(), 1);
    }

    fn set_card_list(bits: Option<&[u64]>) -> Option<Vec<usize>> {
        bits.map(|bits| {
            (0..bits.len() * 64)
                .filter(|card| bits[card / 64] & (1 << (card % 64)) != 0)
                .collect()
        })
    }

    fn map_cards(interp: &Interpreter, id: u64) -> Option<Vec<usize>> {
        let handle = interp.objects.get_cell_expect(id);
        let obj = handle.borrow_untracked();
        set_card_list(obj.map_data().unwrap().dirty_card_bits())
    }

    fn set_cards(interp: &Interpreter, id: u64) -> Option<Vec<usize>> {
        let handle = interp.objects.get_cell_expect(id);
        let obj = handle.borrow_untracked();
        set_card_list(obj.set_data().unwrap().dirty_card_bits())
    }

    fn map_set(interp: &Interpreter, id: u64, key: JsValue, value: JsValue) {
        let handle = interp.objects.get_cell_expect(id);
        let young = interp.gc_write_barrier_entry(handle, &[&key, &value]);
        handle
            .borrow_mut_untracked()
            .map_data_mut()
            .unwrap()
            .upsert_marking((key, value), |old, new| old.1 = new.1, young);
    }

    fn set_add(interp: &Interpreter, id: u64, value: JsValue) {
        let handle = interp.objects.get_cell_expect(id);
        let young = interp.gc_write_barrier_entry(handle, &[&value]);
        handle
            .borrow_mut_untracked()
            .set_data_mut()
            .unwrap()
            .upsert_marking(value, |_, _| {}, young);
    }

    /// An old, rooted Map of `cards` cards of number entries that a major
    /// collection has put into card tracking with every card clean.
    fn tenured_map(interp: &mut Interpreter, cards: usize) -> u64 {
        let mut data = JsObjectData::new();
        data.set_class("Map");
        let mut entries = KeyedEntries::default();
        for i in 0..cards * CARD_ELEMENTS {
            entries.upsert((JsValue::number(i as f64), JsValue::number(0.0)), |_, _| {});
        }
        data.kind = ObjectKind::Map(Box::new(entries));
        let id = interp.alloc_object(data);
        interp.gc_root_id(id);
        major_gc(interp);
        assert!(interp.objects.get_cell_expect(id).is_old());
        assert_eq!(map_cards(interp, id), Some(vec![]));
        id
    }

    fn tenured_set(interp: &mut Interpreter, cards: usize) -> u64 {
        let mut data = JsObjectData::new();
        data.set_class("Set");
        let mut entries = KeyedEntries::default();
        for i in 0..cards * CARD_ELEMENTS {
            entries.upsert(JsValue::number(i as f64), |_, _| {});
        }
        data.kind = ObjectKind::Set(Box::new(entries));
        let id = interp.alloc_object(data);
        interp.gc_root_id(id);
        major_gc(interp);
        assert!(interp.objects.get_cell_expect(id).is_old());
        assert_eq!(set_cards(interp, id), Some(vec![]));
        id
    }

    #[test]
    fn map_set_marks_only_its_card_and_keeps_the_young_value_alive() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let map = tenured_map(&mut interp, 8);
        let child = fresh_object(&mut interp);

        map_set(
            &interp,
            map,
            JsValue::number((6 * CARD_ELEMENTS + 1) as f64),
            obj(child),
        );
        assert_eq!(map_cards(&interp, map), Some(vec![6]));

        minor_gc(&mut interp);
        assert!(interp.objects.get_cell(child).is_some());
        assert_eq!(map_cards(&interp, map), Some(vec![6]));

        minor_gc(&mut interp);
        assert!(interp.objects.get_cell_expect(child).is_old());
        minor_gc(&mut interp);
        assert_eq!(map_cards(&interp, map), Some(vec![]));
        assert_eq!(interp.objects.remembered_len(), 0);
        assert!(interp.objects.get_cell(child).is_some());
    }

    #[test]
    fn map_set_of_a_young_key_appends_into_a_new_card() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let map = tenured_map(&mut interp, 2);
        let key = fresh_object(&mut interp);

        map_set(&interp, map, obj(key), JsValue::number(1.0));
        assert_eq!(map_cards(&interp, map), Some(vec![2]));
        minor_gc(&mut interp);
        assert!(interp.objects.get_cell(key).is_some());
    }

    #[test]
    fn map_set_of_old_values_dirties_no_card() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let map = tenured_map(&mut interp, 4);
        let old = fresh_object(&mut interp);
        interp.gc_root_id(old);
        major_gc(&mut interp);

        map_set(&interp, map, JsValue::number(3.0), obj(old));
        map_set(&interp, map, JsValue::number(-1.0), JsValue::number(1.0));
        assert_eq!(map_cards(&interp, map), Some(vec![]));
        assert_eq!(interp.objects.remembered_len(), 0);
    }

    #[test]
    fn set_add_marks_only_its_card() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let set = tenured_set(&mut interp, 5);
        let child = fresh_object(&mut interp);

        set_add(&interp, set, obj(child));
        assert_eq!(set_cards(&interp, set), Some(vec![5]));
        minor_gc(&mut interp);
        minor_gc(&mut interp);
        minor_gc(&mut interp);
        assert!(interp.objects.get_cell(child).is_some());
        assert_eq!(set_cards(&interp, set), Some(vec![]));
        assert_eq!(interp.objects.remembered_len(), 0);
    }

    #[test]
    fn deleting_until_compaction_keeps_a_young_entry_reachable() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let map = tenured_map(&mut interp, 8);
        let child = fresh_object(&mut interp);
        let last = ((8 * CARD_ELEMENTS) - 1) as f64;
        map_set(&interp, map, JsValue::number(last), obj(child));
        assert_eq!(map_cards(&interp, map), Some(vec![7]));

        // Removing most entries compacts the slots, moving the young value
        // into card 0; the stale bitmap must give way to a full scan.
        {
            let handle = interp.objects.get_cell_expect(map);
            let mut borrowed = handle.borrow_mut_untracked();
            let entries = borrowed.map_data_mut().unwrap();
            for i in 0..(7 * CARD_ELEMENTS) {
                entries.remove(&JsValue::number(i as f64));
            }
        }
        assert_eq!(map_cards(&interp, map), None);

        for _ in 0..4 {
            minor_gc(&mut interp);
            assert!(interp.objects.get_cell(child).is_some());
        }
    }

    #[test]
    fn clear_then_refill_with_young_values_is_scanned() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let set = tenured_set(&mut interp, 4);
        {
            let handle = interp.objects.get_cell_expect(set);
            handle
                .borrow_mut_untracked()
                .set_data_mut()
                .unwrap()
                .clear();
        }
        let child = fresh_object(&mut interp);
        set_add(&interp, set, obj(child));
        minor_gc(&mut interp);
        assert!(interp.objects.get_cell(child).is_some());
    }

    #[test]
    fn conservative_upsert_on_a_card_tracked_map_is_scanned() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let map = tenured_map(&mut interp, 4);
        let child = fresh_object(&mut interp);
        {
            let handle = interp.objects.get_cell_expect(map);
            handle
                .borrow_mut()
                .map_data_mut()
                .unwrap()
                .upsert((JsValue::number(2.0), obj(child)), |old, new| old.1 = new.1);
        }
        assert_eq!(map_cards(&interp, map), None);
        minor_gc(&mut interp);
        assert!(interp.objects.get_cell(child).is_some());
    }

    #[test]
    fn named_property_write_on_a_card_tracked_set_is_scanned() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let set = tenured_set(&mut interp, 4);
        let by_card = fresh_object(&mut interp);
        let by_property = fresh_object(&mut interp);

        set_add(&interp, set, obj(by_card));
        interp
            .objects
            .get_cell_expect(set)
            .borrow_mut()
            .insert_value("tag".to_string(), obj(by_property));

        minor_gc(&mut interp);
        assert!(interp.objects.get_cell(by_card).is_some());
        assert!(interp.objects.get_cell(by_property).is_some());
    }

    #[test]
    fn promoted_map_is_fully_scanned_once() {
        let mut interp = Interpreter::new();
        tenure_initial_heap(&mut interp);
        let mut data = JsObjectData::new();
        data.set_class("Map");
        data.kind = ObjectKind::Map(Box::default());
        let map = interp.alloc_object(data);
        interp.gc_root_id(map);
        let child = fresh_object(&mut interp);
        for i in 0..(3 * CARD_ELEMENTS) {
            let value = if i == 2 * CARD_ELEMENTS + 1 {
                obj(child)
            } else {
                JsValue::number(0.0)
            };
            map_set(&interp, map, JsValue::number(i as f64), value);
        }
        // The map is young, so nothing was recorded; promotion makes every
        // card dirty and the first minor scan narrows it.
        minor_gc(&mut interp);
        minor_gc(&mut interp);
        assert!(interp.objects.get_cell_expect(map).is_old());
        minor_gc(&mut interp);
        assert!(interp.objects.get_cell(child).is_some());
    }
}

#[path = "gc_weak.rs"]
mod weak;

#[cfg(test)]
#[path = "gc_weak_slot_tests.rs"]
mod weak_slot_tests;
