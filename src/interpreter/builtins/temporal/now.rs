use super::*;
use num_bigint::BigInt;
use std::time::{SystemTime, UNIX_EPOCH};

impl Interpreter {
    pub(crate) fn setup_temporal_now(&mut self, temporal_obj: &Rc<RefCell<JsObjectData>>) {
        let now_obj = self.create_object();
        let now_id = now_obj.borrow().id.unwrap();

        // @@toStringTag = "Temporal.Now"
        {
            let key = "Symbol(Symbol.toStringTag)".to_string();
            let desc = PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str("Temporal.Now"))),
                writable: Some(false),
                enumerable: Some(false),
                configurable: Some(true),
                get: None,
                set: None,
            };
            now_obj.borrow_mut().property_order.push(key.clone());
            now_obj.borrow_mut().properties.insert(key, desc);
        }

        // Temporal.Now.timeZoneId()
        let tz_fn = self.create_function(JsFunction::native(
            "timeZoneId".to_string(),
            0,
            |_interp, _this, _args| {
                let tz = iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".to_string());
                Completion::Normal(JsValue::String(JsString::from_str(&tz)))
            },
        ));
        now_obj
            .borrow_mut()
            .insert_builtin("timeZoneId".to_string(), tz_fn);

        // Temporal.Now.instant()
        let instant_fn = self.create_function(JsFunction::native(
            "instant".to_string(),
            0,
            |interp, _this, _args| {
                let ns = current_epoch_nanoseconds();
                let obj = interp.create_object();
                obj.borrow_mut().class_name = "Temporal.Instant".to_string();
                if let Some(ref proto) = interp.temporal_instant_prototype {
                    obj.borrow_mut().prototype = Some(proto.clone());
                }
                obj.borrow_mut().temporal_data = Some(TemporalData::Instant {
                    epoch_nanoseconds: ns,
                });
                let id = obj.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
            },
        ));
        now_obj
            .borrow_mut()
            .insert_builtin("instant".to_string(), instant_fn);

        // Temporal.Now.plainDateTimeISO(timeZone?)
        let pdt_fn = self.create_function(JsFunction::native(
            "plainDateTimeISO".to_string(),
            0,
            |interp, _this, args| {
                let tz_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                // Validate timezone argument (but still use local time)
                match super::to_temporal_time_zone_identifier(interp, &tz_arg) {
                    Ok(_tz) => {}
                    Err(c) => return c,
                }
                let epoch_ms = current_epoch_ms();
                let (y, m, d, h, mi, s, ms) = epoch_ms_to_local_components(epoch_ms);
                super::plain_date_time::create_plain_date_time_result(
                    interp, y, m, d, h, mi, s, ms, 0, 0, "iso8601",
                )
            },
        ));
        now_obj
            .borrow_mut()
            .insert_builtin("plainDateTimeISO".to_string(), pdt_fn);

        // Temporal.Now.plainDateISO(timeZone?)
        let pd_fn = self.create_function(JsFunction::native(
            "plainDateISO".to_string(),
            0,
            |interp, _this, args| {
                let tz_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                match super::to_temporal_time_zone_identifier(interp, &tz_arg) {
                    Ok(_tz) => {}
                    Err(c) => return c,
                }
                let epoch_ms = current_epoch_ms();
                let (y, m, d, _, _, _, _) = epoch_ms_to_local_components(epoch_ms);
                super::plain_date::create_plain_date_result(interp, y, m, d, "iso8601")
            },
        ));
        now_obj
            .borrow_mut()
            .insert_builtin("plainDateISO".to_string(), pd_fn);

        // Temporal.Now.plainTimeISO(timeZone?)
        let pt_fn = self.create_function(JsFunction::native(
            "plainTimeISO".to_string(),
            0,
            |interp, _this, args| {
                let tz_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                match super::to_temporal_time_zone_identifier(interp, &tz_arg) {
                    Ok(_tz) => {}
                    Err(c) => return c,
                }
                let epoch_ms = current_epoch_ms();
                let (_, _, _, h, mi, s, ms) = epoch_ms_to_local_components(epoch_ms);
                super::plain_time::create_plain_time_result(interp, h, mi, s, ms, 0, 0)
            },
        ));
        now_obj
            .borrow_mut()
            .insert_builtin("plainTimeISO".to_string(), pt_fn);

        // Temporal.Now.zonedDateTimeISO(timeZone?)
        let zdt_fn = self.create_function(JsFunction::native(
            "zonedDateTimeISO".to_string(),
            0,
            |interp, _this, args| {
                let tz_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let tz = match super::to_temporal_time_zone_identifier(interp, &tz_arg) {
                    Ok(tz) => tz,
                    Err(c) => return c,
                };
                let ns = current_epoch_nanoseconds();
                let obj = interp.create_object();
                obj.borrow_mut().class_name = "Temporal.ZonedDateTime".to_string();
                if let Some(ref proto) = interp.temporal_zoned_date_time_prototype {
                    obj.borrow_mut().prototype = Some(proto.clone());
                }
                obj.borrow_mut().temporal_data = Some(TemporalData::ZonedDateTime {
                    epoch_nanoseconds: ns,
                    time_zone: tz,
                    calendar: "iso8601".to_string(),
                });
                let id = obj.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
            },
        ));
        now_obj
            .borrow_mut()
            .insert_builtin("zonedDateTimeISO".to_string(), zdt_fn);

        let now_val = JsValue::Object(crate::types::JsObject { id: now_id });
        temporal_obj.borrow_mut().insert_property(
            "Now".to_string(),
            PropertyDescriptor::data(now_val, true, false, true),
        );
    }
}

fn current_epoch_nanoseconds() -> BigInt {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    BigInt::from(dur.as_secs()) * BigInt::from(1_000_000_000i64) + BigInt::from(dur.subsec_nanos())
}

fn current_epoch_ms() -> i64 {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    dur.as_millis() as i64
}

fn epoch_ms_to_local_components(epoch_ms: i64) -> (i32, u8, u8, u8, u8, u8, u16) {
    use chrono::{Datelike, Local, TimeZone, Timelike, Utc};
    let utc = Utc.timestamp_millis_opt(epoch_ms).unwrap();
    let local = utc.with_timezone(&Local);
    (
        local.year() as i32,
        local.month() as u8,
        local.day() as u8,
        local.hour() as u8,
        local.minute() as u8,
        local.second() as u8,
        local.timestamp_subsec_millis() as u16,
    )
}
