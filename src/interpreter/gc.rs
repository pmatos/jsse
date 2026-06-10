use super::*;

impl Interpreter {
    /// Allocate a fresh object slot for `data` and return its id. The id is
    /// written to `data.id` inside the arena's `alloc`, so the field is set
    /// exactly once at allocation and never reassigned.
    pub(crate) fn alloc_object(&mut self, mut data: JsObjectData) -> u64 {
        self.gc_alloc_count += 1;
        data.shape_id = crate::interpreter::types::fresh_shape_id();
        let (id, was_reuse) = self.objects.alloc(data);
        let cost = if was_reuse {
            GC_OBJECT_OVERHEAD / 2
        } else {
            GC_OBJECT_OVERHEAD
        };
        self.gc_bytes_since_gc += cost;
        if self.gc_alloc_count >= GC_THRESHOLD || self.gc_bytes_since_gc >= self.gc_threshold_bytes
        {
            self.gc_requested = true;
        }
        id
    }

    pub(crate) fn gc_track_external_bytes(&mut self, bytes: usize) {
        self.gc_external_bytes += bytes;
        self.gc_bytes_since_gc += bytes;
        if self.gc_bytes_since_gc >= self.gc_threshold_bytes {
            self.gc_requested = true;
        }
    }

    pub(crate) fn gc_untrack_external_bytes(&mut self, bytes: usize) {
        self.gc_external_bytes = self.gc_external_bytes.saturating_sub(bytes);
    }

    pub(crate) fn gc_safepoint(&mut self) {
        if !self.gc_requested {
            return;
        }
        self.gc_requested = false;
        self.gc_alloc_count = 0;
        let obj_count = self.objects.capacity() as usize;
        // Reuse the mark bitmap buffer across collections to avoid per-GC
        // allocation churn. clear()+resize(_, false) yields an all-false buffer
        // (invariant required by mark/sweep) while keeping the backing capacity.
        let mut marks = std::mem::take(&mut self.gc_marks);
        marks.clear();
        marks.resize(obj_count, false);

        // Collect roots from all realms
        let mut worklist: Vec<u64> = Vec::new();
        for realm in &self.realms {
            realm.collect_roots(&mut worklist);
        }
        if let Some(JsValue::Object(o)) = &self.new_target {
            worklist.push(o.id);
        }
        // Root from module environments (not reachable from global_env)
        for module in self.module_registry.values() {
            let m = module.borrow();
            Self::collect_env_roots(&m.env, &mut worklist);
            for val in m.exports.values() {
                Self::collect_value_roots(val, &mut worklist);
            }
            if let Some((promise, resolve, reject)) = &m.top_level_capability {
                Self::collect_value_roots(promise, &mut worklist);
                Self::collect_value_roots(resolve, &mut worklist);
                Self::collect_value_roots(reject, &mut worklist);
            }
        }
        for module in self.synthetic_module_registry.values() {
            let m = module.borrow();
            Self::collect_env_roots(&m.env, &mut worklist);
            for val in m.exports.values() {
                Self::collect_value_roots(val, &mut worklist);
            }
        }
        // Trace active call stack environments
        for env in &self.call_stack_envs {
            Self::collect_env_roots(env, &mut worklist);
        }
        for frame in &self.call_stack_frames {
            if frame.func_obj_id != 0 {
                worklist.push(frame.func_obj_id);
            }
            Self::collect_value_roots(&frame.arguments_obj, &mut worklist);
        }
        // Temporary roots (iterators, etc.)
        worklist.extend_from_slice(&self.gc_temp_roots);
        // Root values captured in pending microtask closures
        for roots in self.scheduler.iter_microtask_roots() {
            for val in roots {
                Self::collect_value_roots(val, &mut worklist);
            }
        }
        // Iterators pending close when a generator yields during destructuring
        for val in &self.pending_iter_close {
            Self::collect_value_roots(val, &mut worklist);
        }
        for iters in self.generator_inline_iters.values() {
            for val in iters {
                Self::collect_value_roots(val, &mut worklist);
            }
        }
        for val in self.iterator_next_cache.values() {
            Self::collect_value_roots(val, &mut worklist);
        }
        for afs in self.scheduler.iter_async_function_states() {
            Self::collect_env_roots(&afs.func_env, &mut worklist);
            Self::collect_value_roots(&afs.resolve_fn, &mut worklist);
            Self::collect_value_roots(&afs.reject_fn, &mut worklist);
            if let Some(ref v) = afs.pending_return {
                Self::collect_value_roots(v, &mut worklist);
            }
            if let Some(ref v) = afs.saved_finally_exception {
                Self::collect_value_roots(v, &mut worklist);
            }
            if let Some(ref env) = afs.for_of_iter_env {
                Self::collect_env_roots(env, &mut worklist);
            }
        }

        // Mark phase (BFS)
        while let Some(id) = worklist.pop() {
            let idx = id as usize;
            if idx >= obj_count || marks[idx] {
                continue;
            }
            marks[idx] = true;
            let obj_rc = match self.objects.get(id) {
                Some(rc) => rc,
                None => continue,
            };
            let obj = obj_rc.borrow();
            Self::trace_object_fields(&obj, &mut worklist);
        }

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
                let obj = obj_rc.borrow();
                if obj.class_name != "WeakMap" {
                    continue;
                }
                if let Some(entries) = obj.map_data() {
                    for entry in entries.iter().flatten() {
                        if let JsValue::Object(key_obj) = &entry.0 {
                            let kid = key_obj.id as usize;
                            if kid < obj_count && marks[kid] {
                                // Key is reachable — mark the value
                                if let JsValue::Object(val_obj) = &entry.1 {
                                    let vid = val_obj.id as usize;
                                    if vid < obj_count && !marks[vid] {
                                        marks[vid] = true;
                                        new_marks = true;
                                        worklist.push(val_obj.id);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // BFS from any newly marked objects (use same full tracing as main mark phase)
            while let Some(id) = worklist.pop() {
                let idx = id as usize;
                if idx >= obj_count || marks[idx] {
                    continue;
                }
                marks[idx] = true;
                new_marks = true;
                let obj_rc = match self.objects.get(id) {
                    Some(rc) => rc,
                    None => continue,
                };
                let obj = obj_rc.borrow();
                Self::trace_object_fields(&obj, &mut worklist);
            }
            if !new_marks {
                break;
            }
        }

        // Sweep phase
        for (i, mark) in marks.iter().enumerate().take(obj_count) {
            let id = i as u64;
            let live = self.objects.slot_at(id).is_some_and(|s| s.is_some());
            if !mark && live {
                if let Some(obj_rc) = self.objects.get(id) {
                    let obj = obj_rc.borrow();
                    if let Some(buf_data) = obj.arraybuffer_data()
                        && let BufferData::Owned(ref v) = *buf_data.borrow()
                    {
                        self.gc_external_bytes = self.gc_external_bytes.saturating_sub(v.len());
                    }
                }
                self.objects.free(id);
                self.function_realm_map.remove(&id);
                self.iterator_next_cache.remove(&id);
                self.generator_inline_iters.remove(&id);
            }
        }
        // Adaptive threshold: scale next GC budget from live heap size
        let live_count = self.objects.live_count();
        let live_bytes = live_count * GC_OBJECT_OVERHEAD + self.gc_external_bytes;
        self.gc_threshold_bytes =
            std::cmp::max(GC_MIN_THRESHOLD_BYTES, live_bytes * GC_GROWTH_FACTOR);
        self.gc_bytes_since_gc = 0;
        // Post-sweep: clear dead weak entries
        for i in 0..obj_count {
            if !marks[i] {
                continue;
            }
            let obj_rc = match self.objects.get(i as u64) {
                Some(rc) => rc,
                None => continue,
            };
            let mut obj = obj_rc.borrow_mut();
            if obj.class_name == "WeakMap" {
                if let Some(entries) = obj.map_data_mut() {
                    for entry in entries.iter_mut() {
                        let dead = if let Some((JsValue::Object(key_obj), _)) = entry {
                            let kid = key_obj.id as usize;
                            kid >= obj_count || !marks[kid]
                        } else {
                            false
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
                    let dead = if let Some(JsValue::Object(val_obj)) = entry {
                        let vid = val_obj.id as usize;
                        vid >= obj_count || !marks[vid]
                    } else {
                        false
                    };
                    if dead {
                        *entry = None;
                    }
                }
            }
        }
        // Return the buffer to the interpreter so its capacity is reused next GC.
        self.gc_marks = marks;
    }

    fn collect_value_roots(val: &JsValue, worklist: &mut Vec<u64>) {
        if let JsValue::Object(o) = val {
            worklist.push(o.id);
        }
    }

    fn trace_object_fields(obj: &JsObjectData, worklist: &mut Vec<u64>) {
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
            Self::collect_env_roots(closure, worklist);
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
                Self::collect_iterator_state_roots(state, worklist);
            }
            ObjectKind::Arguments(map) => {
                for (env_ref, _) in map.values() {
                    Self::collect_env_roots(env_ref, worklist);
                }
            }
            ObjectKind::Array(_) => {
                // Array elements are visited via the property walk above
                // (array_elements is a separate compact storage; values are also tracked).
            }
            ObjectKind::ModuleNamespace(ns) => {
                Self::collect_env_roots(&ns.env, worklist);
            }
        }
    }

    fn collect_iterator_state_roots(state: &IteratorState, worklist: &mut Vec<u64>) {
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
                Self::collect_env_roots(func_env, worklist);
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
                Self::collect_env_roots(func_env, worklist);
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

    pub(crate) fn collect_env_roots(env: &EnvRef, worklist: &mut Vec<u64>) {
        let mut current = Some(env.clone());
        let mut seen = HashSet::new();
        while let Some(e) = current {
            let ptr = Rc::as_ptr(&e) as usize;
            if !seen.insert(ptr) {
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
        JsValue::Object(crate::types::JsObject { id })
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
        Interpreter::collect_value_roots(&JsValue::Undefined, &mut worklist);
        Interpreter::collect_value_roots(&JsValue::Number(3.0), &mut worklist);
        Interpreter::collect_value_roots(&JsValue::Boolean(true), &mut worklist);
        assert!(worklist.is_empty());
    }

    #[test]
    fn trace_object_fields_roots_prototype_and_data_properties() {
        let mut data = JsObjectData::new();
        data.prototype_id = Some(7);
        data.properties.insert(
            "x".to_string(),
            PropertyDescriptor::data(obj(8), true, true, true),
        );
        data.properties.insert(
            "n".to_string(),
            PropertyDescriptor::data(JsValue::Number(1.0), true, true, true),
        );

        let mut worklist = Vec::new();
        Interpreter::trace_object_fields(&data, &mut worklist);
        assert_eq!(as_set(worklist), vec![7, 8]);
    }

    #[test]
    fn trace_object_fields_roots_accessor_get_and_set() {
        let mut data = JsObjectData::new();
        data.properties.insert(
            "acc".to_string(),
            PropertyDescriptor::accessor(Some(obj(10)), Some(obj(11)), true, true),
        );

        let mut worklist = Vec::new();
        Interpreter::trace_object_fields(&data, &mut worklist);
        assert_eq!(as_set(worklist), vec![10, 11]);
    }

    #[test]
    fn trace_object_fields_roots_array_elements_and_native_roots() {
        let mut data = JsObjectData::new();
        data.kind = ObjectKind::Array(vec![obj(20), JsValue::Number(0.0), obj(21)]);
        data.gc_native_roots = Some(vec![obj(22)]);

        let mut worklist = Vec::new();
        Interpreter::trace_object_fields(&data, &mut worklist);
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
        Interpreter::collect_env_roots(&child, &mut worklist);
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
        Interpreter::collect_env_roots(&cyclic, &mut worklist);
        assert_eq!(worklist, vec![32]);
    }
}
