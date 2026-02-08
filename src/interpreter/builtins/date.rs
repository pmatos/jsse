use super::super::*;

impl Interpreter {
    pub(crate) fn setup_date_builtin(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "Date".to_string();
        // Date.prototype does NOT have [[DateValue]] per spec

        fn this_time_value(interp: &Interpreter, this: &JsValue) -> Option<f64> {
            if let JsValue::Object(o) = this
                && let Some(obj) = interp.get_object(o.id)
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

        fn set_date_value(interp: &Interpreter, this: &JsValue, v: f64) {
            if let JsValue::Object(o) = this
                && let Some(obj) = interp.get_object(o.id)
            {
                obj.borrow_mut().primitive_value = Some(JsValue::Number(v));
            }
        }

        fn to_num(interp: &mut Interpreter, val: &JsValue) -> Result<f64, JsValue> {
            interp.to_number_value(val)
        }

        // Getter methods
        let methods: Vec<(
            &str,
            usize,
            Rc<dyn Fn(&mut Interpreter, &JsValue, &[JsValue]) -> Completion>,
        )> = vec![
            (
                "getTime",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) => Completion::Normal(JsValue::Number(t)),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
                }),
            ),
            (
                "valueOf",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) => Completion::Normal(JsValue::Number(t)),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
                }),
            ),
            (
                "getFullYear",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) if t.is_nan() => Completion::Normal(JsValue::Number(f64::NAN)),
                    Some(t) => Completion::Normal(JsValue::Number(year_from_time(local_time(t)))),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
                }),
            ),
            (
                "getMonth",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) if t.is_nan() => Completion::Normal(JsValue::Number(f64::NAN)),
                    Some(t) => Completion::Normal(JsValue::Number(month_from_time(local_time(t)))),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
                }),
            ),
            (
                "getDate",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) if t.is_nan() => Completion::Normal(JsValue::Number(f64::NAN)),
                    Some(t) => Completion::Normal(JsValue::Number(date_from_time(local_time(t)))),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
                }),
            ),
            (
                "getDay",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) if t.is_nan() => Completion::Normal(JsValue::Number(f64::NAN)),
                    Some(t) => Completion::Normal(JsValue::Number(week_day(local_time(t)))),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
                }),
            ),
            (
                "getHours",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) if t.is_nan() => Completion::Normal(JsValue::Number(f64::NAN)),
                    Some(t) => Completion::Normal(JsValue::Number(hour_from_time(local_time(t)))),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
                }),
            ),
            (
                "getMinutes",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) if t.is_nan() => Completion::Normal(JsValue::Number(f64::NAN)),
                    Some(t) => Completion::Normal(JsValue::Number(min_from_time(local_time(t)))),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
                }),
            ),
            (
                "getSeconds",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) if t.is_nan() => Completion::Normal(JsValue::Number(f64::NAN)),
                    Some(t) => Completion::Normal(JsValue::Number(sec_from_time(local_time(t)))),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
                }),
            ),
            (
                "getMilliseconds",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) if t.is_nan() => Completion::Normal(JsValue::Number(f64::NAN)),
                    Some(t) => Completion::Normal(JsValue::Number(ms_from_time(local_time(t)))),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
                }),
            ),
            (
                "getUTCFullYear",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) if t.is_nan() => Completion::Normal(JsValue::Number(f64::NAN)),
                    Some(t) => Completion::Normal(JsValue::Number(year_from_time(t))),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
                }),
            ),
            (
                "getUTCMonth",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) if t.is_nan() => Completion::Normal(JsValue::Number(f64::NAN)),
                    Some(t) => Completion::Normal(JsValue::Number(month_from_time(t))),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
                }),
            ),
            (
                "getUTCDate",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) if t.is_nan() => Completion::Normal(JsValue::Number(f64::NAN)),
                    Some(t) => Completion::Normal(JsValue::Number(date_from_time(t))),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
                }),
            ),
            (
                "getUTCDay",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) if t.is_nan() => Completion::Normal(JsValue::Number(f64::NAN)),
                    Some(t) => Completion::Normal(JsValue::Number(week_day(t))),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
                }),
            ),
            (
                "getUTCHours",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) if t.is_nan() => Completion::Normal(JsValue::Number(f64::NAN)),
                    Some(t) => Completion::Normal(JsValue::Number(hour_from_time(t))),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
                }),
            ),
            (
                "getUTCMinutes",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) if t.is_nan() => Completion::Normal(JsValue::Number(f64::NAN)),
                    Some(t) => Completion::Normal(JsValue::Number(min_from_time(t))),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
                }),
            ),
            (
                "getUTCSeconds",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) if t.is_nan() => Completion::Normal(JsValue::Number(f64::NAN)),
                    Some(t) => Completion::Normal(JsValue::Number(sec_from_time(t))),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
                }),
            ),
            (
                "getUTCMilliseconds",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) if t.is_nan() => Completion::Normal(JsValue::Number(f64::NAN)),
                    Some(t) => Completion::Normal(JsValue::Number(ms_from_time(t))),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
                }),
            ),
            (
                "getTimezoneOffset",
                0,
                Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                    Some(t) if t.is_nan() => Completion::Normal(JsValue::Number(f64::NAN)),
                    Some(t) => Completion::Normal(JsValue::Number((t - local_time(t)) / 60_000.0)),
                    None => {
                        let e = interp.create_type_error("this is not a Date object");
                        Completion::Throw(e)
                    }
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
                    let v = match args.first() {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => f64::NAN,
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
                    let ms = match args.first() {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => f64::NAN,
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let lt = local_time(t);
                    let time =
                        make_time(hour_from_time(lt), min_from_time(lt), sec_from_time(lt), ms);
                    let v = time_clip(utc_time(make_date(day(lt), time)));
                    set_date_value(interp, this, v);
                    Completion::Normal(JsValue::Number(v))
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
                    let ms = match args.first() {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => f64::NAN,
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let time = make_time(hour_from_time(t), min_from_time(t), sec_from_time(t), ms);
                    let v = time_clip(make_date(day(t), time));
                    set_date_value(interp, this, v);
                    Completion::Normal(JsValue::Number(v))
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
                    let s = match args.first() {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => f64::NAN,
                    };
                    let ms = match args.get(1) {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => {
                            if t.is_nan() {
                                f64::NAN
                            } else {
                                ms_from_time(local_time(t))
                            }
                        }
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let lt = local_time(t);
                    let time = make_time(hour_from_time(lt), min_from_time(lt), s, ms);
                    let v = time_clip(utc_time(make_date(day(lt), time)));
                    set_date_value(interp, this, v);
                    Completion::Normal(JsValue::Number(v))
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
                    let s = match args.first() {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => f64::NAN,
                    };
                    let ms = match args.get(1) {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => {
                            if t.is_nan() {
                                f64::NAN
                            } else {
                                ms_from_time(t)
                            }
                        }
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let time = make_time(hour_from_time(t), min_from_time(t), s, ms);
                    let v = time_clip(make_date(day(t), time));
                    set_date_value(interp, this, v);
                    Completion::Normal(JsValue::Number(v))
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
                    let m = match args.first() {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => f64::NAN,
                    };
                    let s = match args.get(1) {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => {
                            if t.is_nan() {
                                f64::NAN
                            } else {
                                sec_from_time(local_time(t))
                            }
                        }
                    };
                    let ms = match args.get(2) {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => {
                            if t.is_nan() {
                                f64::NAN
                            } else {
                                ms_from_time(local_time(t))
                            }
                        }
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let lt = local_time(t);
                    let time = make_time(hour_from_time(lt), m, s, ms);
                    let v = time_clip(utc_time(make_date(day(lt), time)));
                    set_date_value(interp, this, v);
                    Completion::Normal(JsValue::Number(v))
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
                    let m = match args.first() {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => f64::NAN,
                    };
                    let s = match args.get(1) {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => {
                            if t.is_nan() {
                                f64::NAN
                            } else {
                                sec_from_time(t)
                            }
                        }
                    };
                    let ms = match args.get(2) {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => {
                            if t.is_nan() {
                                f64::NAN
                            } else {
                                ms_from_time(t)
                            }
                        }
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let time = make_time(hour_from_time(t), m, s, ms);
                    let v = time_clip(make_date(day(t), time));
                    set_date_value(interp, this, v);
                    Completion::Normal(JsValue::Number(v))
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
                    let h = match args.first() {
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
                        None => {
                            if t.is_nan() {
                                f64::NAN
                            } else {
                                min_from_time(local_time(t))
                            }
                        }
                    };
                    let s = match args.get(2) {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => {
                            if t.is_nan() {
                                f64::NAN
                            } else {
                                sec_from_time(local_time(t))
                            }
                        }
                    };
                    let ms = match args.get(3) {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => {
                            if t.is_nan() {
                                f64::NAN
                            } else {
                                ms_from_time(local_time(t))
                            }
                        }
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let lt = local_time(t);
                    let time = make_time(h, m, s, ms);
                    let v = time_clip(utc_time(make_date(day(lt), time)));
                    set_date_value(interp, this, v);
                    Completion::Normal(JsValue::Number(v))
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
                    let h = match args.first() {
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
                        None => {
                            if t.is_nan() {
                                f64::NAN
                            } else {
                                min_from_time(t)
                            }
                        }
                    };
                    let s = match args.get(2) {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => {
                            if t.is_nan() {
                                f64::NAN
                            } else {
                                sec_from_time(t)
                            }
                        }
                    };
                    let ms = match args.get(3) {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => {
                            if t.is_nan() {
                                f64::NAN
                            } else {
                                ms_from_time(t)
                            }
                        }
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let time = make_time(h, m, s, ms);
                    let v = time_clip(make_date(day(t), time));
                    set_date_value(interp, this, v);
                    Completion::Normal(JsValue::Number(v))
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
                    let dt = match args.first() {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => f64::NAN,
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let lt = local_time(t);
                    let new_date = make_day(year_from_time(lt), month_from_time(lt), dt);
                    let v = time_clip(utc_time(make_date(new_date, time_within_day(lt))));
                    set_date_value(interp, this, v);
                    Completion::Normal(JsValue::Number(v))
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
                    let dt = match args.first() {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => f64::NAN,
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let new_date = make_day(year_from_time(t), month_from_time(t), dt);
                    let v = time_clip(make_date(new_date, time_within_day(t)));
                    set_date_value(interp, this, v);
                    Completion::Normal(JsValue::Number(v))
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
                    let m = match args.first() {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => f64::NAN,
                    };
                    let dt = match args.get(1) {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => {
                            if t.is_nan() {
                                f64::NAN
                            } else {
                                date_from_time(local_time(t))
                            }
                        }
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let lt = local_time(t);
                    let new_date = make_day(year_from_time(lt), m, dt);
                    let v = time_clip(utc_time(make_date(new_date, time_within_day(lt))));
                    set_date_value(interp, this, v);
                    Completion::Normal(JsValue::Number(v))
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
                    let m = match args.first() {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => f64::NAN,
                    };
                    let dt = match args.get(1) {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => {
                            if t.is_nan() {
                                f64::NAN
                            } else {
                                date_from_time(t)
                            }
                        }
                    };
                    if t.is_nan() {
                        return Completion::Normal(JsValue::Number(f64::NAN));
                    }
                    let new_date = make_day(year_from_time(t), m, dt);
                    let v = time_clip(make_date(new_date, time_within_day(t)));
                    set_date_value(interp, this, v);
                    Completion::Normal(JsValue::Number(v))
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
                    let y = match args.first() {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => f64::NAN,
                    };
                    // Per spec: if t is NaN, set t to +0; otherwise set t to LocalTime(t)
                    let lt = if t.is_nan() { 0.0 } else { local_time(t) };
                    let m = match args.get(1) {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => month_from_time(lt),
                    };
                    let dt = match args.get(2) {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => date_from_time(lt),
                    };
                    let new_date = make_day(y, m, dt);
                    let v = time_clip(utc_time(make_date(new_date, time_within_day(lt))));
                    set_date_value(interp, this, v);
                    Completion::Normal(JsValue::Number(v))
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
                        None => month_from_time(t_adj),
                    };
                    let dt = match args.get(2) {
                        Some(a) => match to_num(interp, a) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        },
                        None => date_from_time(t_adj),
                    };
                    let new_date = make_day(y, m, dt);
                    let v = time_clip(make_date(new_date, time_within_day(t_adj)));
                    set_date_value(interp, this, v);
                    Completion::Normal(JsValue::Number(v))
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
                    if let JsValue::Number(n) = &tv {
                        if !n.is_finite() {
                            return Completion::Normal(JsValue::Null);
                        }
                    }
                    if let JsValue::Object(obj_ref) = &o {
                        let to_iso = interp.get_object_property(obj_ref.id, "toISOString", &o);
                        match to_iso {
                            Completion::Normal(func) => {
                                if let JsValue::Object(fo) = &func
                                    && interp
                                        .get_object(fo.id)
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
                "toLocaleString",
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
                "toLocaleTimeString",
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
        ];

        for (name, arity, func) in methods {
            let fn_val =
                self.create_function(JsFunction::Native(name.to_string(), arity, func, false));
            proto.borrow_mut().insert_builtin(name.to_string(), fn_val);
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
                            .get_object(fo.id)
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
        if let Some(sym_val) = self.global_env.borrow().get("Symbol")
            && let JsValue::Object(sym_obj) = &sym_val
            && let Some(sym_data) = self.get_object(sym_obj.id)
        {
            let tp_key = to_js_string(&sym_data.borrow().get_property("toPrimitive"));
            proto.borrow_mut().insert_property(
                tp_key,
                PropertyDescriptor::data(to_prim_fn, false, false, true),
            );
        }

        // Date constructor
        let date_proto_clone = proto.clone();
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
                        && let Some(obj) = interp.get_object(o.id)
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
                    && let Some(obj) = interp.get_object(o.id)
                {
                    let mut b = obj.borrow_mut();
                    b.class_name = "Date".to_string();
                    b.primitive_value = Some(JsValue::Number(time_val));
                    b.prototype = Some(date_proto_clone.clone());
                }
                Completion::Normal(this.clone())
            },
        ));

        if let JsValue::Object(o) = &date_ctor
            && let Some(obj) = self.get_object(o.id)
        {
            obj.borrow_mut().insert_property(
                "length".to_string(),
                PropertyDescriptor::data(JsValue::Number(7.0), false, false, true),
            );

            let now_fn = self.create_function(JsFunction::native(
                "now".to_string(),
                0,
                |_interp, _this, _args| Completion::Normal(JsValue::Number(now_ms().floor())),
            ));
            obj.borrow_mut().insert_builtin("now".to_string(), now_fn);

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
            obj.borrow_mut()
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
            obj.borrow_mut().insert_builtin("UTC".to_string(), utc_fn);

            let proto_val = JsValue::Object(crate::types::JsObject {
                id: proto.borrow().id.unwrap(),
            });
            obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(proto_val, false, false, false),
            );
        }

        proto
            .borrow_mut()
            .insert_builtin("constructor".to_string(), date_ctor.clone());

        self.global_env
            .borrow_mut()
            .declare("Date", BindingKind::Var);
        let _ = self.global_env.borrow_mut().set("Date", date_ctor);

        // Annex B: getYear()
        let get_year_fn = self.create_function(JsFunction::Native(
            "getYear".to_string(),
            0,
            Rc::new(|interp, this, _args| match this_time_value(interp, this) {
                Some(t) if t.is_nan() => Completion::Normal(JsValue::Number(f64::NAN)),
                Some(t) => {
                    let y = year_from_time(local_time(t));
                    Completion::Normal(JsValue::Number(y - 1900.0))
                }
                None => {
                    let e = interp.create_type_error("this is not a Date object");
                    Completion::Throw(e)
                }
            }),
            false,
        ));
        proto
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
                let y = match args.first() {
                    Some(a) => match to_num(interp, a) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    },
                    None => f64::NAN,
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
                let v = time_clip(utc_time(make_date(new_date, time_within_day(t))));
                set_date_value(interp, this, v);
                Completion::Normal(JsValue::Number(v))
            }),
            false,
        ));
        proto
            .borrow_mut()
            .insert_builtin("setYear".to_string(), set_year_fn);

        // Annex B: toGMTString() -- alias for toUTCString()
        let to_gmt = proto.borrow().get_property("toUTCString");
        proto
            .borrow_mut()
            .insert_builtin("toGMTString".to_string(), to_gmt);

        self.date_prototype = Some(proto);
    }

    pub(crate) fn create_range_error(&mut self, msg: &str) -> JsValue {
        self.create_error("RangeError", msg)
    }

    pub(crate) fn create_reference_error(&mut self, msg: &str) -> JsValue {
        self.create_error("ReferenceError", msg)
    }

    pub(crate) fn create_error(&mut self, name: &str, msg: &str) -> JsValue {
        let env = self.global_env.borrow();
        let error_proto = env.get(name).and_then(|v| {
            if let JsValue::Object(o) = &v {
                self.get_object(o.id).and_then(|ctor| {
                    let pv = ctor.borrow().get_property("prototype");
                    if let JsValue::Object(p) = &pv {
                        self.get_object(p.id)
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        });
        drop(env);
        let obj = self.create_object();
        {
            let mut o = obj.borrow_mut();
            o.class_name = name.to_string();
            if let Some(proto) = error_proto {
                o.prototype = Some(proto);
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
        let id = obj.borrow().id.unwrap();
        JsValue::Object(crate::types::JsObject { id })
    }
}
