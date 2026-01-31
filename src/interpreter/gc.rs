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

        // Collect roots
        let mut worklist: Vec<u64> = Vec::new();
        Self::collect_env_roots(&self.global_env, &mut worklist);
        for proto in [
            &self.object_prototype,
            &self.array_prototype,
            &self.string_prototype,
            &self.number_prototype,
            &self.boolean_prototype,
            &self.regexp_prototype,
            &self.iterator_prototype,
            &self.array_iterator_prototype,
            &self.string_iterator_prototype,
            &self.map_prototype,
            &self.map_iterator_prototype,
            &self.set_prototype,
            &self.set_iterator_prototype,
            &self.date_prototype,
            &self.generator_prototype,
            &self.weakmap_prototype,
            &self.weakset_prototype,
            &self.symbol_prototype,
            &self.arraybuffer_prototype,
            &self.typed_array_prototype,
            &self.int8array_prototype,
            &self.uint8array_prototype,
            &self.uint8clampedarray_prototype,
            &self.int16array_prototype,
            &self.uint16array_prototype,
            &self.int32array_prototype,
            &self.uint32array_prototype,
            &self.float32array_prototype,
            &self.float64array_prototype,
            &self.bigint64array_prototype,
            &self.biguint64array_prototype,
            &self.dataview_prototype,
            &self.promise_prototype,
        ] {
            if let Some(p) = proto
                && let Some(id) = p.borrow().id {
                    worklist.push(id);
                }
        }
        if let Some(JsValue::Object(o)) = &self.new_target {
            worklist.push(o.id);
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

            // Trace prototype
            if let Some(ref proto) = obj.prototype
                && let Some(pid) = proto.borrow().id {
                    worklist.push(pid);
                }

            // Trace properties
            for desc in obj.properties.values() {
                if let Some(ref v) = desc.value {
                    Self::collect_value_roots(v, &mut worklist);
                }
                if let Some(ref v) = desc.get {
                    Self::collect_value_roots(v, &mut worklist);
                }
                if let Some(ref v) = desc.set {
                    Self::collect_value_roots(v, &mut worklist);
                }
            }

            // Trace array elements
            if let Some(ref elems) = obj.array_elements {
                for v in elems {
                    Self::collect_value_roots(v, &mut worklist);
                }
            }

            // Trace primitive value
            if let Some(ref v) = obj.primitive_value {
                Self::collect_value_roots(v, &mut worklist);
            }

            // Trace private fields
            for elem in obj.private_fields.values() {
                match elem {
                    PrivateElement::Field(v) | PrivateElement::Method(v) => {
                        Self::collect_value_roots(v, &mut worklist);
                    }
                    PrivateElement::Accessor { get, set } => {
                        if let Some(g) = get {
                            Self::collect_value_roots(g, &mut worklist);
                        }
                        if let Some(s) = set {
                            Self::collect_value_roots(s, &mut worklist);
                        }
                    }
                }
            }

            // Trace callable (closure environments)
            if let Some(ref func) = obj.callable
                && let JsFunction::User { closure, .. } = func {
                    Self::collect_env_roots(closure, &mut worklist);
                }

            // Trace parameter_map environments
            if let Some(ref map) = obj.parameter_map {
                for (env_ref, _) in map.values() {
                    Self::collect_env_roots(env_ref, &mut worklist);
                }
            }

            // Trace map_data (skip WeakMap — handled by ephemeron pass)
            if obj.class_name != "WeakMap"
                && let Some(ref entries) = obj.map_data {
                    for entry in entries.iter().flatten() {
                        Self::collect_value_roots(&entry.0, &mut worklist);
                        Self::collect_value_roots(&entry.1, &mut worklist);
                    }
                }

            // Trace set_data (skip WeakSet — cleared post-sweep)
            if obj.class_name != "WeakSet"
                && let Some(ref entries) = obj.set_data {
                    for val in entries.iter().flatten() {
                        Self::collect_value_roots(val, &mut worklist);
                    }
                }

            // Trace proxy target/handler
            if let Some(ref target) = obj.proxy_target
                && let Some(tid) = target.borrow().id {
                    worklist.push(tid);
                }
            if let Some(ref handler) = obj.proxy_handler
                && let Some(hid) = handler.borrow().id {
                    worklist.push(hid);
                }

            // Trace iterator state
            if let Some(ref state) = obj.iterator_state {
                match state {
                    IteratorState::ArrayIterator { array_id, .. } => worklist.push(*array_id),
                    IteratorState::MapIterator { map_id, .. } => worklist.push(*map_id),
                    IteratorState::SetIterator { set_id, .. } => worklist.push(*set_id),
                    IteratorState::Generator {
                        closure,
                        args,
                        this_val,
                        ..
                    } => {
                        Self::collect_env_roots(closure, &mut worklist);
                        for arg in args {
                            Self::collect_value_roots(arg, &mut worklist);
                        }
                        Self::collect_value_roots(this_val, &mut worklist);
                    }
                    _ => {}
                }
            }
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
            // BFS from any newly marked objects
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
                if let Some(ref proto) = obj.prototype
                    && let Some(pid) = proto.borrow().id {
                        worklist.push(pid);
                    }
                for desc in obj.properties.values() {
                    if let Some(ref v) = desc.value {
                        Self::collect_value_roots(v, &mut worklist);
                    }
                    if let Some(ref v) = desc.get {
                        Self::collect_value_roots(v, &mut worklist);
                    }
                    if let Some(ref v) = desc.set {
                        Self::collect_value_roots(v, &mut worklist);
                    }
                }
                if let Some(ref elems) = obj.array_elements {
                    for v in elems {
                        Self::collect_value_roots(v, &mut worklist);
                    }
                }
                if let Some(ref v) = obj.primitive_value {
                    Self::collect_value_roots(v, &mut worklist);
                }
                if let Some(ref func) = obj.callable
                    && let JsFunction::User { closure, .. } = func {
                        Self::collect_env_roots(closure, &mut worklist);
                    }
                if obj.class_name != "WeakMap"
                    && let Some(ref entries) = obj.map_data {
                        for entry in entries.iter().flatten() {
                            Self::collect_value_roots(&entry.0, &mut worklist);
                            Self::collect_value_roots(&entry.1, &mut worklist);
                        }
                    }
                if obj.class_name != "WeakSet"
                    && let Some(ref entries) = obj.set_data {
                        for val in entries.iter().flatten() {
                            Self::collect_value_roots(val, &mut worklist);
                        }
                    }
            }
            if !new_marks {
                break;
            }
        }

        // Sweep phase
        for i in 0..obj_count {
            if !marks[i] && self.objects[i].is_some() {
                self.objects[i] = None;
                self.free_list.push(i);
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
                        let dead = if let Some((ref k, _)) = *entry {
                            if let JsValue::Object(key_obj) = k {
                                let kid = key_obj.id as usize;
                                kid >= obj_count || !marks[kid]
                            } else {
                                false
                            }
                        } else {
                            false
                        };
                        if dead {
                            *entry = None;
                        }
                    }
                }
            } else if obj.class_name == "WeakSet"
                && let Some(ref mut entries) = obj.set_data {
                    for entry in entries.iter_mut() {
                        let dead = if let Some(ref val) = *entry {
                            if let JsValue::Object(val_obj) = val {
                                let vid = val_obj.id as usize;
                                vid >= obj_count || !marks[vid]
                            } else {
                                false
                            }
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

    fn collect_env_roots(env: &EnvRef, worklist: &mut Vec<u64>) {
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
