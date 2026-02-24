use super::super::super::*;
use icu::list::ListFormatter;
use icu::list::options::{ListFormatterOptions, ListLength};
use icu::locale::Locale as IcuLocale;

pub(crate) fn create_list_formatter(
    locale_str: &str,
    list_type: &str,
    style: &str,
) -> ListFormatter {
    let locale: IcuLocale = locale_str.parse().unwrap_or_else(|_| "en".parse().unwrap());
    let length = match style {
        "short" => ListLength::Short,
        "narrow" => ListLength::Narrow,
        _ => ListLength::Wide,
    };
    let opts = ListFormatterOptions::default().with_length(length);
    let prefs = (&locale).into();
    match list_type {
        "disjunction" => ListFormatter::try_new_or(prefs, opts),
        "unit" => ListFormatter::try_new_unit(prefs, opts),
        _ => ListFormatter::try_new_and(prefs, opts),
    }
    .unwrap_or_else(|_| {
        let fallback: IcuLocale = "en".parse().unwrap();
        let fallback_prefs = (&fallback).into();
        match list_type {
            "disjunction" => ListFormatter::try_new_or(fallback_prefs, opts),
            "unit" => ListFormatter::try_new_unit(fallback_prefs, opts),
            _ => ListFormatter::try_new_and(fallback_prefs, opts),
        }
        .unwrap()
    })
}

fn string_list_from_iterable(
    interp: &mut Interpreter,
    iterable: &JsValue,
) -> Result<Vec<String>, JsValue> {
    if matches!(iterable, JsValue::Undefined) {
        return Ok(Vec::new());
    }

    let iterator = interp.get_iterator(iterable)?;
    let mut list = Vec::new();
    loop {
        let next = interp.iterator_step(&iterator)?;
        match next {
            None => break,
            Some(result) => {
                let value = interp.iterator_value(&result)?;
                if let JsValue::String(s) = &value {
                    list.push(s.to_rust_string());
                } else {
                    let err = interp.create_type_error("Iterable yielded a non-string value");
                    interp.iterator_close(&iterator, err.clone());
                    return Err(err);
                }
            }
        }
    }
    Ok(list)
}

fn format_list_to_parts(formatter: &ListFormatter, elements: &[String]) -> Vec<(String, String)> {
    if elements.is_empty() {
        return Vec::new();
    }
    if elements.len() == 1 {
        return vec![("element".to_string(), elements[0].clone())];
    }

    // Use unique placeholders to identify element boundaries in the formatted output.
    // Format with placeholders that won't appear in any real locale data.
    let placeholder_prefix = "\x01\x02";
    let placeholder_suffix = "\x03\x04";
    let placeholders: Vec<String> = (0..elements.len())
        .map(|i| format!("{}{}{}", placeholder_prefix, i, placeholder_suffix))
        .collect();

    let formatted_with_ph = formatter.format_to_string(placeholders.iter().map(|s| s.as_str()));

    let mut parts: Vec<(String, String)> = Vec::new();
    let mut remaining = formatted_with_ph.as_str();

    for (i, element) in elements.iter().enumerate() {
        let ph = &placeholders[i];
        if let Some(pos) = remaining.find(ph.as_str()) {
            if pos > 0 {
                parts.push(("literal".to_string(), remaining[..pos].to_string()));
            }
            parts.push(("element".to_string(), element.clone()));
            remaining = &remaining[pos + ph.len()..];
        }
    }
    if !remaining.is_empty() {
        parts.push(("literal".to_string(), remaining.to_string()));
    }

    parts
}

impl Interpreter {
    pub(crate) fn setup_intl_list_format(&mut self, intl_obj: &Rc<RefCell<JsObjectData>>) {
        let proto = self.create_object();
        if let Some(ref op) = self.object_prototype {
            proto.borrow_mut().prototype = Some(op.clone());
        }
        proto.borrow_mut().class_name = "Intl.ListFormat".to_string();

        // @@toStringTag
        proto.borrow_mut().insert_property(
            "Symbol(Symbol.toStringTag)".to_string(),
            PropertyDescriptor {
                value: Some(JsValue::String(JsString::from_str("Intl.ListFormat"))),
                writable: Some(false),
                enumerable: Some(false),
                configurable: Some(true),
                get: None,
                set: None,
            },
        );

        // format(list)
        let format_fn = self.create_function(JsFunction::native(
            "format".to_string(),
            1,
            |interp, this, args| {
                let (locale, list_type, style) = match extract_list_format_data(interp, this) {
                    Ok(data) => data,
                    Err(e) => return Completion::Throw(e),
                };

                let iterable = args.first().cloned().unwrap_or(JsValue::Undefined);
                let string_list = match string_list_from_iterable(interp, &iterable) {
                    Ok(list) => list,
                    Err(e) => return Completion::Throw(e),
                };

                if string_list.is_empty() {
                    return Completion::Normal(JsValue::String(JsString::from_str("")));
                }

                let formatter = create_list_formatter(&locale, &list_type, &style);
                let result = formatter.format_to_string(string_list.iter().map(|s| s.as_str()));
                Completion::Normal(JsValue::String(JsString::from_str(&result)))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("format".to_string(), format_fn);

        // formatToParts(list)
        let format_to_parts_fn = self.create_function(JsFunction::native(
            "formatToParts".to_string(),
            1,
            |interp, this, args| {
                let (locale, list_type, style) = match extract_list_format_data(interp, this) {
                    Ok(data) => data,
                    Err(e) => return Completion::Throw(e),
                };

                let iterable = args.first().cloned().unwrap_or(JsValue::Undefined);
                let string_list = match string_list_from_iterable(interp, &iterable) {
                    Ok(list) => list,
                    Err(e) => return Completion::Throw(e),
                };

                if string_list.is_empty() {
                    return Completion::Normal(interp.create_array(Vec::new()));
                }

                let formatter = create_list_formatter(&locale, &list_type, &style);
                let parts = format_list_to_parts(&formatter, &string_list);

                let js_parts: Vec<JsValue> = parts
                    .into_iter()
                    .map(|(ptype, value)| {
                        let part_obj = interp.create_object();
                        if let Some(ref op) = interp.object_prototype {
                            part_obj.borrow_mut().prototype = Some(op.clone());
                        }
                        part_obj.borrow_mut().insert_property(
                            "type".to_string(),
                            PropertyDescriptor::data(
                                JsValue::String(JsString::from_str(&ptype)),
                                true,
                                true,
                                true,
                            ),
                        );
                        part_obj.borrow_mut().insert_property(
                            "value".to_string(),
                            PropertyDescriptor::data(
                                JsValue::String(JsString::from_str(&value)),
                                true,
                                true,
                                true,
                            ),
                        );
                        let id = part_obj.borrow().id.unwrap();
                        JsValue::Object(crate::types::JsObject { id })
                    })
                    .collect();

                Completion::Normal(interp.create_array(js_parts))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("formatToParts".to_string(), format_to_parts_fn);

        // resolvedOptions()
        let resolved_fn = self.create_function(JsFunction::native(
            "resolvedOptions".to_string(),
            0,
            |interp, this, _args| {
                let (locale, list_type, style) = match extract_list_format_data(interp, this) {
                    Ok(data) => data,
                    Err(e) => return Completion::Throw(e),
                };

                let result = interp.create_object();
                if let Some(ref op) = interp.object_prototype {
                    result.borrow_mut().prototype = Some(op.clone());
                }

                let props = vec![
                    ("locale", JsValue::String(JsString::from_str(&locale))),
                    ("type", JsValue::String(JsString::from_str(&list_type))),
                    ("style", JsValue::String(JsString::from_str(&style))),
                ];
                for (key, val) in props {
                    result.borrow_mut().insert_property(
                        key.to_string(),
                        PropertyDescriptor::data(val, true, true, true),
                    );
                }

                let result_id = result.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(crate::types::JsObject { id: result_id }))
            },
        ));
        proto
            .borrow_mut()
            .insert_builtin("resolvedOptions".to_string(), resolved_fn);

        self.intl_list_format_prototype = Some(proto.clone());

        // --- Constructor ---
        let proto_id = proto.borrow().id.unwrap();
        let proto_val = JsValue::Object(crate::types::JsObject { id: proto_id });
        let proto_clone = proto.clone();

        let list_format_ctor = self.create_function(JsFunction::constructor(
            "ListFormat".to_string(),
            0,
            move |interp, _this, args| {
                if interp.new_target.is_none() {
                    return Completion::Throw(
                        interp.create_type_error("Constructor Intl.ListFormat requires 'new'"),
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

                let list_type = match interp.intl_get_option(
                    &options,
                    "type",
                    &["conjunction", "disjunction", "unit"],
                    Some("conjunction"),
                ) {
                    Ok(Some(v)) => v,
                    Ok(None) => "conjunction".to_string(),
                    Err(e) => return Completion::Throw(e),
                };

                let style = match interp.intl_get_option(
                    &options,
                    "style",
                    &["long", "short", "narrow"],
                    Some("long"),
                ) {
                    Ok(Some(v)) => v,
                    Ok(None) => "long".to_string(),
                    Err(e) => return Completion::Throw(e),
                };

                let locale = interp.intl_resolve_locale(&requested);

                let obj = interp.create_object();
                obj.borrow_mut().prototype = Some(proto_clone.clone());
                obj.borrow_mut().class_name = "Intl.ListFormat".to_string();
                obj.borrow_mut().intl_data = Some(IntlData::ListFormat {
                    locale,
                    list_type,
                    style,
                });

                let obj_id = obj.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(crate::types::JsObject { id: obj_id }))
            },
        ));

        // Set ListFormat.prototype on constructor
        if let JsValue::Object(ctor_ref) = &list_format_ctor {
            if let Some(obj) = self.get_object(ctor_ref.id) {
                obj.borrow_mut().insert_property(
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
                obj.borrow_mut()
                    .insert_builtin("supportedLocalesOf".to_string(), slof);
            }
        }

        // Set constructor on prototype
        proto.borrow_mut().insert_property(
            "constructor".to_string(),
            PropertyDescriptor::data(list_format_ctor.clone(), true, false, true),
        );

        // Register Intl.ListFormat on the Intl namespace
        intl_obj.borrow_mut().insert_property(
            "ListFormat".to_string(),
            PropertyDescriptor::data(list_format_ctor, true, false, true),
        );
    }
}

fn extract_list_format_data(
    interp: &mut Interpreter,
    this: &JsValue,
) -> Result<(String, String, String), JsValue> {
    if let JsValue::Object(o) = this {
        if let Some(obj) = interp.get_object(o.id) {
            let b = obj.borrow();
            if let Some(IntlData::ListFormat {
                ref locale,
                ref list_type,
                ref style,
            }) = b.intl_data
            {
                return Ok((locale.clone(), list_type.clone(), style.clone()));
            }
        }
    }
    Err(interp.create_type_error("Intl.ListFormat method called on incompatible receiver"))
}
