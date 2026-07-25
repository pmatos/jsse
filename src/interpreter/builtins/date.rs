use super::super::*;

/// The pure core shared by every `Date.prototype.get*` component accessor: given
/// a Date's time value `t`, an invalid Date (`t` is NaN) yields NaN without ever
/// consulting `component`; otherwise `component` is applied to `t` in local time
/// when `local`, or in UTC when not. `component` is one of the calendar/clock
/// extractors (`year_from_time`, `hour_from_time`, ...) — or a small adapter for
/// the odd ones out (`getTime`/`valueOf` pass `t` through, `getTimezoneOffset`
/// returns `(t - local_time(t)) / 60_000`, Annex B `getYear` subtracts 1900).
/// Interpreter-free so it is unit-testable without a running engine.
fn date_field_value(t: f64, local: bool, component: fn(f64) -> f64) -> f64 {
    if t.is_nan() {
        return f64::NAN;
    }
    component(if local { local_time(t) } else { t })
}

fn date_to_locale_string(
    interp: &mut Interpreter,
    this: &JsValue,
    args: &[JsValue],
    required: &str,
    defaults: &str,
) -> Completion {
    fn this_time_value_locale(interp: &Interpreter, this: &JsValue) -> Option<f64> {
        if let JsValue::Object(o) = this
            && let Some(obj) = interp.get_object_cell(o.id)
        {
            let b = obj.borrow();
            if b.class_name == "Date"
                && let Some(JsValue::Number(t)) = &b.primitive_value
            {
                return Some(*t);
            }
        }
        None
    }

    let tv = match this_time_value_locale(interp, this) {
        Some(t) => t,
        None => {
            let e = interp.create_type_error("this is not a Date object");
            return Completion::Throw(e);
        }
    };

    if tv.is_nan() {
        return Completion::Normal(JsValue::String(JsString::from_str("Invalid Date")));
    }

    let locales_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
    let options_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);

    // ToDateTimeOptions(options, required, defaults)
    let options_obj_id = if matches!(options_arg, JsValue::Undefined) {
        interp.create_object_id()
    } else {
        match interp.to_object(&options_arg) {
            Completion::Normal(v) => {
                if let JsValue::Object(o) = &v {
                    if let Some(src) = interp.get_object_cell(o.id) {
                        let pds: Vec<(JsPropertyKey, PropertyDescriptor)> = src
                            .borrow()
                            .properties
                            .iter()
                            .map(|(k, pd)| (k.clone(), pd.clone()))
                            .collect();
                        let new_obj_id = interp.create_object_id();
                        for (k, pd) in pds {
                            interp
                                .get_object_cell_expect(new_obj_id)
                                .borrow_mut()
                                .insert_property(k, pd);
                        }
                        new_obj_id
                    } else {
                        interp.create_object_id()
                    }
                } else {
                    interp.create_object_id()
                }
            }
            Completion::Throw(e) => return Completion::Throw(e),
            _ => interp.create_object_id(),
        }
    };

    let has_prop = |obj: &JsObjectData, name: &str| -> bool { obj.properties.contains_key(name) };

    let mut need_defaults = true;

    if required == "date" || required == "any" {
        for prop in &["weekday", "year", "month", "day"] {
            if has_prop(
                &interp.get_object_cell_expect(options_obj_id).borrow(),
                prop,
            ) {
                need_defaults = false;
                break;
            }
        }
    }
    if need_defaults && (required == "time" || required == "any") {
        for prop in &[
            "dayPeriod",
            "hour",
            "minute",
            "second",
            "fractionalSecondDigits",
        ] {
            if has_prop(
                &interp.get_object_cell_expect(options_obj_id).borrow(),
                prop,
            ) {
                need_defaults = false;
                break;
            }
        }
    }
    if need_defaults
        && (has_prop(
            &interp.get_object_cell_expect(options_obj_id).borrow(),
            "dateStyle",
        ) || has_prop(
            &interp.get_object_cell_expect(options_obj_id).borrow(),
            "timeStyle",
        ))
    {
        need_defaults = false;
    }

    if need_defaults {
        let numeric = JsValue::String(JsString::from_str("numeric"));
        if defaults == "date" || defaults == "all" {
            interp
                .get_object_cell_expect(options_obj_id)
                .borrow_mut()
                .insert_property(
                    "year".to_string(),
                    PropertyDescriptor::data(numeric.clone(), true, true, true),
                );
            interp
                .get_object_cell_expect(options_obj_id)
                .borrow_mut()
                .insert_property(
                    "month".to_string(),
                    PropertyDescriptor::data(numeric.clone(), true, true, true),
                );
            interp
                .get_object_cell_expect(options_obj_id)
                .borrow_mut()
                .insert_property(
                    "day".to_string(),
                    PropertyDescriptor::data(numeric.clone(), true, true, true),
                );
        }
        if defaults == "time" || defaults == "all" {
            interp
                .get_object_cell_expect(options_obj_id)
                .borrow_mut()
                .insert_property(
                    "hour".to_string(),
                    PropertyDescriptor::data(numeric.clone(), true, true, true),
                );
            interp
                .get_object_cell_expect(options_obj_id)
                .borrow_mut()
                .insert_property(
                    "minute".to_string(),
                    PropertyDescriptor::data(numeric.clone(), true, true, true),
                );
            interp
                .get_object_cell_expect(options_obj_id)
                .borrow_mut()
                .insert_property(
                    "second".to_string(),
                    PropertyDescriptor::data(numeric, true, true, true),
                );
        }
    }

    let opt_val = JsValue::Object(crate::types::JsObject { id: options_obj_id });

    // Use the built-in DateTimeFormat constructor directly (not through user-visible Intl property)
    let dtf_val = match interp.realm().intl_date_time_format_ctor.clone() {
        Some(v) => v,
        None => {
            return Completion::Normal(JsValue::String(JsString::from_str(&format_date_string(
                tv,
            ))));
        }
    };

    // Call new Intl.DateTimeFormat(locales, options)
    let dtf_instance = match interp.construct(&dtf_val, &[locales_arg, opt_val]) {
        Completion::Normal(v) => v,
        Completion::Throw(e) => return Completion::Throw(e),
        _ => return Completion::Normal(JsValue::Undefined),
    };

    // Get the format function from the DateTimeFormat instance
    if let JsValue::Object(dtf_obj) = &dtf_instance {
        let format_val = match interp.get_object_property(dtf_obj.id, "format", &dtf_instance) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Completion::Throw(e),
            _ => JsValue::Undefined,
        };

        // Call format(tv)
        let date_val = JsValue::Number(tv);
        match interp.call_function(&format_val, &dtf_instance, &[date_val]) {
            Completion::Normal(v) => Completion::Normal(v),
            Completion::Throw(e) => Completion::Throw(e),
            _ => Completion::Normal(JsValue::Undefined),
        }
    } else {
        Completion::Normal(JsValue::String(JsString::from_str(&format_date_string(tv))))
    }
}

impl Interpreter {
    pub(crate) fn setup_date_builtin(&mut self) {
        let proto_id = self.create_object_id();
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .class_name = "Date".to_string();
        // Date.prototype does NOT have [[DateValue]] per spec

        fn this_time_value(interp: &Interpreter, this: &JsValue) -> Option<f64> {
            if let JsValue::Object(o) = this
                && let Some(obj) = interp.get_object_cell(o.id)
            {
                let b = obj.borrow();
                if b.class_name == "Date"
                    && let Some(JsValue::Number(t)) = &b.primitive_value
                {
                    return Some(*t);
                }
            }
            None
        }

        // Guard wrapper around `date_field_value`: brand-check `this` as a Date
        // (throwing the standard TypeError otherwise), then delegate the NaN /
        // local-vs-UTC / component logic to the pure core. Every `get*` component
        // accessor is one call to this.
        fn date_field(
            interp: &mut Interpreter,
            this: &JsValue,
            local: bool,
            component: fn(f64) -> f64,
        ) -> Completion {
            match this_time_value(interp, this) {
                Some(t) => {
                    Completion::Normal(JsValue::Number(date_field_value(t, local, component)))
                }
                None => {
                    let e = interp.create_type_error("this is not a Date object");
                    Completion::Throw(e)
                }
            }
        }

        fn set_date_value(interp: &Interpreter, this: &JsValue, v: f64) {
            if let JsValue::Object(o) = this
                && let Some(obj) = interp.get_object_cell(o.id)
            {
                obj.borrow_mut().primitive_value = Some(JsValue::Number(v));
            }
        }

        fn to_num(interp: &mut Interpreter, val: &JsValue) -> Result<f64, JsValue> {
            interp.to_number_value(val)
        }

        // Every `set*` method reads its Nth argument (falling back to a
        // caller-supplied default, e.g. the current time's own component,
        // when the argument is absent) via ToNumber -- shared here so the
        // cascading-default logic isn't hand-rolled per component per method.
        fn arg_num_or(
            interp: &mut Interpreter,
            args: &[JsValue],
            idx: usize,
            default: f64,
        ) -> Result<f64, JsValue> {
            match args.get(idx) {
                Some(a) => to_num(interp, a),
                None => Ok(default),
            }
        }

        // Shared final step of every `set*` method: combine day + time,
        // clip, store on `this`, and return the new time value.
        fn finish_set(
            interp: &Interpreter,
            this: &JsValue,
            day: f64,
            time: f64,
            is_local: bool,
        ) -> Completion {
            let v = make_date_clipped(day, time, is_local);
            set_date_value(interp, this, v);
            Completion::Normal(JsValue::Number(v))
        }

        // Getter methods
        #[allow(clippy::type_complexity)]
        let methods: Vec<(
            &str,
            usize,
            Rc<dyn Fn(&mut Interpreter, &JsValue, &[JsValue]) -> Completion>,
        )> = vec![
            // getTime / valueOf return the (finite) time value straight through;
            // date_field's NaN short-circuit yields NaN for an invalid Date, which
            // is the value they would have returned anyway.
            (
                "getTime",
                0,
                Rc::new(|interp, this, _args| date_field(interp, this, false, |t| t)),
            ),
            (
                "valueOf",
                0,
                Rc::new(|interp, this, _args| date_field(interp, this, false, |t| t)),
            ),
            // Local-time component accessors.
            (
                "getFullYear",
                0,
                Rc::new(|interp, this, _args| date_field(interp, this, true, year_from_time)),
            ),
            (
                "getMonth",
                0,
                Rc::new(|interp, this, _args| date_field(interp, this, true, month_from_time)),
            ),
            (
                "getDate",
                0,
                Rc::new(|interp, this, _args| date_field(interp, this, true, date_from_time)),
            ),
            (
                "getDay",
                0,
                Rc::new(|interp, this, _args| date_field(interp, this, true, week_day)),
            ),
            (
                "getHours",
                0,
                Rc::new(|interp, this, _args| date_field(interp, this, true, hour_from_time)),
            ),
            (
                "getMinutes",
                0,
                Rc::new(|interp, this, _args| date_field(interp, this, true, min_from_time)),
            ),
            (
                "getSeconds",
                0,
                Rc::new(|interp, this, _args| date_field(interp, this, true, sec_from_time)),
            ),
            (
                "getMilliseconds",
                0,
                Rc::new(|interp, this, _args| date_field(interp, this, true, ms_from_time)),
            ),
            // UTC component accessors (no local_time adjustment).
            (
                "getUTCFullYear",
                0,
                Rc::new(|interp, this, _args| date_field(interp, this, false, year_from_time)),
            ),
            (
                "getUTCMonth",
                0,
                Rc::new(|interp, this, _args| date_field(interp, this, false, month_from_time)),
            ),
            (
                "getUTCDate",
                0,
                Rc::new(|interp, this, _args| date_field(interp, this, false, date_from_time)),
            ),
            (
                "getUTCDay",
                0,
                Rc::new(|interp, this, _args| date_field(interp, this, false, week_day)),
            ),
            (
                "getUTCHours",
                0,
                Rc::new(|interp, this, _args| date_field(interp, this, false, hour_from_time)),
            ),
            (
                "getUTCMinutes",
                0,
                Rc::new(|interp, this, _args| date_field(interp, this, false, min_from_time)),
            ),
            (
                "getUTCSeconds",
                0,
                Rc::new(|interp, this, _args| date_field(interp, this, false, sec_from_time)),
            ),
            (
                "getUTCMilliseconds",
                0,
                Rc::new(|interp, this, _args| date_field(interp, this, false, ms_from_time)),
            ),
            // Minutes between UTC and local time: component sees the raw UTC t.
            (
                "getTimezoneOffset",
                0,
                Rc::new(|interp, this, _args| {
                    date_field(interp, this, false, |t| (t - local_time(t)) / 60_000.0)
                }),
            ),
            // Setter methods -- all use to_number_value for proper error propagation
            (
                "setTime",
                1,
                Rc::new(|interp, this, args| {
                    let Some(_) = this_time_value(interp, this) else {
                        let e = interp.create_type_error("this is not a Date object");
                        return Completion::Throw(e);
                    };
                    let v = match arg_num_or(interp, args, 0, f64::NAN) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let v = time_clip(v);
                    set_date_value(interp, this, v);
                    Completion::Normal(JsValue::Number(v))
                }),
            ),
            (
                "setMilliseconds",
                1,
                Rc::new(|interp, this, args| {
                    let Some(t) = this_time_value(interp, this) else {
                        let e = interp.create_type_error("this is not a Date object");
                        return Completion::Throw(e);
                    };
                    let ms = match arg_num_or(interp, args, 0, f64::NAN) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let lt = local_time(t);
                    let time =
                        make_time(hour_from_time(lt), min_from_time(lt), sec_from_time(lt), ms);
                    finish_set(interp, this, day(lt), time, true)
                }),
            ),
            (
                "setUTCMilliseconds",
                1,
                Rc::new(|interp, this, args| {
                    let Some(t) = this_time_value(interp, this) else {
                        let e = interp.create_type_error("this is not a Date object");
                        return Completion::Throw(e);
                    };
                    let ms = match arg_num_or(interp, args, 0, f64::NAN) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let time = make_time(hour_from_time(t), min_from_time(t), sec_from_time(t), ms);
                    finish_set(interp, this, day(t), time, false)
                }),
            ),
            (
                "setSeconds",
                2,
                Rc::new(|interp, this, args| {
                    let Some(t) = this_time_value(interp, this) else {
                        let e = interp.create_type_error("this is not a Date object");
                        return Completion::Throw(e);
                    };
                    let s = match arg_num_or(interp, args, 0, f64::NAN) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let ms_default = if t.is_nan() {
                        f64::NAN
                    } else {
                        ms_from_time(local_time(t))
                    };
                    let ms = match arg_num_or(interp, args, 1, ms_default) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let lt = local_time(t);
                    let time = make_time(hour_from_time(lt), min_from_time(lt), s, ms);
                    finish_set(interp, this, day(lt), time, true)
                }),
            ),
            (
                "setUTCSeconds",
                2,
                Rc::new(|interp, this, args| {
                    let Some(t) = this_time_value(interp, this) else {
                        let e = interp.create_type_error("this is not a Date object");
                        return Completion::Throw(e);
                    };
                    let s = match arg_num_or(interp, args, 0, f64::NAN) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let ms_default = if t.is_nan() {
                        f64::NAN
                    } else {
                        ms_from_time(t)
                    };
                    let ms = match arg_num_or(interp, args, 1, ms_default) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let time = make_time(hour_from_time(t), min_from_time(t), s, ms);
                    finish_set(interp, this, day(t), time, false)
                }),
            ),
            (
                "setMinutes",
                3,
                Rc::new(|interp, this, args| {
                    let Some(t) = this_time_value(interp, this) else {
                        let e = interp.create_type_error("this is not a Date object");
                        return Completion::Throw(e);
                    };
                    let m = match arg_num_or(interp, args, 0, f64::NAN) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let s_default = if t.is_nan() {
                        f64::NAN
                    } else {
                        sec_from_time(local_time(t))
                    };
                    let s = match arg_num_or(interp, args, 1, s_default) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let ms_default = if t.is_nan() {
                        f64::NAN
                    } else {
                        ms_from_time(local_time(t))
                    };
                    let ms = match arg_num_or(interp, args, 2, ms_default) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let lt = local_time(t);
                    let time = make_time(hour_from_time(lt), m, s, ms);
                    finish_set(interp, this, day(lt), time, true)
                }),
            ),
            (
                "setUTCMinutes",
                3,
                Rc::new(|interp, this, args| {
                    let Some(t) = this_time_value(interp, this) else {
                        let e = interp.create_type_error("this is not a Date object");
                        return Completion::Throw(e);
                    };
                    let m = match arg_num_or(interp, args, 0, f64::NAN) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let s_default = if t.is_nan() {
                        f64::NAN
                    } else {
                        sec_from_time(t)
                    };
                    let s = match arg_num_or(interp, args, 1, s_default) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let ms_default = if t.is_nan() {
                        f64::NAN
                    } else {
                        ms_from_time(t)
                    };
                    let ms = match arg_num_or(interp, args, 2, ms_default) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let time = make_time(hour_from_time(t), m, s, ms);
                    finish_set(interp, this, day(t), time, false)
                }),
            ),
            (
                "setHours",
                4,
                Rc::new(|interp, this, args| {
                    let Some(t) = this_time_value(interp, this) else {
                        let e = interp.create_type_error("this is not a Date object");
                        return Completion::Throw(e);
                    };
                    let h = match arg_num_or(interp, args, 0, f64::NAN) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let m_default = if t.is_nan() {
                        f64::NAN
                    } else {
                        min_from_time(local_time(t))
                    };
                    let m = match arg_num_or(interp, args, 1, m_default) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let s_default = if t.is_nan() {
                        f64::NAN
                    } else {
                        sec_from_time(local_time(t))
                    };
                    let s = match arg_num_or(interp, args, 2, s_default) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let ms_default = if t.is_nan() {
                        f64::NAN
                    } else {
                        ms_from_time(local_time(t))
                    };
                    let ms = match arg_num_or(interp, args, 3, ms_default) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let lt = local_time(t);
                    let time = make_time(h, m, s, ms);
                    finish_set(interp, this, day(lt), time, true)
                }),
            ),
            (
                "setUTCHours",
                4,
                Rc::new(|interp, this, args| {
                    let Some(t) = this_time_value(interp, this) else {
                        let e = interp.create_type_error("this is not a Date object");
                        return Completion::Throw(e);
                    };
                    let h = match arg_num_or(interp, args, 0, f64::NAN) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let m_default = if t.is_nan() {
                        f64::NAN
                    } else {
                        min_from_time(t)
                    };
                    let m = match arg_num_or(interp, args, 1, m_default) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let s_default = if t.is_nan() {
                        f64::NAN
                    } else {
                        sec_from_time(t)
                    };
                    let s = match arg_num_or(interp, args, 2, s_default) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let ms_default = if t.is_nan() {
                        f64::NAN
                    } else {
                        ms_from_time(t)
                    };
                    let ms = match arg_num_or(interp, args, 3, ms_default) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let time = make_time(h, m, s, ms);
                    finish_set(interp, this, day(t), time, false)
                }),
            ),
            (
                "setDate",
                1,
                Rc::new(|interp, this, args| {
                    let Some(t) = this_time_value(interp, this) else {
                        let e = interp.create_type_error("this is not a Date object");
                        return Completion::Throw(e);
                    };
                    let dt = match arg_num_or(interp, args, 0, f64::NAN) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let lt = local_time(t);
                    let new_date = make_day(year_from_time(lt), month_from_time(lt), dt);
                    finish_set(interp, this, new_date, time_within_day(lt), true)
                }),
            ),
            (
                "setUTCDate",
                1,
                Rc::new(|interp, this, args| {
                    let Some(t) = this_time_value(interp, this) else {
                        let e = interp.create_type_error("this is not a Date object");
                        return Completion::Throw(e);
                    };
                    let dt = match arg_num_or(interp, args, 0, f64::NAN) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let new_date = make_day(year_from_time(t), month_from_time(t), dt);
                    finish_set(interp, this, new_date, time_within_day(t), false)
                }),
            ),
            (
                "setMonth",
                2,
                Rc::new(|interp, this, args| {
                    let Some(t) = this_time_value(interp, this) else {
                        let e = interp.create_type_error("this is not a Date object");
                        return Completion::Throw(e);
                    };
                    let m = match arg_num_or(interp, args, 0, f64::NAN) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let dt_default = if t.is_nan() {
                        f64::NAN
                    } else {
                        date_from_time(local_time(t))
                    };
                    let dt = match arg_num_or(interp, args, 1, dt_default) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let lt = local_time(t);
                    let new_date = make_day(year_from_time(lt), m, dt);
                    finish_set(interp, this, new_date, time_within_day(lt), true)
                }),
            ),
            (
                "setUTCMonth",
                2,
                Rc::new(|interp, this, args| {
                    let Some(t) = this_time_value(interp, this) else {
                        let e = interp.create_type_error("this is not a Date object");
                        return Completion::Throw(e);
                    };
                    let m = match arg_num_or(interp, args, 0, f64::NAN) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let dt_default = if t.is_nan() {
                        f64::NAN
                    } else {
                        date_from_time(t)
                    };
                    let dt = match arg_num_or(interp, args, 1, dt_default) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let new_date = make_day(year_from_time(t), m, dt);
                    finish_set(interp, this, new_date, time_within_day(t), false)
                }),
            ),
            (
                "setFullYear",
                3,
                Rc::new(|interp, this, args| {
                    let Some(t) = this_time_value(interp, this) else {
                        let e = interp.create_type_error("this is not a Date object");
                        return Completion::Throw(e);
                    };
                    let y = match arg_num_or(interp, args, 0, f64::NAN) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    // Per spec: if t is NaN, set t to +0; otherwise set t to LocalTime(t)
                    let lt = if t.is_nan() { 0.0 } else { local_time(t) };
                    let m = match arg_num_or(interp, args, 1, month_from_time(lt)) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let dt = match arg_num_or(interp, args, 2, date_from_time(lt)) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let new_date = make_day(y, m, dt);
                    finish_set(interp, this, new_date, time_within_day(lt), true)
                }),
            ),
            (
                "setUTCFullYear",
                3,
                Rc::new(|interp, this, args| {
                    let Some(t) = this_time_value(interp, this) else {
                        let e = interp.create_type_error("this is not a Date object");
                        return Completion::Throw(e);
                    };
                    // Per spec: NaN check before ToNumber for setUTCFullYear
                    let t_adj = if t.is_nan() { 0.0 } else { t };
                    let y = match arg_num_or(interp, args, 0, f64::NAN) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let m = match arg_num_or(interp, args, 1, month_from_time(t_adj)) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let dt = match arg_num_or(interp, args, 2, date_from_time(t_adj)) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let new_date = make_day(y, m, dt);
                    finish_set(interp, this, new_date, time_within_day(t_adj), false)
                }),
            ),
            // String formatting methods
            (
                "toString",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) if t.is_nan() => {
                        Completion::Normal(JsValue::String(JsString::from_str("Invalid Date")))
                    }
                    Some(t) => Completion::Normal(JsValue::String(JsString::from_str(
                        &format_date_string(t),
                    ))),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
                }),
            ),
            (
                "toDateString",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) if t.is_nan() => {
                        Completion::Normal(JsValue::String(JsString::from_str("Invalid Date")))
                    }
                    Some(t) => Completion::Normal(JsValue::String(JsString::from_str(
                        &format_date_only_string(t),
                    ))),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
                }),
            ),
            (
                "toTimeString",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) if t.is_nan() => {
                        Completion::Normal(JsValue::String(JsString::from_str("Invalid Date")))
                    }
                    Some(t) => Completion::Normal(JsValue::String(JsString::from_str(
                        &format_time_only_string(t),
                    ))),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
                }),
            ),
            (
                "toISOString",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) if !t.is_finite() => {
                        let e = interp.create_range_error("Invalid time value");
                        Completion::Throw(e)
                    }
                    Some(t) => Completion::Normal(JsValue::String(JsString::from_str(
                        &format_iso_string(t),
                    ))),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
                }),
            ),
            (
                "toUTCString",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) if t.is_nan() => {
                        Completion::Normal(JsValue::String(JsString::from_str("Invalid Date")))
                    }
                    Some(t) => Completion::Normal(JsValue::String(JsString::from_str(
                        &format_utc_string(t),
                    ))),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
                }),
            ),
            // toJSON per spec:
            // 1. Let O be ? ToObject(this value).
            // 2. Let tv be ? ToPrimitive(O, number).
            // 3. If Type(tv) is Number and tv is not finite, return null.
            // 4. Return ? Invoke(O, "toISOString").
            (
                "toJSON",
                1,
                Rc::new(|interp, this, _args| {
                    let o = match interp.to_object(this) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => return Completion::Normal(JsValue::Undefined),
                    };
                    let tv = match interp.to_primitive(&o, "number") {
                        Ok(v) => v,
                        Err(e) => return Completion::Throw(e),
                    };
                    if let JsValue::Number(n) = &tv
                        && !n.is_finite()
                    {
                        return Completion::Normal(JsValue::Null);
                    }
                    if let JsValue::Object(obj_ref) = &o {
                        let to_iso = interp.get_object_property(obj_ref.id, "toISOString", &o);
                        match to_iso {
                            Completion::Normal(func) => {
                                if let JsValue::Object(fo) = &func
                                    && interp
                                        .get_object_cell(fo.id)
                                        .map(|obj| obj.borrow().callable.is_some())
                                        .unwrap_or(false)
                                {
                                    return interp.call_function(&func, &o, &[]);
                                }
                                let e = interp.create_type_error("toISOString is not a function");
                                Completion::Throw(e)
                            }
                            Completion::Throw(e) => Completion::Throw(e),
                            _ => {
                                let e = interp.create_type_error("toISOString is not a function");
                                Completion::Throw(e)
                            }
                        }
                    } else {
                        let e = interp.create_type_error("toISOString is not a function");
                        Completion::Throw(e)
                    }
                }),
            ),
            (
                "toLocaleDateString",
                0,
                Rc::new(|interp, this, args| {
                    date_to_locale_string(interp, this, args, "date", "date")
                }),
            ),
            (
                "toLocaleString",
                0,
                Rc::new(|interp, this, args| {
                    date_to_locale_string(interp, this, args, "any", "all")
                }),
            ),
            (
                "toLocaleTimeString",
                0,
                Rc::new(|interp, this, args| {
                    date_to_locale_string(interp, this, args, "time", "time")
                }),
            ),
            (
                "toTemporalInstant",
                0,
                Rc::new(|interp, this, _args| {
                    // §21.4.4.45 Date.prototype.toTemporalInstant()
                    let t = this_time_value(interp, this);
                    let Some(t) = t else {
                        let e = interp.create_type_error("this is not a Date object");
                        return Completion::Throw(e);
                    };
                    if t.is_nan() {
                        let e = interp.create_range_error("Invalid time value");
                        return Completion::Throw(e);
                    }
                    let ms = num_bigint::BigInt::from(t as i64);
                    let ns = ms * num_bigint::BigInt::from(1_000_000i64);
                    let obj_id = interp.create_object_id();
                    interp
                        .get_object_cell_expect(obj_id)
                        .borrow_mut()
                        .class_name = "Temporal.Instant".to_string();
                    if let Some(proto_id) = interp.realm().temporal_instant_prototype {
                        interp
                            .get_object_cell_expect(obj_id)
                            .borrow_mut()
                            .prototype_id = Some(proto_id);
                    }
                    interp.get_object_cell_expect(obj_id).borrow_mut().kind =
                        crate::interpreter::types::ObjectKind::Temporal(
                            crate::interpreter::types::TemporalData::Instant {
                                epoch_nanoseconds: ns,
                            },
                        );
                    let id = obj_id;
                    Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
                }),
            ),
        ];

        for (name, arity, func) in methods {
            let fn_val =
                self.create_function(JsFunction::Native(name.to_string(), arity, func, false));
            self.get_object_cell_expect(proto_id)
                .borrow_mut()
                .insert_builtin(name.to_string(), fn_val);
        }

        // Symbol.toPrimitive per spec:
        // 1. Let O be the this value.
        // 2. If Type(O) is not Object, throw TypeError.
        // 3. If hint is "string" or "default", let tryFirst be "string".
        // 4. Else if hint is "number", let tryFirst be "number".
        // 5. Else throw TypeError.
        // 6. Return ? OrdinaryToPrimitive(O, tryFirst).
        let to_prim_fn = self.create_function(JsFunction::native(
            "[Symbol.toPrimitive]".to_string(),
            1,
            |interp, this, args| {
                let JsValue::Object(_) = this else {
                    let e = interp.create_type_error("this is not an object");
                    return Completion::Throw(e);
                };
                let hint = args.first().map(to_js_string).unwrap_or_default();
                let try_first = match hint.as_str() {
                    "string" | "default" => "string",
                    "number" => "number",
                    _ => {
                        let e = interp.create_type_error("Invalid hint");
                        return Completion::Throw(e);
                    }
                };
                // Inline OrdinaryToPrimitive to avoid infinite recursion
                // (to_primitive() checks @@toPrimitive which would call us again)
                let JsValue::Object(o) = this else {
                    return Completion::Normal(this.clone());
                };
                let methods = if try_first == "string" {
                    ["toString", "valueOf"]
                } else {
                    ["valueOf", "toString"]
                };
                for method_name in &methods {
                    let method_val = match interp.get_object_property(o.id, method_name, this) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => JsValue::Undefined,
                    };
                    if let JsValue::Object(fo) = &method_val
                        && interp
                            .get_object_cell(fo.id)
                            .map(|o| o.borrow().callable.is_some())
                            .unwrap_or(false)
                    {
                        let result = interp.call_function(&method_val, this, &[]);
                        match result {
                            Completion::Normal(v) if !matches!(v, JsValue::Object(_)) => {
                                return Completion::Normal(v);
                            }
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => {}
                        }
                    }
                }
                let e = interp.create_type_error("Cannot convert object to primitive value");
                Completion::Throw(e)
            },
        ));
        if let Some(key) = self.get_symbol_key("toPrimitive") {
            self.get_object_cell_expect(proto_id)
                .borrow_mut()
                .insert_property(
                    key,
                    PropertyDescriptor::data(to_prim_fn, false, false, true),
                );
        }

        // Date constructor
        let date_proto_clone_id = proto_id;
        let date_ctor = self.create_function(JsFunction::constructor(
            "Date".to_string(),
            7,
            move |interp, this, args| {
                if interp.new_target.is_none() {
                    let t = now_ms();
                    return Completion::Normal(JsValue::String(JsString::from_str(
                        &format_date_string(t),
                    )));
                }

                let time_val = if args.is_empty() {
                    now_ms()
                } else if args.len() == 1 {
                    let v = &args[0];
                    if let JsValue::Object(o) = v
                        && let Some(obj) = interp.get_object_cell(o.id)
                        && obj.borrow().class_name == "Date"
                        && obj.borrow().primitive_value.is_some()
                    {
                        if let Some(JsValue::Number(t)) = obj.borrow().primitive_value.clone() {
                            t
                        } else {
                            f64::NAN
                        }
                    } else {
                        // ToPrimitive(value) with hint "default"
                        let prim = match interp.to_primitive(v, "default") {
                            Ok(p) => p,
                            Err(e) => return Completion::Throw(e),
                        };
                        if let JsValue::String(_) = &prim {
                            parse_date_string(&to_js_string(&prim))
                        } else {
                            match interp.to_number_value(&prim) {
                                Ok(n) => time_clip(n),
                                Err(e) => return Completion::Throw(e),
                            }
                        }
                    }
                } else {
                    let y = match interp.to_number_value(&args[0]) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let m = match args.get(1) {
                        Some(a) => match interp.to_number_value(a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => 0.0,
                    };
                    let dt = match args.get(2) {
                        Some(a) => match interp.to_number_value(a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => 1.0,
                    };
                    let h = match args.get(3) {
                        Some(a) => match interp.to_number_value(a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => 0.0,
                    };
                    let min = match args.get(4) {
                        Some(a) => match interp.to_number_value(a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => 0.0,
                    };
                    let s = match args.get(5) {
                        Some(a) => match interp.to_number_value(a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => 0.0,
                    };
                    let ms = match args.get(6) {
                        Some(a) => match interp.to_number_value(a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => 0.0,
                    };
                    let yr = if !y.is_nan() {
                        let yi = y.trunc();
                        if (0.0..=99.0).contains(&yi) {
                            1900.0 + yi
                        } else {
                            y
                        }
                    } else {
                        y
                    };
                    let d = make_day(yr, m, dt);
                    let time = make_time(h, min, s, ms);
                    time_clip(utc_time(make_date(d, time)))
                };

                if let JsValue::Object(o) = this
                    && interp.get_object_cell(o.id).is_some()
                {
                    // OrdinaryCreateFromConstructor — realm-aware prototype
                    let proto = match interp
                        .get_prototype_from_new_target_realm(|realm| realm.date_prototype)
                    {
                        Ok(p) => p.unwrap_or(date_proto_clone_id),
                        Err(e) => return Completion::Throw(e),
                    };
                    let mut b = interp.get_object_cell_expect(o.id).borrow_mut();
                    b.class_name = "Date".to_string();
                    b.primitive_value = Some(JsValue::Number(time_val));
                    b.prototype_id = Some(proto);
                }
                Completion::Normal(this.clone())
            },
        ));

        if let JsValue::Object(o) = &date_ctor
            && self.get_object_cell(o.id).is_some()
        {
            let date_ctor_id = o.id;
            self.get_object_cell_expect(date_ctor_id)
                .borrow_mut()
                .insert_property(
                    "length".to_string(),
                    PropertyDescriptor::data(JsValue::Number(7.0), false, false, true),
                );

            let now_fn = self.create_function(JsFunction::native(
                "now".to_string(),
                0,
                |_interp, _this, _args| Completion::Normal(JsValue::Number(now_ms().floor())),
            ));
            self.get_object_cell_expect(date_ctor_id)
                .borrow_mut()
                .insert_builtin("now".to_string(), now_fn);

            let parse_fn = self.create_function(JsFunction::native(
                "parse".to_string(),
                1,
                |interp, _this, args| {
                    let s = match args.first() {
                        Some(a) => match interp.to_string_value(a) {
                            Ok(s) => s,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => String::new(),
                    };
                    Completion::Normal(JsValue::Number(parse_date_string(&s)))
                },
            ));
            self.get_object_cell_expect(date_ctor_id)
                .borrow_mut()
                .insert_builtin("parse".to_string(), parse_fn);

            let utc_fn = self.create_function(JsFunction::native(
                "UTC".to_string(),
                7,
                |interp, _this, args| {
                    let y = match args.first() {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => f64::NAN,
                    };
                    let m = match args.get(1) {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => 0.0,
                    };
                    let dt = match args.get(2) {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => 1.0,
                    };
                    let h = match args.get(3) {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => 0.0,
                    };
                    let min = match args.get(4) {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => 0.0,
                    };
                    let s = match args.get(5) {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => 0.0,
                    };
                    let ms = match args.get(6) {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => 0.0,
                    };
                    let yr = if !y.is_nan() {
                        let yi = y.trunc();
                        if (0.0..=99.0).contains(&yi) {
                            1900.0 + yi
                        } else {
                            y
                        }
                    } else {
                        y
                    };
                    let d = make_day(yr, m, dt);
                    let time = make_time(h, min, s, ms);
                    Completion::Normal(JsValue::Number(time_clip(make_date(d, time))))
                },
            ));
            self.get_object_cell_expect(date_ctor_id)
                .borrow_mut()
                .insert_builtin("UTC".to_string(), utc_fn);

            let proto_val = JsValue::Object(crate::types::JsObject { id: proto_id });
            self.get_object_cell_expect(date_ctor_id)
                .borrow_mut()
                .insert_property(
                    "prototype".to_string(),
                    PropertyDescriptor::data(proto_val, false, false, false),
                );
        }

        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_builtin("constructor".to_string(), date_ctor.clone());

        self.realm()
            .global_env
            .borrow_mut()
            .declare("Date", BindingKind::Var);
        let env = self.realm().global_env.clone();
        let _ = self.env_set(&env, "Date", date_ctor);

        // Annex B: getYear() -- local full year offset by 1900.
        let get_year_fn = self.create_function(JsFunction::Native(
            "getYear".to_string(),
            0,
            Rc::new(|interp, this, _args| {
                date_field(interp, this, true, |t| year_from_time(t) - 1900.0)
            }),
            false,
        ));
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_builtin("getYear".to_string(), get_year_fn);

        // Annex B: setYear(year)
        let set_year_fn = self.create_function(JsFunction::Native(
            "setYear".to_string(),
            1,
            Rc::new(|interp, this, args| {
                let Some(t) = this_time_value(interp, this) else {
                    let e = interp.create_type_error("this is not a Date object");
                    return Completion::Throw(e);
                };
                let y = match arg_num_or(interp, args, 0, f64::NAN) {
                    Ok(n) => n,
                    Err(e) => return Completion::Throw(e),
                };
                if y.is_nan() {
                    set_date_value(interp, this, f64::NAN);
                    return Completion::Normal(JsValue::Number(f64::NAN));
                }
                let yi = y as i64;
                let yr = if (0..=99).contains(&yi) {
                    1900.0 + yi as f64
                } else {
                    y
                };
                let t = if t.is_nan() { 0.0 } else { local_time(t) };
                let new_date = make_day(yr, month_from_time(t), date_from_time(t));
                finish_set(interp, this, new_date, time_within_day(t), true)
            }),
            false,
        ));
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_builtin("setYear".to_string(), set_year_fn);

        // Annex B: toGMTString() -- alias for toUTCString()
        let to_gmt = self.get_property_on_id(proto_id, "toUTCString");
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_builtin("toGMTString".to_string(), to_gmt);

        self.realm_mut().date_prototype = Some(proto_id);
    }

    pub(crate) fn create_range_error(&mut self, msg: &str) -> JsValue {
        self.create_error("RangeError", msg)
    }

    pub(crate) fn create_reference_error(&mut self, msg: &str) -> JsValue {
        self.create_error("ReferenceError", msg)
    }

    pub(crate) fn create_error(&mut self, name: &str, msg: &str) -> JsValue {
        let ctor = self.get_global_var(name);
        let error_proto_id: Option<u64> = ctor.and_then(|v| {
            if let JsValue::Object(o) = &v {
                let pv = self.get_property_on_id(o.id, "prototype");
                if let JsValue::Object(p) = &pv {
                    Some(p.id)
                } else {
                    None
                }
            } else {
                None
            }
        });
        let obj_id = self.create_object_id();
        {
            let mut o = self.get_object_cell_expect(obj_id).borrow_mut();
            o.class_name = name.to_string();
            if let Some(proto_id) = error_proto_id {
                o.prototype_id = Some(proto_id);
            }
            o.insert_builtin(
                "message".to_string(),
                JsValue::String(JsString::from_str(msg)),
            );
            o.insert_builtin(
                "name".to_string(),
                JsValue::String(JsString::from_str(name)),
            );
        }
        let id = obj_id;
        JsValue::Object(crate::types::JsObject { id })
    }
}

#[cfg(test)]
mod date_field_tests {
    use super::*;

    // Independent known-good values, not recomputed the way the code does.
    // Epoch 0 = 1970-01-01T00:00:00.000Z, a Thursday (week_day 4, 0=Sunday).
    const EPOCH: f64 = 0.0;
    // 2021-01-01T00:00:00.000Z (1_609_459_200 s * 1000), a Friday (week_day 5).
    const Y2021: f64 = 1_609_459_200_000.0;
    // 1970-01-01T13:37:42.123Z: 13h37m42.123s past epoch.
    const TOD: f64 = 49_062_123.0;

    #[test]
    fn utc_calendar_components_at_epoch() {
        assert_eq!(date_field_value(EPOCH, false, year_from_time), 1970.0);
        assert_eq!(date_field_value(EPOCH, false, month_from_time), 0.0);
        assert_eq!(date_field_value(EPOCH, false, date_from_time), 1.0);
        assert_eq!(date_field_value(EPOCH, false, week_day), 4.0);
        assert_eq!(date_field_value(EPOCH, false, hour_from_time), 0.0);
        assert_eq!(date_field_value(EPOCH, false, min_from_time), 0.0);
        assert_eq!(date_field_value(EPOCH, false, sec_from_time), 0.0);
        assert_eq!(date_field_value(EPOCH, false, ms_from_time), 0.0);
    }

    #[test]
    fn utc_calendar_components_at_2021() {
        assert_eq!(date_field_value(Y2021, false, year_from_time), 2021.0);
        assert_eq!(date_field_value(Y2021, false, month_from_time), 0.0);
        assert_eq!(date_field_value(Y2021, false, date_from_time), 1.0);
        assert_eq!(date_field_value(Y2021, false, week_day), 5.0);
    }

    #[test]
    fn utc_time_of_day_components() {
        assert_eq!(date_field_value(TOD, false, hour_from_time), 13.0);
        assert_eq!(date_field_value(TOD, false, min_from_time), 37.0);
        assert_eq!(date_field_value(TOD, false, sec_from_time), 42.0);
        assert_eq!(date_field_value(TOD, false, ms_from_time), 123.0);
    }

    #[test]
    fn nan_time_short_circuits_before_component() {
        // A component that would panic if ever called proves the short-circuit.
        fn boom(_: f64) -> f64 {
            panic!("component must not run for an invalid Date");
        }
        assert!(date_field_value(f64::NAN, false, boom).is_nan());
        assert!(date_field_value(f64::NAN, true, boom).is_nan());
        assert!(date_field_value(f64::NAN, false, year_from_time).is_nan());
    }

    #[test]
    fn getyear_component_offsets_by_1900() {
        // Annex B getYear = year_from_time(...) - 1900.
        assert_eq!(
            date_field_value(EPOCH, false, |t| year_from_time(t) - 1900.0),
            70.0
        );
        assert_eq!(
            date_field_value(Y2021, false, |t| year_from_time(t) - 1900.0),
            121.0
        );
    }

    #[test]
    fn gettime_component_is_identity() {
        // getTime / valueOf pass the (finite) time value straight through.
        assert_eq!(date_field_value(1234.0, false, |t| t), 1234.0);
        assert_eq!(date_field_value(Y2021, false, |t| t), Y2021);
    }

    #[test]
    fn local_branch_routes_through_local_time_utc_branch_does_not() {
        // The UTC branch must pass t through unchanged; the local branch must
        // apply local_time, so the two branches differ by exactly the host
        // offset local_tza(t). This has teeth only for a developer running in a
        // non-UTC zone: on a UTC host (the CI default) local_tza() is 0 and the
        // branches coincide -- an inherent blind spot, since local_time is
        // host-derived and not injectable, so routing is unobservable there.
        let id = |t: f64| t;
        assert_eq!(date_field_value(Y2021, false, id), Y2021);
        assert_eq!(
            date_field_value(Y2021, true, id) - date_field_value(Y2021, false, id),
            local_tza(Y2021),
        );
    }
}
