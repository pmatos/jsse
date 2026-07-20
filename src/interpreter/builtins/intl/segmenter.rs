use super::super::super::*;
use icu::segmenter::{GraphemeClusterSegmenter, SentenceSegmenter, WordSegmenter};

struct SegmentInfo {
    segment: Vec<u16>,
    index: usize, // UTF-16 code unit index
    is_word_like: Option<bool>,
}

fn compute_segments(input: &[u16], granularity: &str) -> Vec<SegmentInfo> {
    let mut segments = Vec::new();

    match granularity {
        "word" => {
            let ws = WordSegmenter::new_auto(Default::default());
            let mut iter = ws.segment_utf16(input);
            let mut prev = match iter.next() {
                Some(p) => p,
                None => return segments,
            };
            while let Some(pos) = iter.next() {
                let is_word_like = iter.is_word_like();
                segments.push(SegmentInfo {
                    segment: input[prev..pos].to_vec(),
                    index: prev,
                    is_word_like: Some(is_word_like),
                });
                prev = pos;
            }
        }
        "sentence" => {
            let ss = SentenceSegmenter::new(Default::default());
            let breaks: Vec<usize> = ss.segment_utf16(input).collect();
            for w in breaks.windows(2) {
                segments.push(SegmentInfo {
                    segment: input[w[0]..w[1]].to_vec(),
                    index: w[0],
                    is_word_like: None,
                });
            }
        }
        _ => {
            let gs = GraphemeClusterSegmenter::new();
            let breaks: Vec<usize> = gs.segment_utf16(input).collect();
            for w in breaks.windows(2) {
                segments.push(SegmentInfo {
                    segment: input[w[0]..w[1]].to_vec(),
                    index: w[0],
                    is_word_like: None,
                });
            }
        }
    }

    segments
}

fn compute_break_points_utf16(input: &[u16], granularity: &str) -> Vec<usize> {
    match granularity {
        "word" => {
            let ws = WordSegmenter::new_auto(Default::default());
            ws.segment_utf16(input).collect()
        }
        "sentence" => {
            let ss = SentenceSegmenter::new(Default::default());
            ss.segment_utf16(input).collect()
        }
        _ => {
            let gs = GraphemeClusterSegmenter::new();
            gs.segment_utf16(input).collect()
        }
    }
}

fn compute_word_like_at_break_utf16(input: &[u16], break_end_utf16: usize) -> bool {
    let ws = WordSegmenter::new_auto(Default::default());
    let mut iter = ws.segment_utf16(input);
    loop {
        match iter.next() {
            Some(p) if p == break_end_utf16 => return iter.is_word_like(),
            Some(_) => continue,
            None => return false,
        }
    }
}

fn create_segment_object(
    interp: &mut Interpreter,
    segment: &[u16],
    index: usize,
    input: &[u16],
    is_word_like: Option<bool>,
) -> JsValue {
    let obj_id = interp.create_object_id();
    if let Some(op_id) = interp.realm().object_prototype {
        interp
            .get_object_cell_expect(obj_id)
            .borrow_mut()
            .prototype_id = Some(op_id);
    }
    interp
        .get_object_cell_expect(obj_id)
        .borrow_mut()
        .insert_property(
            "segment".to_string(),
            PropertyDescriptor::data(
                JsValue::String(JsString::from_vec(segment.to_vec())),
                true,
                true,
                true,
            ),
        );
    interp
        .get_object_cell_expect(obj_id)
        .borrow_mut()
        .insert_property(
            "index".to_string(),
            PropertyDescriptor::data(JsValue::Number(index as f64), true, true, true),
        );
    interp
        .get_object_cell_expect(obj_id)
        .borrow_mut()
        .insert_property(
            "input".to_string(),
            PropertyDescriptor::data(
                JsValue::String(JsString::from_vec(input.to_vec())),
                true,
                true,
                true,
            ),
        );
    if let Some(wl) = is_word_like {
        interp
            .get_object_cell_expect(obj_id)
            .borrow_mut()
            .insert_property(
                "isWordLike".to_string(),
                PropertyDescriptor::data(JsValue::Boolean(wl), true, true, true),
            );
    }
    let id = obj_id;
    JsValue::Object(crate::types::JsObject { id })
}

fn extract_segmenter_data(
    interp: &mut Interpreter,
    this: &JsValue,
) -> Result<(String, String), JsValue> {
    if let JsValue::Object(o) = this
        && let Some(obj) = interp.get_object_cell(o.id)
    {
        let b = obj.borrow();
        if let Some(IntlData::Segmenter {
            locale,
            granularity,
        }) = b.intl_data()
        {
            return Ok((locale.clone(), granularity.clone()));
        }
    }
    Err(interp.create_type_error("Intl.Segmenter method called on incompatible receiver"))
}

impl Interpreter {
    pub(crate) fn setup_intl_segmenter(&mut self, intl_obj_id: u64) {
        let proto_id = self.create_object_id();
        if let Some(op_id) = self.realm().object_prototype {
            self.get_object_cell_expect(proto_id)
                .borrow_mut()
                .prototype_id = Some(op_id);
        }
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .class_name = "Intl.Segmenter".to_string();

        // @@toStringTag
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_property(
                JsPropertyKey::well_known_symbol("toStringTag"),
                PropertyDescriptor {
                    value: Some(JsValue::String(JsString::from_str("Intl.Segmenter"))),
                    writable: Some(false),
                    enumerable: Some(false),
                    configurable: Some(true),
                    get: None,
                    set: None,
                },
            );

        // segment(string)
        let segment_fn = self.create_function(JsFunction::native(
            "segment".to_string(),
            1,
            |interp, this, args| {
                let (locale, granularity) = match extract_segmenter_data(interp, this) {
                    Ok(data) => data,
                    Err(e) => return Completion::Throw(e),
                };

                let str_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let input_js = match interp.to_js_string(&str_arg) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                let input_u16 = input_js.code_units.clone();

                let breaks = compute_break_points_utf16(&input_u16, &granularity);

                let segments_obj_id = interp.create_object_id();
                if let Some(op_id) = interp.realm().object_prototype {
                    interp
                        .get_object_cell_expect(segments_obj_id)
                        .borrow_mut()
                        .prototype_id = Some(op_id);
                }
                interp
                    .get_object_cell_expect(segments_obj_id)
                    .borrow_mut()
                    .class_name = "Segmenter Segments".to_string();

                interp
                    .get_object_cell_expect(segments_obj_id)
                    .borrow_mut()
                    .insert_property(
                        "[[SegmenterInput]]".to_string(),
                        PropertyDescriptor::data(JsValue::String(input_js), false, false, false),
                    );
                interp
                    .get_object_cell_expect(segments_obj_id)
                    .borrow_mut()
                    .insert_property(
                        "[[SegmenterGranularity]]".to_string(),
                        PropertyDescriptor::data(
                            JsValue::String(JsString::from_str(&granularity)),
                            false,
                            false,
                            false,
                        ),
                    );
                interp
                    .get_object_cell_expect(segments_obj_id)
                    .borrow_mut()
                    .insert_property(
                        "[[SegmenterLocale]]".to_string(),
                        PropertyDescriptor::data(
                            JsValue::String(JsString::from_str(&locale)),
                            false,
                            false,
                            false,
                        ),
                    );

                let breaks_vals: Vec<JsValue> =
                    breaks.iter().map(|&b| JsValue::Number(b as f64)).collect();
                let breaks_arr = interp.create_array(breaks_vals);
                interp
                    .get_object_cell_expect(segments_obj_id)
                    .borrow_mut()
                    .insert_property(
                        "[[SegmenterBreaks]]".to_string(),
                        PropertyDescriptor::data(breaks_arr, false, false, false),
                    );

                // containing(index) method
                let containing_fn = interp.create_function(JsFunction::native(
                    "containing".to_string(),
                    1,
                    |interp, this, args| {
                        // RequireInternalSlot(segments, [[SegmentsSegmenter]])
                        let is_segments = if let JsValue::Object(o) = this {
                            if let Some(obj) = interp.get_object_cell(o.id) {
                                let b = obj.borrow();
                                b.class_name == "Segmenter Segments"
                                    && b.properties.contains_key("[[SegmenterInput]]")
                            } else {
                                false
                            }
                        } else {
                            false
                        };
                        if !is_segments {
                            return Completion::Throw(interp.create_type_error(
                                "%Segments.prototype%.containing called on incompatible receiver",
                            ));
                        }

                        let idx_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                        let idx = match interp.to_number_value(&idx_arg) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        };

                        // ToInteger: NaN -> 0, -0 -> 0
                        let idx = if idx.is_nan() || idx == 0.0 {
                            0.0
                        } else {
                            idx.trunc()
                        };

                        if idx < 0.0 {
                            return Completion::Normal(JsValue::Undefined);
                        }
                        let idx = idx as usize;

                        let (input_u16, granularity, breaks) = if let JsValue::Object(o) = this {
                            if let Some(obj) = interp.get_object_cell(o.id) {
                                let b = obj.borrow();
                                let input_u16 = b
                                    .properties
                                    .get("[[SegmenterInput]]")
                                    .and_then(|pd| pd.value.as_ref())
                                    .and_then(|v| {
                                        if let JsValue::String(s) = v {
                                            Some(s.code_units.clone())
                                        } else {
                                            None
                                        }
                                    });
                                let granularity = b
                                    .properties
                                    .get("[[SegmenterGranularity]]")
                                    .and_then(|pd| pd.value.as_ref())
                                    .and_then(|v| {
                                        if let JsValue::String(s) = v {
                                            Some(s.to_rust_string())
                                        } else {
                                            None
                                        }
                                    });
                                let breaks_val = b
                                    .properties
                                    .get("[[SegmenterBreaks]]")
                                    .and_then(|pd| pd.value.clone());
                                drop(b);

                                let mut break_points = Vec::new();
                                if let Some(JsValue::Object(arr_obj)) = breaks_val
                                    && let Some(arr) = interp.get_object_cell(arr_obj.id)
                                {
                                    let ab = arr.borrow();
                                    if let Some(elems) = ab.array_elements() {
                                        for elem in elems {
                                            if let JsValue::Number(n) = elem {
                                                break_points.push(*n as usize);
                                            }
                                        }
                                    }
                                }

                                (input_u16, granularity, break_points)
                            } else {
                                (None, None, Vec::new())
                            }
                        } else {
                            (None, None, Vec::new())
                        };

                        let input_u16 = match input_u16 {
                            Some(s) => s,
                            None => return Completion::Normal(JsValue::Undefined),
                        };
                        let granularity = granularity.unwrap_or_else(|| "grapheme".to_string());

                        if idx >= input_u16.len() {
                            return Completion::Normal(JsValue::Undefined);
                        }

                        // Find the segment containing idx (UTF-16 index)
                        let mut seg_start = 0;
                        let mut seg_end = input_u16.len();
                        for w in breaks.windows(2) {
                            if w[0] <= idx && idx < w[1] {
                                seg_start = w[0];
                                seg_end = w[1];
                                break;
                            }
                        }

                        let segment = input_u16[seg_start..seg_end].to_vec();
                        let is_word_like = if granularity == "word" {
                            Some(compute_word_like_at_break_utf16(&input_u16, seg_end))
                        } else {
                            None
                        };

                        Completion::Normal(create_segment_object(
                            interp,
                            &segment,
                            seg_start,
                            &input_u16,
                            is_word_like,
                        ))
                    },
                ));
                interp
                    .get_object_cell_expect(segments_obj_id)
                    .borrow_mut()
                    .insert_builtin("containing".to_string(), containing_fn);

                // [Symbol.iterator]() method
                let granularity_clone = granularity.clone();
                let input_clone = input_u16.clone();
                let iter_fn = interp.create_function(JsFunction::native(
                    "[Symbol.iterator]".to_string(),
                    0,
                    move |interp, _this, _args| {
                        let segs = compute_segments(&input_clone, &granularity_clone);
                        let seg_data: Vec<(Vec<u16>, usize, bool)> = segs
                            .into_iter()
                            .map(|s| (s.segment, s.index, s.is_word_like.unwrap_or(false)))
                            .collect();

                        let iter_obj_id = interp.create_object_id();
                        if let Some(ip_id) = interp.realm().iterator_prototype {
                            interp
                                .get_object_cell_expect(iter_obj_id)
                                .borrow_mut()
                                .prototype_id = Some(ip_id);
                        }
                        interp
                            .get_object_cell_expect(iter_obj_id)
                            .borrow_mut()
                            .class_name = "Segmenter String Iterator".to_string();

                        let has_word_like = granularity_clone == "word";

                        interp.get_object_cell_expect(iter_obj_id).borrow_mut().kind =
                            crate::interpreter::types::ObjectKind::Iterator(
                                IteratorState::SegmentIterator {
                                    segments: seg_data,
                                    input: std::rc::Rc::new(input_clone.as_ref().clone()),
                                    position: 0,
                                    done: false,
                                },
                            );

                        interp
                            .get_object_cell_expect(iter_obj_id)
                            .borrow_mut()
                            .insert_property(
                                "[[HasWordLike]]".to_string(),
                                PropertyDescriptor::data(
                                    JsValue::Boolean(has_word_like),
                                    false,
                                    false,
                                    false,
                                ),
                            );

                        let next_fn = interp.create_function(JsFunction::native(
                            "next".to_string(),
                            0,
                            |interp, this, _args| {
                                if let JsValue::Object(o) = this
                                    && interp.get_object_cell(o.id).is_some()
                                {
                                    let iter_id = o.id;
                                    enum Step {
                                        Done,
                                        Yield {
                                            seg: Vec<u16>,
                                            idx: usize,
                                            input: std::rc::Rc<Vec<u16>>,
                                            wl: bool,
                                            has_word_like: bool,
                                        },
                                        NotIter,
                                    }
                                    let step = {
                                        let cell = interp.get_object_cell_expect(iter_id);
                                        let has_word_like = cell
                                            .borrow()
                                            .properties
                                            .get("[[HasWordLike]]")
                                            .and_then(|pd| pd.value.as_ref())
                                            .map(|v| matches!(v, JsValue::Boolean(true)))
                                            .unwrap_or(false);
                                        let mut b = cell.borrow_mut();
                                        if let Some(IteratorState::SegmentIterator {
                                            segments,
                                            input,
                                            position,
                                            done,
                                        }) = b.iterator_state_mut()
                                        {
                                            if *done || *position >= segments.len() {
                                                *done = true;
                                                Step::Done
                                            } else {
                                                let (seg, idx, wl) = segments[*position].clone();
                                                *position += 1;
                                                Step::Yield {
                                                    seg,
                                                    idx,
                                                    input: input.clone(),
                                                    wl,
                                                    has_word_like,
                                                }
                                            }
                                        } else {
                                            Step::NotIter
                                        }
                                    };
                                    match step {
                                        Step::Done => {
                                            return Completion::Normal(
                                                interp.create_iter_result_object(
                                                    JsValue::Undefined,
                                                    true,
                                                ),
                                            );
                                        }
                                        Step::Yield {
                                            seg,
                                            idx,
                                            input,
                                            wl,
                                            has_word_like,
                                        } => {
                                            let is_word_like =
                                                if has_word_like { Some(wl) } else { None };
                                            let seg_obj = create_segment_object(
                                                interp,
                                                &seg,
                                                idx,
                                                &input,
                                                is_word_like,
                                            );
                                            return Completion::Normal(
                                                interp.create_iter_result_object(seg_obj, false),
                                            );
                                        }
                                        Step::NotIter => {}
                                    }
                                }
                                Completion::Normal(
                                    interp.create_iter_result_object(JsValue::Undefined, true),
                                )
                            },
                        ));
                        interp
                            .get_object_cell_expect(iter_obj_id)
                            .borrow_mut()
                            .insert_builtin("next".to_string(), next_fn);

                        let iter_id = iter_obj_id;
                        Completion::Normal(JsValue::Object(crate::types::JsObject { id: iter_id }))
                    },
                ));

                interp
                    .get_object_cell_expect(segments_obj_id)
                    .borrow_mut()
                    .insert_property(
                        JsPropertyKey::well_known_symbol("iterator"),
                        PropertyDescriptor::data(iter_fn, true, false, true),
                    );

                let segments_id = segments_obj_id;
                Completion::Normal(JsValue::Object(crate::types::JsObject { id: segments_id }))
            },
        ));
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_builtin("segment".to_string(), segment_fn);

        // resolvedOptions()
        let resolved_fn = self.create_function(JsFunction::native(
            "resolvedOptions".to_string(),
            0,
            |interp, this, _args| {
                let (locale, granularity) = match extract_segmenter_data(interp, this) {
                    Ok(data) => data,
                    Err(e) => return Completion::Throw(e),
                };

                let result_id = interp.create_object_id();
                if let Some(op_id) = interp.realm().object_prototype {
                    interp
                        .get_object_cell_expect(result_id)
                        .borrow_mut()
                        .prototype_id = Some(op_id);
                }

                let props = vec![
                    ("locale", JsValue::String(JsString::from_str(&locale))),
                    (
                        "granularity",
                        JsValue::String(JsString::from_str(&granularity)),
                    ),
                ];
                for (key, val) in props {
                    interp
                        .get_object_cell_expect(result_id)
                        .borrow_mut()
                        .insert_property(
                            key.to_string(),
                            PropertyDescriptor::data(val, true, true, true),
                        );
                }

                Completion::Normal(JsValue::Object(crate::types::JsObject { id: result_id }))
            },
        ));
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_builtin("resolvedOptions".to_string(), resolved_fn);

        self.realm_mut().intl_segmenter_prototype = Some(proto_id);

        // --- Constructor ---
        let proto_val = JsValue::Object(crate::types::JsObject { id: proto_id });
        let proto_clone_id = proto_id;

        let segmenter_ctor = self.create_function(JsFunction::constructor(
            "Segmenter".to_string(),
            0,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("Constructor Intl.Segmenter requires 'new'"),
                    );
                }

                let locales_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let options_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);

                let requested = match interp.intl_canonicalize_locale_list(&locales_arg) {
                    Ok(list) => list,
                    Err(e) => return Completion::Throw(e),
                };

                let options = match interp.intl_get_options_object(&options_arg) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                let _locale_matcher = match interp.intl_get_option(
                    &options,
                    "localeMatcher",
                    &["lookup", "best fit"],
                    Some("best fit"),
                ) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                let granularity = match interp.intl_get_option(
                    &options,
                    "granularity",
                    &["grapheme", "word", "sentence"],
                    Some("grapheme"),
                ) {
                    Ok(Some(v)) => v,
                    Ok(None) => "grapheme".to_string(),
                    Err(e) => return Completion::Throw(e),
                };

                let locale = interp.intl_resolve_locale(&requested);

                let proto = match interp
                    .get_prototype_from_new_target_realm(|realm| realm.intl_segmenter_prototype)
                {
                    Ok(p) => p.unwrap_or(proto_clone_id),
                    Err(e) => return Completion::Throw(e),
                };
                let obj_id = interp.create_object_id();
                interp
                    .get_object_cell_expect(obj_id)
                    .borrow_mut()
                    .prototype_id = Some(proto);
                interp
                    .get_object_cell_expect(obj_id)
                    .borrow_mut()
                    .class_name = "Intl.Segmenter".to_string();
                interp.get_object_cell_expect(obj_id).borrow_mut().kind =
                    crate::interpreter::types::ObjectKind::Intl(Box::new(IntlData::Segmenter {
                        locale,
                        granularity,
                    }));

                Completion::Normal(JsValue::Object(crate::types::JsObject { id: obj_id }))
            },
        ));

        // Set Segmenter.prototype on constructor
        if let JsValue::Object(ctor_ref) = &segmenter_ctor
            && self.get_object_cell(ctor_ref.id).is_some()
        {
            let ctor_id = ctor_ref.id;
            self.get_object_cell_expect(ctor_id)
                .borrow_mut()
                .insert_property(
                    "prototype".to_string(),
                    PropertyDescriptor::data(proto_val.clone(), false, false, false),
                );

            // supportedLocalesOf static method
            let slof = self.create_function(JsFunction::native(
                "supportedLocalesOf".to_string(),
                1,
                |interp, _this, args| {
                    let locales = args.first().unwrap_or(&JsValue::Undefined);
                    let options = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                    let requested = match interp.intl_canonicalize_locale_list(locales) {
                        Ok(list) => list,
                        Err(e) => return Completion::Throw(e),
                    };
                    match interp.intl_supported_locales(&requested, &options) {
                        Ok(v) => Completion::Normal(v),
                        Err(e) => Completion::Throw(e),
                    }
                },
            ));
            self.get_object_cell_expect(ctor_id)
                .borrow_mut()
                .insert_builtin("supportedLocalesOf".to_string(), slof);
        }

        // Set constructor on prototype
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_property(
                "constructor".to_string(),
                PropertyDescriptor::data(segmenter_ctor.clone(), true, false, true),
            );

        // Register Intl.Segmenter on the Intl namespace
        self.get_object_cell_expect(intl_obj_id)
            .borrow_mut()
            .insert_property(
                "Segmenter".to_string(),
                PropertyDescriptor::data(segmenter_ctor, true, false, true),
            );
    }
}
