use super::super::*;

impl Interpreter {
    pub(super) fn setup_global_functions(&mut self) {
        self.add_number_parsing_functions();
        self.add_number_test_functions();
        self.add_uri_functions();
        self.add_annex_b_escape_functions();
    }

    fn add_number_parsing_functions(&mut self) {
        // Global functions
        self.register_global_fn(
            "parseInt",
            BindingKind::Var,
            JsFunction::native("parseInt".to_string(), 2, |interp, _this, args| {
                let input = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let s = match interp.to_string_value(&input) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                let radix_val = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                // §19.2.5 step 6: Let R be 𝔽(? ToInt32(radix))
                let radix_num = match interp.to_number_value(&radix_val) {
                    Ok(n) => n,
                    Err(e) => return Completion::Throw(e),
                };
                let mut radix = number_ops::to_int32(radix_num);
                let s = s.trim_matches(crate::interpreter::helpers::is_ecma_whitespace);
                let (negative, s) = if let Some(rest) = s.strip_prefix('-') {
                    (true, rest)
                } else if let Some(rest) = s.strip_prefix('+') {
                    (false, rest)
                } else {
                    (false, s)
                };
                if radix == 0 {
                    if s.starts_with("0x") || s.starts_with("0X") {
                        radix = 16;
                    } else {
                        radix = 10;
                    }
                }
                if !(2..=36).contains(&radix) {
                    return Completion::Normal(JsValue::number(f64::NAN));
                }
                let s = if radix == 16 {
                    s.strip_prefix("0x")
                        .or_else(|| s.strip_prefix("0X"))
                        .unwrap_or(s)
                } else {
                    s
                };
                // §19.2.5 steps 11-12: Z is the longest prefix of radix-R digits
                let radix = radix as u32;
                let end = s.find(|c: char| !c.is_digit(radix)).unwrap_or(s.len());
                let digits = &s[..end];
                if digits.is_empty() {
                    return Completion::Normal(JsValue::number(f64::NAN));
                }
                let mut result =
                    crate::interpreter::helpers::prevalidated_radix_digits_to_f64(digits, radix);
                if negative {
                    result = -result;
                }
                Completion::Normal(JsValue::number(result))
            }),
        );

        self.register_global_fn(
            "parseFloat",
            BindingKind::Var,
            JsFunction::native("parseFloat".to_string(), 1, |interp, _this, args| {
                let input = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let s = match interp.to_string_value(&input) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                let s = s.trim_matches(crate::interpreter::helpers::is_ecma_whitespace);
                if s.is_empty() {
                    return Completion::Normal(JsValue::number(f64::NAN));
                }
                // Handle Infinity/-Infinity
                if s.starts_with("Infinity") || s.starts_with("+Infinity") {
                    return Completion::Normal(JsValue::number(f64::INFINITY));
                }
                if s.starts_with("-Infinity") {
                    return Completion::Normal(JsValue::number(f64::NEG_INFINITY));
                }
                // Find longest valid float prefix
                let mut end = 0;
                let mut has_dot = false;
                let mut has_e = false;
                let bytes = s.as_bytes();
                if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
                    end += 1;
                }
                while end < bytes.len() && bytes[end].is_ascii_digit() {
                    end += 1;
                }
                if end < bytes.len() && bytes[end] == b'.' {
                    has_dot = true;
                    end += 1;
                    while end < bytes.len() && bytes[end].is_ascii_digit() {
                        end += 1;
                    }
                }
                if end < bytes.len() && (bytes[end] == b'e' || bytes[end] == b'E') {
                    let saved = end;
                    has_e = true;
                    end += 1;
                    if end < bytes.len() && (bytes[end] == b'+' || bytes[end] == b'-') {
                        end += 1;
                    }
                    if end < bytes.len() && bytes[end].is_ascii_digit() {
                        while end < bytes.len() && bytes[end].is_ascii_digit() {
                            end += 1;
                        }
                    } else {
                        end = saved;
                        has_e = false;
                    }
                }
                let _ = (has_dot, has_e);
                let prefix = &s[..end];
                if prefix.is_empty() || prefix == "+" || prefix == "-" {
                    return Completion::Normal(JsValue::number(f64::NAN));
                }
                match prefix.parse::<f64>() {
                    Ok(n) => Completion::Normal(JsValue::number(n)),
                    Err(_) => Completion::Normal(JsValue::number(f64::NAN)),
                }
            }),
        );

        // Attach parseInt/parseFloat to Number constructor (must be after global registration)
        {
            let parse_int = self.get_global_var("parseInt");
            let parse_float = self.get_global_var("parseFloat");
            if let Some(num_val) = self.get_global_var("Number")
                && let Some(number_id) = num_val.as_object_id()
                && let Some(num_obj) = self.get_object_cell(number_id)
            {
                let mut n = num_obj.borrow_mut();
                if let Some(pi) = parse_int {
                    n.insert_builtin("parseInt".to_string(), pi);
                }
                if let Some(pf) = parse_float {
                    n.insert_builtin("parseFloat".to_string(), pf);
                }
            }
        }
    }

    fn add_number_test_functions(&mut self) {
        self.register_global_fn(
            "isNaN",
            BindingKind::Var,
            JsFunction::native("isNaN".to_string(), 1, |interp, _this, args| {
                let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let n = match interp.to_number_value(&val) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                Completion::Normal(JsValue::boolean(n.is_nan()))
            }),
        );

        self.register_global_fn(
            "isFinite",
            BindingKind::Var,
            JsFunction::native("isFinite".to_string(), 1, |interp, _this, args| {
                let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let n = match interp.to_number_value(&val) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                Completion::Normal(JsValue::boolean(n.is_finite()))
            }),
        );
    }

    fn add_uri_functions(&mut self) {
        self.register_global_fn(
            "encodeURI",
            BindingKind::Var,
            JsFunction::native("encodeURI".to_string(), 1, |interp, _this, args| {
                let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let code_units = match interp.to_js_string(&val) {
                    Ok(s) => s.code_units,
                    Err(e) => return Completion::Throw(e),
                };
                match encode_uri_string(&code_units, true) {
                    Ok(encoded) => {
                        Completion::Normal(JsValue::string(JsString::from_str(&encoded)))
                    }
                    Err(msg) => Completion::Throw(interp.create_error("URIError", &msg)),
                }
            }),
        );

        self.register_global_fn(
            "encodeURIComponent",
            BindingKind::Var,
            JsFunction::native(
                "encodeURIComponent".to_string(),
                1,
                |interp, _this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let code_units = match interp.to_js_string(&val) {
                        Ok(s) => s.code_units,
                        Err(e) => return Completion::Throw(e),
                    };
                    match encode_uri_string(&code_units, false) {
                        Ok(encoded) => {
                            Completion::Normal(JsValue::string(JsString::from_str(&encoded)))
                        }
                        Err(msg) => Completion::Throw(interp.create_error("URIError", &msg)),
                    }
                },
            ),
        );

        self.register_global_fn(
            "decodeURI",
            BindingKind::Var,
            JsFunction::native("decodeURI".to_string(), 1, |interp, _this, args| {
                let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let code_units = match interp.to_js_string(&val) {
                    Ok(s) => s.code_units.to_vec(),
                    Err(e) => return Completion::Throw(e),
                };
                match decode_uri_string(&code_units, true) {
                    Ok(decoded) => Completion::Normal(JsValue::string(JsString::from_vec(decoded))),
                    Err(msg) => Completion::Throw(interp.create_error("URIError", &msg)),
                }
            }),
        );

        self.register_global_fn(
            "decodeURIComponent",
            BindingKind::Var,
            JsFunction::native(
                "decodeURIComponent".to_string(),
                1,
                |interp, _this, args| {
                    let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let code_units = match interp.to_js_string(&val) {
                        Ok(s) => s.code_units.to_vec(),
                        Err(e) => return Completion::Throw(e),
                    };
                    match decode_uri_string(&code_units, false) {
                        Ok(decoded) => {
                            Completion::Normal(JsValue::string(JsString::from_vec(decoded)))
                        }
                        Err(msg) => Completion::Throw(interp.create_error("URIError", &msg)),
                    }
                },
            ),
        );
    }

    fn add_annex_b_escape_functions(&mut self) {
        // Annex B: escape()
        self.register_global_fn(
            "escape",
            BindingKind::Var,
            JsFunction::native("escape".to_string(), 1, |interp, _this, args| {
                let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let s = match interp.to_string_value(&val) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                let units: Vec<u16> = s.encode_utf16().collect();
                let mut result = String::new();
                for &cu in &units {
                    match cu {
                        b if (b'A' as u16..=b'Z' as u16).contains(&b)
                            || (b'a' as u16..=b'z' as u16).contains(&b)
                            || (b'0' as u16..=b'9' as u16).contains(&b)
                            || b == b'@' as u16
                            || b == b'*' as u16
                            || b == b'_' as u16
                            || b == b'+' as u16
                            || b == b'-' as u16
                            || b == b'.' as u16
                            || b == b'/' as u16 =>
                        {
                            result.push(cu as u8 as char);
                        }
                        b if b <= 0xFF => {
                            result.push_str(&format!("%{:02X}", b));
                        }
                        _ => {
                            result.push_str(&format!("%u{:04X}", cu));
                        }
                    }
                }
                Completion::Normal(JsValue::string(JsString::from_str(&result)))
            }),
        );

        // Annex B: unescape()
        self.register_global_fn(
            "unescape",
            BindingKind::Var,
            JsFunction::native("unescape".to_string(), 1, |interp, _this, args| {
                let val = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let s = match interp.to_string_value(&val) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                let chars: Vec<char> = s.chars().collect();
                let mut result: Vec<u16> = Vec::new();
                let mut i = 0;
                while i < chars.len() {
                    if chars[i] == '%' {
                        if i + 5 < chars.len()
                            && chars[i + 1] == 'u'
                            && chars[i + 2..i + 6].iter().all(|c| c.is_ascii_hexdigit())
                        {
                            let hex: String = chars[i + 2..i + 6].iter().collect();
                            if let Ok(code) = u16::from_str_radix(&hex, 16) {
                                result.push(code);
                                i += 6;
                                continue;
                            }
                        }
                        if i + 2 < chars.len()
                            && chars[i + 1..i + 3].iter().all(|c| c.is_ascii_hexdigit())
                        {
                            let hex: String = chars[i + 1..i + 3].iter().collect();
                            if let Ok(code) = u8::from_str_radix(&hex, 16) {
                                result.push(code as u16);
                                i += 3;
                                continue;
                            }
                        }
                    }
                    let ch = chars[i];
                    let mut buf = [0u16; 2];
                    for u in ch.encode_utf16(&mut buf) {
                        result.push(*u);
                    }
                    i += 1;
                }
                Completion::Normal(JsValue::string(JsString::from_vec(result)))
            }),
        );
    }
}
