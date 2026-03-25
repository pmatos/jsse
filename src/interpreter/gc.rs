use super::*;

impl Interpreter {
    pub(crate) fn allocate_object_slot(&mut self, obj: Rc<RefCell<JsObjectData>>) -> u64 {
        self.gc_alloc_count += 1;
        let id = if let Some(idx) = self.free_list.pop() {
            self.objects[idx] = Some(obj.clone());
            idx as u64
        } else {
            let idx = self.objects.len();
            self.objects.push(Some(obj.clone()));
            idx as u64
        };
        obj.borrow_mut().id = Some(id);
        id
    }

    pub(crate) fn maybe_gc(&mut self) {
        if self.gc_alloc_count < GC_THRESHOLD {
            return;
        }
        self.gc_alloc_count = 0;
        let obj_count = self.objects.len();
        let mut marks = vec![false; obj_count];

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
        for (roots, _) in &self.microtask_queue {
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
        for afs in self.async_function_states.values() {
            Self::collect_env_roots(&afs.func_env, &mut worklist);
            Self::collect_value_roots(&afs.resolve_fn, &mut worklist);
            Self::collect_value_roots(&afs.reject_fn, &mut worklist);
            if let Some(ref v) = afs.pending_return {
                Self::collect_value_roots(v, &mut worklist);
            }
            if let Some(ref v) = afs.saved_finally_exception {
                Self::collect_value_roots(v, &mut worklist);
            }
        }

        // Mark phase (BFS)
        while let Some(id) = worklist.pop() {
            let idx = id as usize;
            if idx >= obj_count || marks[idx] {
                continue;
            }
            marks[idx] = true;
            let obj_rc = match &self.objects[idx] {
                Some(rc) => rc.clone(),
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
                let obj_rc = match &self.objects[i] {
                    Some(rc) => rc.clone(),
                    None => continue,
                };
                let obj = obj_rc.borrow();
                if obj.class_name != "WeakMap" {
                    continue;
                }
                if let Some(ref entries) = obj.map_data {
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
                let obj_rc = match &self.objects[idx] {
                    Some(rc) => rc.clone(),
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
            if !mark && self.objects[i].is_some() {
                self.objects[i] = None;
                self.free_list.push(i);
                self.function_realm_map.remove(&(i as u64));
            }
        }

        // Post-sweep: clear dead weak entries
        for i in 0..obj_count {
            if !marks[i] {
                continue;
            }
            let obj_rc = match &self.objects[i] {
                Some(rc) => rc.clone(),
                None => continue,
            };
            let mut obj = obj_rc.borrow_mut();
            if obj.class_name == "WeakMap" {
                if let Some(ref mut entries) = obj.map_data {
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
                && let Some(ref mut entries) = obj.set_data
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
    }

    fn collect_value_roots(val: &JsValue, worklist: &mut Vec<u64>) {
        if let JsValue::Object(o) = val {
            worklist.push(o.id);
        }
    }

    fn trace_object_fields(obj: &JsObjectData, worklist: &mut Vec<u64>) {
        if let Some(ref proto) = obj.prototype
            && let Some(pid) = proto.borrow().id
        {
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
        if let Some(ref elems) = obj.array_elements {
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
        if let Some(target_id) = obj.wrapped_target_function_id {
            worklist.push(target_id);
        }
        if let Some(ref target) = obj.bound_target_function {
            Self::collect_value_roots(target, worklist);
        }
        if let Some(ref bargs) = obj.bound_args {
            for v in bargs {
                Self::collect_value_roots(v, worklist);
            }
        }
        if let Some(ref map) = obj.parameter_map {
            for (env_ref, _) in map.values() {
                Self::collect_env_roots(env_ref, worklist);
            }
        }
        if obj.class_name != "WeakMap"
            && let Some(ref entries) = obj.map_data
        {
            for entry in entries.iter().flatten() {
                Self::collect_value_roots(&entry.0, worklist);
                Self::collect_value_roots(&entry.1, worklist);
            }
        }
        if obj.class_name != "WeakSet"
            && let Some(ref entries) = obj.set_data
        {
            for val in entries.iter().flatten() {
                Self::collect_value_roots(val, worklist);
            }
        }
        if let Some(ref target) = obj.proxy_target
            && let Some(tid) = target.borrow().id
        {
            worklist.push(tid);
        }
        if let Some(ref handler) = obj.proxy_handler
            && let Some(hid) = handler.borrow().id
        {
            worklist.push(hid);
        }
        if let Some(ref pd) = obj.promise_data {
            match &pd.state {
                crate::interpreter::types::PromiseState::Fulfilled(v)
                | crate::interpreter::types::PromiseState::Rejected(v) => {
                    Self::collect_value_roots(v, worklist);
                }
                crate::interpreter::types::PromiseState::Pending => {}
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
        if let Some(buf_id) = obj.view_buffer_object_id {
            worklist.push(buf_id);
        }
        if let Some(ref roots) = obj.gc_native_roots {
            for v in roots {
                Self::collect_value_roots(v, worklist);
            }
        }
        if let Some((ref iter, ref next)) = obj.wrap_iter_record {
            Self::collect_value_roots(iter, worklist);
            Self::collect_value_roots(next, worklist);
        }
        if let Some(ref v) = obj.helper_next_closure {
            Self::collect_value_roots(v, worklist);
        }
        if let Some(ref v) = obj.helper_return_closure {
            Self::collect_value_roots(v, worklist);
        }
        if let Some(ref state) = obj.iterator_state {
            match state {
                IteratorState::ArrayIterator { array_id, .. } => worklist.push(*array_id),
                IteratorState::TypedArrayIterator { typed_array_id, .. } => {
                    worklist.push(*typed_array_id)
                }
                IteratorState::MapIterator { map_id, .. } => worklist.push(*map_id),
                IteratorState::SetIterator { set_id, .. } => worklist.push(*set_id),
                IteratorState::Generator { func_env, .. }
                | IteratorState::AsyncGenerator { func_env, .. }
                | IteratorState::StateMachineGenerator { func_env, .. }
                | IteratorState::StateMachineAsyncGenerator { func_env, .. } => {
                    Self::collect_env_roots(func_env, worklist);
                }
                _ => {}
            }
        }
    }

    pub(crate) fn collect_env_roots(env: &EnvRef, worklist: &mut Vec<u64>) {
        let mut current = Some(env.clone());
        let mut seen = std::collections::HashSet::new();
        while let Some(e) = current {
            let ptr = Rc::as_ptr(&e) as usize;
            if !seen.insert(ptr) {
                break;
            }
            let borrowed = e.borrow();
            for binding in borrowed.bindings.values() {
                Self::collect_value_roots(&binding.value, worklist);
            }
            current = borrowed.parent.clone();
        }
    }
}
