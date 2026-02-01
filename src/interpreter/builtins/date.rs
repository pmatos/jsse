use super::super::*;

impl Interpreter {
    pub(crate) fn setup_date_builtin(&mut self) {
        let proto = self.create_object();
        proto.borrow_mut().class_name = "Date".to_string();
        proto.borrow_mut().primitive_value = Some(JsValue::Number(f64::NAN));

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
            // Setter methods
            (
                "setTime",
                1,
                Rc::new(|interp, this, args| {
                    let Some(_) = this_time_value(interp, this) else {
                        let e = interp.create_type_error("this is not a Date object");
                        return Completion::Throw(e);
                    };
                    let v = args.first().map(to_number).unwrap_or(f64::NAN);
                    let v = time_clip(v);
                    if let JsValue::Object(o) = this
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        obj.borrow_mut().primitive_value = Some(JsValue::Number(v));
                    }
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
                    let ms = args.first().map(to_number).unwrap_or(f64::NAN);
                    let lt = local_time(t);
                    let time =
                        make_time(hour_from_time(lt), min_from_time(lt), sec_from_time(lt), ms);
                    let v = time_clip(utc_time(make_date(day(lt), time)));
                    if let JsValue::Object(o) = this
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        obj.borrow_mut().primitive_value = Some(JsValue::Number(v));
                    }
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
                    let ms = args.first().map(to_number).unwrap_or(f64::NAN);
                    let time = make_time(hour_from_time(t), min_from_time(t), sec_from_time(t), ms);
                    let v = time_clip(make_date(day(t), time));
                    if let JsValue::Object(o) = this
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        obj.borrow_mut().primitive_value = Some(JsValue::Number(v));
                    }
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
                    let s = args.first().map(to_number).unwrap_or(f64::NAN);
                    let lt = local_time(t);
                    let ms = args
                        .get(1)
                        .map(to_number)
                        .unwrap_or_else(|| ms_from_time(lt));
                    let time = make_time(hour_from_time(lt), min_from_time(lt), s, ms);
                    let v = time_clip(utc_time(make_date(day(lt), time)));
                    if let JsValue::Object(o) = this
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        obj.borrow_mut().primitive_value = Some(JsValue::Number(v));
                    }
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
                    let s = args.first().map(to_number).unwrap_or(f64::NAN);
                    let ms = args
                        .get(1)
                        .map(to_number)
                        .unwrap_or_else(|| ms_from_time(t));
                    let time = make_time(hour_from_time(t), min_from_time(t), s, ms);
                    let v = time_clip(make_date(day(t), time));
                    if let JsValue::Object(o) = this
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        obj.borrow_mut().primitive_value = Some(JsValue::Number(v));
                    }
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
                    let m = args.first().map(to_number).unwrap_or(f64::NAN);
                    let lt = local_time(t);
                    let s = args
                        .get(1)
                        .map(to_number)
                        .unwrap_or_else(|| sec_from_time(lt));
                    let ms = args
                        .get(2)
                        .map(to_number)
                        .unwrap_or_else(|| ms_from_time(lt));
                    let time = make_time(hour_from_time(lt), m, s, ms);
                    let v = time_clip(utc_time(make_date(day(lt), time)));
                    if let JsValue::Object(o) = this
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        obj.borrow_mut().primitive_value = Some(JsValue::Number(v));
                    }
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
                    let m = args.first().map(to_number).unwrap_or(f64::NAN);
                    let s = args
                        .get(1)
                        .map(to_number)
                        .unwrap_or_else(|| sec_from_time(t));
                    let ms = args
                        .get(2)
                        .map(to_number)
                        .unwrap_or_else(|| ms_from_time(t));
                    let time = make_time(hour_from_time(t), m, s, ms);
                    let v = time_clip(make_date(day(t), time));
                    if let JsValue::Object(o) = this
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        obj.borrow_mut().primitive_value = Some(JsValue::Number(v));
                    }
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
                    let h = args.first().map(to_number).unwrap_or(f64::NAN);
                    let lt = local_time(t);
                    let m = args
                        .get(1)
                        .map(to_number)
                        .unwrap_or_else(|| min_from_time(lt));
                    let s = args
                        .get(2)
                        .map(to_number)
                        .unwrap_or_else(|| sec_from_time(lt));
                    let ms = args
                        .get(3)
                        .map(to_number)
                        .unwrap_or_else(|| ms_from_time(lt));
                    let time = make_time(h, m, s, ms);
                    let v = time_clip(utc_time(make_date(day(lt), time)));
                    if let JsValue::Object(o) = this
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        obj.borrow_mut().primitive_value = Some(JsValue::Number(v));
                    }
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
                    let h = args.first().map(to_number).unwrap_or(f64::NAN);
                    let m = args
                        .get(1)
                        .map(to_number)
                        .unwrap_or_else(|| min_from_time(t));
                    let s = args
                        .get(2)
                        .map(to_number)
                        .unwrap_or_else(|| sec_from_time(t));
                    let ms = args
                        .get(3)
                        .map(to_number)
                        .unwrap_or_else(|| ms_from_time(t));
                    let time = make_time(h, m, s, ms);
                    let v = time_clip(make_date(day(t), time));
                    if let JsValue::Object(o) = this
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        obj.borrow_mut().primitive_value = Some(JsValue::Number(v));
                    }
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
                    let dt = args.first().map(to_number).unwrap_or(f64::NAN);
                    let lt = local_time(t);
                    let new_date = make_day(year_from_time(lt), month_from_time(lt), dt);
                    let v = time_clip(utc_time(make_date(new_date, time_within_day(lt))));
                    if let JsValue::Object(o) = this
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        obj.borrow_mut().primitive_value = Some(JsValue::Number(v));
                    }
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
                    let dt = args.first().map(to_number).unwrap_or(f64::NAN);
                    let new_date = make_day(year_from_time(t), month_from_time(t), dt);
                    let v = time_clip(make_date(new_date, time_within_day(t)));
                    if let JsValue::Object(o) = this
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        obj.borrow_mut().primitive_value = Some(JsValue::Number(v));
                    }
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
                    let m = args.first().map(to_number).unwrap_or(f64::NAN);
                    let lt = local_time(t);
                    let dt = args
                        .get(1)
                        .map(to_number)
                        .unwrap_or_else(|| date_from_time(lt));
                    let new_date = make_day(year_from_time(lt), m, dt);
                    let v = time_clip(utc_time(make_date(new_date, time_within_day(lt))));
                    if let JsValue::Object(o) = this
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        obj.borrow_mut().primitive_value = Some(JsValue::Number(v));
                    }
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
                    let m = args.first().map(to_number).unwrap_or(f64::NAN);
                    let dt = args
                        .get(1)
                        .map(to_number)
                        .unwrap_or_else(|| date_from_time(t));
                    let new_date = make_day(year_from_time(t), m, dt);
                    let v = time_clip(make_date(new_date, time_within_day(t)));
                    if let JsValue::Object(o) = this
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        obj.borrow_mut().primitive_value = Some(JsValue::Number(v));
                    }
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
                    let t = if t.is_nan() { 0.0 } else { t };
                    let y = args.first().map(to_number).unwrap_or(f64::NAN);
                    let lt = local_time(t);
                    let m = args
                        .get(1)
                        .map(to_number)
                        .unwrap_or_else(|| month_from_time(lt));
                    let dt = args
                        .get(2)
                        .map(to_number)
                        .unwrap_or_else(|| date_from_time(lt));
                    let new_date = make_day(y, m, dt);
                    let v = time_clip(utc_time(make_date(new_date, time_within_day(lt))));
                    if let JsValue::Object(o) = this
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        obj.borrow_mut().primitive_value = Some(JsValue::Number(v));
                    }
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
                    let t = if t.is_nan() { 0.0 } else { t };
                    let y = args.first().map(to_number).unwrap_or(f64::NAN);
                    let m = args
                        .get(1)
                        .map(to_number)
                        .unwrap_or_else(|| month_from_time(t));
                    let dt = args
                        .get(2)
                        .map(to_number)
                        .unwrap_or_else(|| date_from_time(t));
                    let new_date = make_day(y, m, dt);
                    let v = time_clip(make_date(new_date, time_within_day(t)));
                    if let JsValue::Object(o) = this
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        obj.borrow_mut().primitive_value = Some(JsValue::Number(v));
                    }
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
            (
                "toJSON",
                1,
                Rc::new(|interp, this, _args| {
                    let num = interp.to_number_coerce(this);
                    if !num.is_finite() {
                        return Completion::Normal(JsValue::Null);
                    }
                    if let JsValue::Object(o) = this
                        && let Some(obj) = interp.get_object(o.id)
                    {
                        let to_iso = obj.borrow().get_property("toISOString");
                        if let JsValue::Object(_) = &to_iso {
                            return interp.call_function(&to_iso, this, &[]);
                        }
                    }
                    let e = interp.create_type_error("toISOString is not a function");
                    Completion::Throw(e)
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
            let fn_val = self.create_function(JsFunction::Native(name.to_string(), arity, func, false));
            proto.borrow_mut().insert_builtin(name.to_string(), fn_val);
        }

        // Symbol.toPrimitive
        let to_prim_fn = self.create_function(JsFunction::native(
            "[Symbol.toPrimitive]".to_string(),
            1,
            |interp, this, args| {
                let Some(_) = this_time_value(interp, this) else {
                    let e = interp.create_type_error("this is not a Date object");
                    return Completion::Throw(e);
                };
                let hint = args.first().map(to_js_string).unwrap_or_default();
                match hint.as_str() {
                    "string" | "default" => {
                        // Call toString
                        if let JsValue::Object(o) = this
                            && let Some(obj) = interp.get_object(o.id)
                        {
                            let ts = obj.borrow().get_property("toString");
                            if let JsValue::Object(_) = &ts {
                                return interp.call_function(&ts, this, &[]);
                            }
                        }
                        Completion::Normal(JsValue::Undefined)
                    }
                    "number" => {
                        // Call valueOf
                        if let JsValue::Object(o) = this
                            && let Some(obj) = interp.get_object(o.id)
                        {
                            let vo = obj.borrow().get_property("valueOf");
                            if let JsValue::Object(_) = &vo {
                                return interp.call_function(&vo, this, &[]);
                            }
                        }
                        Completion::Normal(JsValue::Undefined)
                    }
                    _ => {
                        let e = interp.create_type_error("Invalid hint");
                        Completion::Throw(e)
                    }
                }
            },
        ));
        // Get the Symbol.toPrimitive key
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
                // Called as function (no new) - return string
                if interp.new_target.is_none() {
                    let t = now_ms();
                    return Completion::Normal(JsValue::String(JsString::from_str(
                        &format_date_string(t),
                    )));
                }

                // Called with new
                let time_val = if args.is_empty() {
                    now_ms()
                } else if args.len() == 1 {
                    let v = &args[0];
                    if let JsValue::Object(o) = v
                        && let Some(obj) = interp.get_object(o.id)
                        && obj.borrow().class_name == "Date"
                    {
                        if let Some(JsValue::Number(t)) = obj.borrow().primitive_value.clone() {
                            t
                        } else {
                            f64::NAN
                        }
                    } else if let JsValue::String(_) = v {
                        parse_date_string(&to_js_string(v))
                    } else {
                        let n = to_number(v);
                        time_clip(n)
                    }
                } else {
                    // 2-7 args
                    let y = to_number(&args[0]);
                    let m = args.get(1).map(to_number).unwrap_or(0.0);
                    let dt = args.get(2).map(to_number).unwrap_or(1.0);
                    let h = args.get(3).map(to_number).unwrap_or(0.0);
                    let min = args.get(4).map(to_number).unwrap_or(0.0);
                    let s = args.get(5).map(to_number).unwrap_or(0.0);
                    let ms = args.get(6).map(to_number).unwrap_or(0.0);
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

        // Set .length = 7 on Date constructor
        if let JsValue::Object(o) = &date_ctor
            && let Some(obj) = self.get_object(o.id)
        {
            obj.borrow_mut().insert_property(
                "length".to_string(),
                PropertyDescriptor::data(JsValue::Number(7.0), false, false, true),
            );

            // Date.now()
            let now_fn = self.create_function(JsFunction::native(
                "now".to_string(),
                0,
                |_interp, _this, _args| Completion::Normal(JsValue::Number(now_ms().floor())),
            ));
            obj.borrow_mut().insert_builtin("now".to_string(), now_fn);

            // Date.parse()
            let parse_fn = self.create_function(JsFunction::native(
                "parse".to_string(),
                2,
                |_interp, _this, args| {
                    let s = args.first().map(to_js_string).unwrap_or_default();
                    Completion::Normal(JsValue::Number(parse_date_string(&s)))
                },
            ));
            obj.borrow_mut()
                .insert_builtin("parse".to_string(), parse_fn);

            // Date.UTC()
            let utc_fn = self.create_function(JsFunction::native(
                "UTC".to_string(),
                7,
                |_interp, _this, args| {
                    let y = args.first().map(to_number).unwrap_or(f64::NAN);
                    let m = args.get(1).map(to_number).unwrap_or(0.0);
                    let dt = args.get(2).map(to_number).unwrap_or(1.0);
                    let h = args.get(3).map(to_number).unwrap_or(0.0);
                    let min = args.get(4).map(to_number).unwrap_or(0.0);
                    let s = args.get(5).map(to_number).unwrap_or(0.0);
                    let ms = args.get(6).map(to_number).unwrap_or(0.0);
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

            // Set Date.prototype
            let proto_val = JsValue::Object(crate::types::JsObject {
                id: proto.borrow().id.unwrap(),
            });
            obj.borrow_mut()
                .insert_value("prototype".to_string(), proto_val);
        }

        // Date.prototype.constructor
        proto
            .borrow_mut()
            .insert_builtin("constructor".to_string(), date_ctor.clone());

        self.global_env
            .borrow_mut()
            .declare("Date", BindingKind::Var);
        let _ = self.global_env.borrow_mut().set("Date", date_ctor);
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
            o.insert_value(
                "message".to_string(),
                JsValue::String(JsString::from_str(msg)),
            );
            o.insert_value(
                "name".to_string(),
                JsValue::String(JsString::from_str(name)),
            );
        }
        let id = obj.borrow().id.unwrap();
        JsValue::Object(crate::types::JsObject { id })
    }
}
