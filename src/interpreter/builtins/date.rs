use super::super::*;

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
                        let pds: Vec<(String, PropertyDescriptor)> = src
                            .borrow()
                            .properties
                            .iter()
                            .map(|(k, pd)| (k.to_string(), pd.clone()))
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

        // Shared brand check for every Date.prototype method: return `this`'s
        // [[DateValue]], or the canonical TypeError when `this` is not a Date.
        // Single home of the receiver check previously hand-rolled per method.
        fn require_time_value(interp: &mut Interpreter, this: &JsValue) -> Result<f64, Completion> {
            match this_time_value(interp, this) {
                Some(t) => Ok(t),
                None => Err(Completion::Throw(
                    interp.create_type_error("this is not a Date object"),
                )),
            }
        }

        // Builds a component-getter closure: brand-check `this`, return NaN for an
        // invalid Date, otherwise apply `field` to the (optionally localized)
        // time. Collapses the structurally-identical Date.prototype.get* bodies.
        fn date_field_getter(
            field: fn(f64) -> f64,
            local: bool,
        ) -> Rc<dyn Fn(&mut Interpreter, &JsValue, &[JsValue]) -> Completion> {
            Rc::new(move |interp, this, _args| {
                let t = match require_time_value(interp, this) {
                    Ok(t) => t,
                    Err(c) => return c,
                };
                if t.is_nan() {
                    return Completion::Normal(JsValue::Number(f64::NAN));
                }
                let tv = if local { local_time(t) } else { t };
                Completion::Normal(JsValue::Number(field(tv)))
            })
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
            (
                "getTime",
                0,
                Rc::new(
                    |interp, this, _args| match require_time_value(interp, this) {
                        Ok(t) => Completion::Normal(JsValue::Number(t)),
                        Err(c) => c,
                    },
                ),
            ),
            (
                "valueOf",
                0,
                Rc::new(
                    |interp, this, _args| match require_time_value(interp, this) {
                        Ok(t) => Completion::Normal(JsValue::Number(t)),
                        Err(c) => c,
                    },
                ),
            ),
            ("getFullYear", 0, date_field_getter(year_from_time, true)),
            ("getMonth", 0, date_field_getter(month_from_time, true)),
            ("getDate", 0, date_field_getter(date_from_time, true)),
            ("getDay", 0, date_field_getter(week_day, true)),
            ("getHours", 0, date_field_getter(hour_from_time, true)),
            ("getMinutes", 0, date_field_getter(min_from_time, true)),
            ("getSeconds", 0, date_field_getter(sec_from_time, true)),
            ("getMilliseconds", 0, date_field_getter(ms_from_time, true)),
            (
                "getUTCFullYear",
                0,
                date_field_getter(year_from_time, false),
            ),
            ("getUTCMonth", 0, date_field_getter(month_from_time, false)),
            ("getUTCDate", 0, date_field_getter(date_from_time, false)),
            ("getUTCDay", 0, date_field_getter(week_day, false)),
            ("getUTCHours", 0, date_field_getter(hour_from_time, false)),
            ("getUTCMinutes", 0, date_field_getter(min_from_time, false)),
            ("getUTCSeconds", 0, date_field_getter(sec_from_time, false)),
            (
                "getUTCMilliseconds",
                0,
                date_field_getter(ms_from_time, false),
            ),
            (
                "getTimezoneOffset",
                0,
                date_field_getter(|t| (t - local_time(t)) / 60_000.0, false),
            ),
            // Setter methods -- all use to_number_value for proper error propagation
            (
                "setTime",
                1,
                Rc::new(|interp, this, args| {
                    if let Err(c) = require_time_value(interp, this) {
                        return c;
                    }
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
                    let t = match require_time_value(interp, this) {
                        Ok(t) => t,
                        Err(c) => return c,
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
                    let t = match require_time_value(interp, this) {
                        Ok(t) => t,
                        Err(c) => return c,
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
                    let t = match require_time_value(interp, this) {
                        Ok(t) => t,
                        Err(c) => return c,
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
                    let t = match require_time_value(interp, this) {
                        Ok(t) => t,
                        Err(c) => return c,
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
                    let t = match require_time_value(interp, this) {
                        Ok(t) => t,
                        Err(c) => return c,
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
                    let t = match require_time_value(interp, this) {
                        Ok(t) => t,
                        Err(c) => return c,
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
                    let t = match require_time_value(interp, this) {
                        Ok(t) => t,
                        Err(c) => return c,
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
                    let t = match require_time_value(interp, this) {
                        Ok(t) => t,
                        Err(c) => return c,
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
                    let t = match require_time_value(interp, this) {
                        Ok(t) => t,
                        Err(c) => return c,
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
                    let t = match require_time_value(interp, this) {
                        Ok(t) => t,
                        Err(c) => return c,
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
                    let t = match require_time_value(interp, this) {
                        Ok(t) => t,
                        Err(c) => return c,
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
                    let t = match require_time_value(interp, this) {
                        Ok(t) => t,
                        Err(c) => return c,
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
                    let t = match require_time_value(interp, this) {
                        Ok(t) => t,
                        Err(c) => return c,
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
                    let t = match require_time_value(interp, this) {
                        Ok(t) => t,
                        Err(c) => return c,
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
                Rc::new(|interp, this, _args| {
                    let t = match require_time_value(interp, this) {
                        Ok(t) => t,
                        Err(c) => return c,
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::String(JsString::from_str(
                            "Invalid Date",
                        )));
                    }
                    Completion::Normal(JsValue::String(JsString::from_str(&format_date_string(t))))
                }),
            ),
            (
                "toDateString",
                0,
                Rc::new(|interp, this, _args| {
                    let t = match require_time_value(interp, this) {
                        Ok(t) => t,
                        Err(c) => return c,
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::String(JsString::from_str(
                            "Invalid Date",
                        )));
                    }
                    Completion::Normal(JsValue::String(JsString::from_str(
                        &format_date_only_string(t),
                    )))
                }),
            ),
            (
                "toTimeString",
                0,
                Rc::new(|interp, this, _args| {
                    let t = match require_time_value(interp, this) {
                        Ok(t) => t,
                        Err(c) => return c,
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::String(JsString::from_str(
                            "Invalid Date",
                        )));
                    }
                    Completion::Normal(JsValue::String(JsString::from_str(
                        &format_time_only_string(t),
                    )))
                }),
            ),
            (
                "toISOString",
                0,
                Rc::new(|interp, this, _args| {
                    let t = match require_time_value(interp, this) {
                        Ok(t) => t,
                        Err(c) => return c,
                    };
                    if !t.is_finite() {
                        let e = interp.create_range_error("Invalid time value");
                        return Completion::Throw(e);
                    }
                    Completion::Normal(JsValue::String(JsString::from_str(&format_iso_string(t))))
                }),
            ),
            (
                "toUTCString",
                0,
                Rc::new(|interp, this, _args| {
                    let t = match require_time_value(interp, this) {
                        Ok(t) => t,
                        Err(c) => return c,
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::String(JsString::from_str(
                            "Invalid Date",
                        )));
                    }
                    Completion::Normal(JsValue::String(JsString::from_str(&format_utc_string(t))))
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
                    let t = match require_time_value(interp, this) {
                        Ok(t) => t,
                        Err(c) => return c,
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
        if let Some(sym_val) = self.get_global_var("Symbol")
            && let JsValue::Object(sym_obj) = &sym_val
        {
            let tp_key = to_js_string(&self.get_property_on_id(sym_obj.id, "toPrimitive"));
            self.get_object_cell_expect(proto_id)
                .borrow_mut()
                .insert_property(
                    tp_key,
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

        // Annex B: getYear()
        let get_year_fn = self.create_function(JsFunction::Native(
            "getYear".to_string(),
            0,
            date_field_getter(|t| year_from_time(t) - 1900.0, true),
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
                let t = match require_time_value(interp, this) {
                    Ok(t) => t,
                    Err(c) => return c,
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
