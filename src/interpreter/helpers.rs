use super::*;

pub(crate) fn to_integer_or_infinity(n: f64) -> f64 {
    if n.is_nan() || n == 0.0 {
        0.0
    } else if n.is_infinite() {
        n
    } else {
        n.trunc()
    }
}

pub(crate) fn format_radix(mut n: i64, radix: u32) -> String {
    if !(2..=36).contains(&radix) {
        return n.to_string();
    }
    if n == 0 {
        return "0".to_string();
    }
    let negative = n < 0;
    if negative {
        n = -n;
    }
    let mut digits = Vec::new();
    while n > 0 {
        let d = (n % radix as i64) as u32;
        digits.push(char::from_digit(d, radix).unwrap_or('?'));
        n /= radix as i64;
    }
    if negative {
        digits.push('-');
    }
    digits.iter().rev().collect()
}

// §7.1.3 ToBoolean
pub(crate) fn to_boolean(val: &JsValue) -> bool {
    match val {
        JsValue::Undefined | JsValue::Null => false,
        JsValue::Boolean(b) => *b,
        JsValue::Number(n) => *n != 0.0 && !n.is_nan(),
        JsValue::String(s) => !s.is_empty(),
        JsValue::BigInt(_) => true,
        JsValue::Symbol(_) | JsValue::Object(_) => true,
    }
}

// §7.1.4 ToNumber
pub(crate) fn to_number(val: &JsValue) -> f64 {
    match val {
        JsValue::Undefined => f64::NAN,
        JsValue::Null => 0.0,
        JsValue::Boolean(b) => *b as u8 as f64,
        JsValue::Number(n) => *n,
        JsValue::String(s) => string_to_number(s),
        _ => f64::NAN,
    }
}

// §7.1.4.1.1 StringToNumber (uses §7.1.4.1.2 RoundMVResult via f64::parse)
fn string_to_number(s: &JsString) -> f64 {
    let rust_str = s.to_rust_string();
    let trimmed = rust_str.trim();
    if trimmed.is_empty() {
        return 0.0;
    }
    if trimmed.starts_with("0x") || trimmed.starts_with("0X") {
        return i64::from_str_radix(&trimmed[2..], 16)
            .map(|n| n as f64)
            .unwrap_or(f64::NAN);
    }
    if trimmed.starts_with("0o") || trimmed.starts_with("0O") {
        return i64::from_str_radix(&trimmed[2..], 8)
            .map(|n| n as f64)
            .unwrap_or(f64::NAN);
    }
    if trimmed.starts_with("0b") || trimmed.starts_with("0B") {
        return i64::from_str_radix(&trimmed[2..], 2)
            .map(|n| n as f64)
            .unwrap_or(f64::NAN);
    }
    trimmed.parse::<f64>().unwrap_or(f64::NAN)
}

pub(crate) fn to_js_string(val: &JsValue) -> String {
    format!("{val}")
}

pub(crate) fn is_string(val: &JsValue) -> bool {
    matches!(val, JsValue::String(_))
}

pub(crate) fn get_set_record(
    interp: &mut Interpreter,
    obj: &JsValue,
) -> Result<SetRecord, JsValue> {
    if !matches!(obj, JsValue::Object(_)) {
        return Err(interp.create_type_error("GetSetRecord requires an object"));
    }
    let o = if let JsValue::Object(o) = obj {
        o
    } else {
        unreachable!()
    };
    let obj_rc = interp
        .get_object(o.id)
        .ok_or_else(|| interp.create_type_error("invalid object"))?;

    // Get size via getter - read the property descriptor
    let size_val = {
        let borrowed = obj_rc.borrow();
        let desc = borrowed.get_property_descriptor("size");
        match desc {
            Some(ref d) if d.get.is_some() => {
                let getter = d.get.clone().unwrap();
                drop(borrowed);
                match interp.call_function(&getter, obj, &[]) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::Undefined,
                }
            }
            _ => borrowed.get_property("size"),
        }
    };
    let size = to_number(&size_val);
    if size.is_nan() {
        return Err(interp.create_type_error("Set-like size is not a number"));
    }
    if size < 0.0 {
        return Err(interp.create_type_error("Set-like size is negative"));
    }

    let has = obj_rc.borrow().get_property("has");
    if !matches!(&has, JsValue::Object(ho) if interp.get_object(ho.id).is_some_and(|o| o.borrow().callable.is_some()))
    {
        return Err(interp.create_type_error("Set-like object must have a callable has method"));
    }

    let keys = obj_rc.borrow().get_property("keys");
    if !matches!(&keys, JsValue::Object(ko) if interp.get_object(ko.id).is_some_and(|o| o.borrow().callable.is_some()))
    {
        return Err(interp.create_type_error("Set-like object must have a callable keys method"));
    }

    Ok(SetRecord { has, keys, size })
}

pub(crate) fn extract_iter_result(interp: &Interpreter, result: &JsValue) -> (bool, JsValue) {
    if let JsValue::Object(ro) = result
        && let Some(result_obj) = interp.get_object(ro.id)
    {
        let borrowed = result_obj.borrow();
        let done = matches!(borrowed.get_property("done"), JsValue::Boolean(true));
        let value = borrowed.get_property("value");
        return (done, value);
    }
    (true, JsValue::Undefined)
}

pub(crate) fn same_value(left: &JsValue, right: &JsValue) -> bool {
    match (left, right) {
        (JsValue::Number(a), JsValue::Number(b)) => {
            if a.is_nan() && b.is_nan() {
                return true;
            }
            if *a == 0.0 && *b == 0.0 {
                return a.is_sign_positive() == b.is_sign_positive();
            }
            a == b
        }
        _ => strict_equality(left, right),
    }
}

pub(crate) fn same_value_zero(left: &JsValue, right: &JsValue) -> bool {
    match (left, right) {
        (JsValue::Number(a), JsValue::Number(b)) => {
            if a.is_nan() && b.is_nan() {
                return true;
            }
            // -0 === +0
            a == b
        }
        _ => strict_equality(left, right),
    }
}

pub(crate) fn strict_equality(left: &JsValue, right: &JsValue) -> bool {
    match (left, right) {
        (JsValue::Undefined, JsValue::Undefined) => true,
        (JsValue::Null, JsValue::Null) => true,
        (JsValue::Boolean(a), JsValue::Boolean(b)) => a == b,
        (JsValue::Number(a), JsValue::Number(b)) => number_ops::equal(*a, *b),
        (JsValue::String(a), JsValue::String(b)) => a == b,
        (JsValue::Symbol(a), JsValue::Symbol(b)) => a.id == b.id,
        (JsValue::Object(a), JsValue::Object(b)) => a.id == b.id,
        _ => false,
    }
}

pub(crate) fn abstract_equality(left: &JsValue, right: &JsValue) -> bool {
    // Same type
    if std::mem::discriminant(left) == std::mem::discriminant(right) {
        return strict_equality(left, right);
    }
    // null == undefined
    if (left.is_null() && right.is_undefined()) || (left.is_undefined() && right.is_null()) {
        return true;
    }
    // Number vs String
    if left.is_number() && right.is_string() {
        return abstract_equality(left, &JsValue::Number(to_number(right)));
    }
    if left.is_string() && right.is_number() {
        return abstract_equality(&JsValue::Number(to_number(left)), right);
    }
    // Boolean coercion
    if left.is_boolean() {
        return abstract_equality(&JsValue::Number(to_number(left)), right);
    }
    if right.is_boolean() {
        return abstract_equality(left, &JsValue::Number(to_number(right)));
    }
    false
}

pub(crate) fn abstract_relational(left: &JsValue, right: &JsValue) -> Option<bool> {
    if is_string(left) && is_string(right) {
        let ls = to_js_string(left);
        let rs = to_js_string(right);
        return Some(ls < rs);
    }
    let ln = to_number(left);
    let rn = to_number(right);
    number_ops::less_than(ln, rn)
}

pub(crate) fn typeof_val<'a>(
    val: &JsValue,
    objects: &[Option<Rc<RefCell<JsObjectData>>>],
) -> &'a str {
    match val {
        JsValue::Undefined => "undefined",
        JsValue::Null => "object",
        JsValue::Boolean(_) => "boolean",
        JsValue::Number(_) => "number",
        JsValue::String(_) => "string",
        JsValue::Symbol(_) => "symbol",
        JsValue::BigInt(_) => "bigint",
        JsValue::Object(o) => {
            if let Some(Some(obj)) = objects.get(o.id as usize)
                && obj.borrow().callable.is_some()
            {
                return "function";
            }
            "object"
        }
    }
}

fn json_quote(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 2);
    result.push('"');
    for ch in s.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\u{0008}' => result.push_str("\\b"),
            '\u{000C}' => result.push_str("\\f"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c < '\u{0020}' => {
                result.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => result.push(c),
        }
    }
    result.push('"');
    result
}

pub(crate) fn json_stringify_value(interp: &mut Interpreter, val: &JsValue) -> Option<String> {
    let mut stack = Vec::new();
    json_stringify_internal(interp, None, "", val, &mut stack, &None, &None, "", "").unwrap_or_default()
}

pub(crate) fn json_stringify_full(
    interp: &mut Interpreter,
    val: &JsValue,
    replacer: &Option<JsValue>,
    space: &str,
) -> Result<Option<String>, JsValue> {
    let mut stack = Vec::new();
    let mut property_list: Option<Vec<String>> = None;
    let mut replacer_fn: Option<JsValue> = None;

    if let Some(rep) = replacer {
        if let JsValue::Object(o) = rep {
            if let Some(obj) = interp.get_object(o.id) {
                if obj.borrow().callable.is_some() {
                    replacer_fn = Some(rep.clone());
                } else if obj.borrow().class_name == "Array" {
                    let mut keys = Vec::new();
                    let len = obj.borrow().get_property_value("length")
                        .and_then(|v| if let JsValue::Number(n) = v { Some(n as usize) } else { None })
                        .unwrap_or(0);
                    for i in 0..len {
                        let item = obj.borrow().get_property(&i.to_string());
                        let key_str = match &item {
                            JsValue::String(s) => Some(s.to_rust_string()),
                            JsValue::Number(n) => Some(number_ops::to_string(*n)),
                            JsValue::Object(oo) => {
                                if let Some(inner) = interp.get_object(oo.id) {
                                    let cn = inner.borrow().class_name.clone();
                                    if cn == "String" || cn == "Number" {
                                        inner.borrow().primitive_value.as_ref().map(to_js_string)
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            }
                            _ => None,
                        };
                        if let Some(k) = key_str
                            && !keys.contains(&k) {
                                keys.push(k);
                            }
                    }
                    property_list = Some(keys);
                }
            }
        }
    }

    // Wrap root value in {"": val} for replacer function calls
    let holder_id = if replacer_fn.is_some() {
        let wrapper = interp.create_object();
        wrapper.borrow_mut().insert_value("".to_string(), val.clone());
        Some(wrapper.borrow().id.unwrap())
    } else {
        None
    };

    json_stringify_internal(
        interp, holder_id, "", val, &mut stack,
        &replacer_fn, &property_list, space, "",
    )
}

fn json_stringify_internal(
    interp: &mut Interpreter,
    holder_id: Option<u64>,
    key: &str,
    val: &JsValue,
    stack: &mut Vec<u64>,
    replacer_fn: &Option<JsValue>,
    property_list: &Option<Vec<String>>,
    gap: &str,
    indent: &str,
) -> Result<Option<String>, JsValue> {
    let mut value = val.clone();

    // If value is object, check for toJSON
    if let JsValue::Object(o) = &value
        && let Some(obj) = interp.get_object(o.id) {
            let to_json = obj.borrow().get_property("toJSON");
            if let JsValue::Object(fobj) = &to_json
                && let Some(fdata) = interp.get_object(fobj.id)
                    && fdata.borrow().callable.is_some() {
                        let key_val = JsValue::String(JsString::from_str(key));
                        match interp.call_function(&to_json, &value, &[key_val]) {
                            Completion::Normal(v) => value = v,
                            Completion::Throw(e) => return Err(e),
                            _ => {}
                        }
                    }
        }

    // Apply replacer function
    if let Some(rep) = replacer_fn
        && let Some(hid) = holder_id {
            let holder_val = JsValue::Object(crate::types::JsObject { id: hid });
            let key_val = JsValue::String(JsString::from_str(key));
            match interp.call_function(rep, &holder_val, &[key_val, value.clone()]) {
                Completion::Normal(v) => value = v,
                Completion::Throw(e) => return Err(e),
                _ => {}
            }
        }

    // Unwrap wrapper objects (Number, String, Boolean, BigInt)
    if let JsValue::Object(o) = &value
        && let Some(obj) = interp.get_object(o.id) {
            let class = obj.borrow().class_name.clone();
            let pv = obj.borrow().primitive_value.clone();
            match class.as_str() {
                "Number" => {
                    if let Some(pv) = pv {
                        value = JsValue::Number(to_number(&pv));
                    }
                }
                "String" => {
                    if let Some(pv) = pv {
                        value = JsValue::String(match pv {
                            JsValue::String(s) => s,
                            other => JsString::from_str(&to_js_string(&other)),
                        });
                    }
                }
                "Boolean" => {
                    if let Some(pv) = pv {
                        value = pv;
                    }
                }
                "BigInt" => {
                    if let Some(pv) = pv {
                        value = pv;
                    }
                }
                _ => {}
            }
        }

    // Type dispatch
    match &value {
        JsValue::Null => Ok(Some("null".to_string())),
        JsValue::Boolean(b) => Ok(Some(b.to_string())),
        JsValue::Number(n) => {
            if n.is_nan() || n.is_infinite() {
                Ok(Some("null".to_string()))
            } else {
                Ok(Some(number_ops::to_string(*n)))
            }
        }
        JsValue::String(s) => Ok(Some(json_quote(&s.to_rust_string()))),
        JsValue::BigInt(_) => {
            Err(interp.create_error("TypeError", "Do not know how to serialize a BigInt"))
        }
        JsValue::Object(o) => {
            if let Some(obj) = interp.get_object(o.id) {
                // Check for rawJSON
                if obj.borrow().is_raw_json
                    && let Some(raw) = obj.borrow().get_property_value("rawJSON") {
                        return Ok(Some(to_js_string(&raw)));
                    }

                // Skip functions and symbols
                if obj.borrow().callable.is_some() {
                    return Ok(None);
                }

                let obj_id = obj.borrow().id.unwrap();

                // Circular reference check
                if stack.contains(&obj_id) {
                    return Err(interp.create_error(
                        "TypeError",
                        "Converting circular structure to JSON",
                    ));
                }
                stack.push(obj_id);

                let is_array = obj.borrow().class_name == "Array";
                let new_indent = format!("{}{}", indent, gap);

                let result = if is_array {
                    let len = obj.borrow().get_property_value("length")
                        .and_then(|v| if let JsValue::Number(n) = v { Some(n as usize) } else { None })
                        .unwrap_or(0);
                    let mut items = Vec::new();
                    // Create a holder for this array for replacer calls
                    let arr_holder_id = Some(obj_id);
                    for i in 0..len {
                        let v = obj.borrow().get_property(&i.to_string());
                        match json_stringify_internal(
                            interp, arr_holder_id, &i.to_string(), &v, stack,
                            replacer_fn, property_list, gap, &new_indent,
                        )? {
                            Some(s) => items.push(s),
                            None => items.push("null".to_string()),
                        }
                    }
                    if items.is_empty() {
                        Ok(Some("[]".to_string()))
                    } else if gap.is_empty() {
                        Ok(Some(format!("[{}]", items.join(","))))
                    } else {
                        let sep = format!(",\n{}", new_indent);
                        Ok(Some(format!("[\n{}{}\n{}]", new_indent, items.join(&sep), indent)))
                    }
                } else {
                    // Object
                    let keys: Vec<String> = if let Some(pl) = property_list {
                        pl.clone()
                    } else {
                        obj.borrow().property_order.clone()
                    };

                    let mut entries = Vec::new();
                    let obj_holder_id = Some(obj_id);
                    for k in &keys {
                        let desc = obj.borrow().properties.get(k).cloned();
                        if let Some(d) = desc {
                            if d.enumerable != Some(true) && property_list.is_none() {
                                continue;
                            }
                            let v = d.value.clone().unwrap_or(JsValue::Undefined);
                            match json_stringify_internal(
                                interp, obj_holder_id, k, &v, stack,
                                replacer_fn, property_list, gap, &new_indent,
                            )? {
                                Some(sv) => {
                                    let quoted_key = json_quote(k);
                                    if gap.is_empty() {
                                        entries.push(format!("{}:{}", quoted_key, sv));
                                    } else {
                                        entries.push(format!("{}: {}", quoted_key, sv));
                                    }
                                }
                                None => {} // omit undefined/function/symbol
                            }
                        }
                    }
                    if entries.is_empty() {
                        Ok(Some("{}".to_string()))
                    } else if gap.is_empty() {
                        Ok(Some(format!("{{{}}}", entries.join(","))))
                    } else {
                        let sep = format!(",\n{}", new_indent);
                        Ok(Some(format!("{{\n{}{}\n{}}}", new_indent, entries.join(&sep), indent)))
                    }
                };

                stack.pop();
                result
            } else {
                Ok(Some("null".to_string()))
            }
        }
        JsValue::Undefined | JsValue::Symbol(_) => Ok(None),
    }
}

fn json_trim(s: &str) -> &str {
    let bytes = s.as_bytes();
    let mut start = 0;
    while start < bytes.len() {
        match bytes[start] {
            b' ' | b'\t' | b'\n' | b'\r' => start += 1,
            _ => break,
        }
    }
    let mut end = bytes.len();
    while end > start {
        match bytes[end - 1] {
            b' ' | b'\t' | b'\n' | b'\r' => end -= 1,
            _ => break,
        }
    }
    &s[start..end]
}

pub(crate) fn json_parse_value(interp: &mut Interpreter, s: &str) -> Completion {
    let s = json_trim(s);
    if s == "null" {
        return Completion::Normal(JsValue::Null);
    }
    if s == "true" {
        return Completion::Normal(JsValue::Boolean(true));
    }
    if s == "false" {
        return Completion::Normal(JsValue::Boolean(false));
    }
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        let inner = &s[1..s.len() - 1];
        if let Err(msg) = json_validate_string(inner) {
            let err = interp.create_error("SyntaxError", &msg);
            return Completion::Throw(err);
        }
        let unescaped = json_unescape_string(inner);
        return Completion::Normal(JsValue::String(JsString::from_str(&unescaped)));
    }
    if let Ok(n) = s.parse::<f64>() {
        return Completion::Normal(JsValue::Number(n));
    }
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1];
        let items = json_split_items(inner);
        let mut vals = Vec::new();
        for item in &items {
            match json_parse_value(interp, item) {
                Completion::Normal(v) => vals.push(v),
                other => return other,
            }
        }
        return Completion::Normal(interp.create_array(vals));
    }
    if s.starts_with('{') && s.ends_with('}') {
        let inner = &s[1..s.len() - 1];
        let pairs = json_split_items(inner);
        let obj = interp.create_object();
        for pair in &pairs {
            let pair = pair.trim();
            if pair.is_empty() {
                continue;
            }
            if let Some(colon_pos) = find_json_colon(pair) {
                let key_str = pair[..colon_pos].trim();
                let val_str = pair[colon_pos + 1..].trim();
                let key =
                    if key_str.starts_with('"') && key_str.ends_with('"') && key_str.len() >= 2 {
                        let inner = &key_str[1..key_str.len() - 1];
                        if json_validate_string(inner).is_err() {
                            let err = interp.create_error("SyntaxError", "Unexpected token in JSON");
                            return Completion::Throw(err);
                        }
                        json_unescape_string(inner)
                    } else {
                        let err = interp.create_error("SyntaxError", "Unexpected token in JSON");
                        return Completion::Throw(err);
                    };
                match json_parse_value(interp, val_str) {
                    Completion::Normal(v) => {
                        obj.borrow_mut().insert_value(key, v);
                    }
                    other => return other,
                }
            }
        }
        let id = obj.borrow().id.unwrap();
        return Completion::Normal(JsValue::Object(crate::types::JsObject { id }));
    }
    let err = interp.create_error("SyntaxError", "Unexpected token in JSON");
    Completion::Throw(err)
}

fn json_validate_string(s: &str) -> Result<(), String> {
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch < '\u{0020}' {
            return Err("Unexpected token in JSON".to_string());
        }
        if ch == '\\' {
            match chars.next() {
                Some('"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't') => {}
                Some('u') => {
                    for _ in 0..4 {
                        match chars.next() {
                            Some(c) if c.is_ascii_hexdigit() => {}
                            _ => return Err("Unexpected token in JSON".to_string()),
                        }
                    }
                }
                _ => return Err("Unexpected token in JSON".to_string()),
            }
        }
    }
    Ok(())
}

fn json_unescape_string(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('"') => result.push('"'),
                Some('\\') => result.push('\\'),
                Some('/') => result.push('/'),
                Some('b') => result.push('\u{0008}'),
                Some('f') => result.push('\u{000C}'),
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Ok(code) = u32::from_str_radix(&hex, 16)
                        && let Some(c) = char::from_u32(code) {
                            result.push(c);
                        }
                }
                Some(c) => {
                    result.push('\\');
                    result.push(c);
                }
                None => result.push('\\'),
            }
        } else {
            result.push(ch);
        }
    }
    result
}

fn json_internalize_apply(interp: &mut Interpreter, obj_id: u64, key: &str, new_val: JsValue) {
    if let Some(obj) = interp.get_object(obj_id) {
        let configurable = obj.borrow().properties.get(key)
            .and_then(|d| d.configurable)
            .unwrap_or(true);
        if !configurable {
            return;
        }
        if let JsValue::Undefined = &new_val {
            obj.borrow_mut().properties.remove(key);
            obj.borrow_mut().property_order.retain(|k| k != key);
        } else {
            obj.borrow_mut().insert_value(key.to_string(), new_val);
        }
    }
}

pub(crate) fn json_internalize(
    interp: &mut Interpreter,
    holder: &JsValue,
    name: &str,
    reviver: &JsValue,
) -> Completion {
    let val = if let JsValue::Object(o) = holder {
        if let Some(obj) = interp.get_object(o.id) {
            obj.borrow().get_property(name)
        } else {
            JsValue::Undefined
        }
    } else {
        JsValue::Undefined
    };

    let walked = if let JsValue::Object(o) = &val {
        if let Some(obj) = interp.get_object(o.id) {
            let is_array = obj.borrow().class_name == "Array";
            if is_array {
                let len = obj.borrow().get_property_value("length")
                    .and_then(|v| if let JsValue::Number(n) = v { Some(n as usize) } else { None })
                    .unwrap_or(0);
                for i in 0..len {
                    let key = i.to_string();
                    match json_internalize(interp, &val, &key, reviver) {
                        Completion::Normal(new_val) => {
                            json_internalize_apply(interp, o.id, &key, new_val);
                        }
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => {}
                    }
                }
            } else {
                let keys: Vec<String> = obj.borrow().property_order.clone();
                for key in keys {
                    match json_internalize(interp, &val, &key, reviver) {
                        Completion::Normal(new_val) => {
                            json_internalize_apply(interp, o.id, &key, new_val);
                        }
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => {}
                    }
                }
            }
        }
        val.clone()
    } else {
        val.clone()
    };

    let key_val = JsValue::String(JsString::from_str(name));
    interp.call_function(reviver, holder, &[key_val, walked])
}

pub(crate) fn json_split_items(s: &str) -> Vec<String> {
    let mut items = Vec::new();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    let mut current = String::new();
    for ch in s.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }
        if ch == '\\' && in_string {
            current.push(ch);
            escape = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            current.push(ch);
            continue;
        }
        if in_string {
            current.push(ch);
            continue;
        }
        match ch {
            '[' | '{' => {
                depth += 1;
                current.push(ch);
            }
            ']' | '}' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    items.push(trimmed);
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        items.push(trimmed);
    }
    items
}

// === Date spec helper functions ===

pub(crate) const MS_PER_DAY: f64 = 86_400_000.0;

pub(crate) fn day(t: f64) -> f64 {
    (t / MS_PER_DAY).floor()
}

pub(crate) fn time_within_day(t: f64) -> f64 {
    t.rem_euclid(MS_PER_DAY)
}

pub(crate) fn days_in_year(y: f64) -> f64 {
    let y = y as i64;
    if y % 4 != 0 {
        365.0
    } else if y % 100 != 0 {
        366.0
    } else if y % 400 != 0 {
        365.0
    } else {
        366.0
    }
}

pub(crate) fn day_from_year(y: f64) -> f64 {
    let y = y as i64;
    365.0 * (y - 1970) as f64 + ((y - 1969) / 4) as f64 - ((y - 1901) / 100) as f64
        + ((y - 1601) / 400) as f64
}

pub(crate) fn time_from_year(y: f64) -> f64 {
    day_from_year(y) * MS_PER_DAY
}

pub(crate) fn year_from_time(t: f64) -> f64 {
    if t.is_nan() || t.is_infinite() {
        return f64::NAN;
    }
    let mut lo = (t / MS_PER_DAY / 366.0 + 1970.0).floor() as i64 - 1;
    let mut hi = (t / MS_PER_DAY / 365.0 + 1970.0).ceil() as i64 + 1;
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if time_from_year(mid as f64) <= t {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    (lo - 1) as f64
}

pub(crate) fn in_leap_year(t: f64) -> bool {
    days_in_year(year_from_time(t)) == 366.0
}

pub(crate) fn day_within_year(t: f64) -> f64 {
    day(t) - day_from_year(year_from_time(t))
}

pub(crate) fn month_from_time(t: f64) -> f64 {
    let d = day_within_year(t) as i32;
    let leap = in_leap_year(t) as i32;
    match d {
        d if d < 31 => 0.0,
        d if d < 59 + leap => 1.0,
        d if d < 90 + leap => 2.0,
        d if d < 120 + leap => 3.0,
        d if d < 151 + leap => 4.0,
        d if d < 181 + leap => 5.0,
        d if d < 212 + leap => 6.0,
        d if d < 243 + leap => 7.0,
        d if d < 273 + leap => 8.0,
        d if d < 304 + leap => 9.0,
        d if d < 334 + leap => 10.0,
        _ => 11.0,
    }
}

pub(crate) fn date_from_time(t: f64) -> f64 {
    let d = day_within_year(t) as i32;
    let leap = in_leap_year(t) as i32;
    let m = month_from_time(t) as i32;
    (match m {
        0 => d + 1,
        1 => d - 30,
        2 => d - 58 - leap,
        3 => d - 89 - leap,
        4 => d - 119 - leap,
        5 => d - 150 - leap,
        6 => d - 180 - leap,
        7 => d - 211 - leap,
        8 => d - 242 - leap,
        9 => d - 272 - leap,
        10 => d - 303 - leap,
        _ => d - 333 - leap,
    }) as f64
}

pub(crate) fn week_day(t: f64) -> f64 {
    (day(t) + 4.0).rem_euclid(7.0)
}

pub(crate) fn hour_from_time(t: f64) -> f64 {
    (time_within_day(t) / 3_600_000.0).floor().rem_euclid(24.0)
}

pub(crate) fn min_from_time(t: f64) -> f64 {
    (time_within_day(t) / 60_000.0).floor().rem_euclid(60.0)
}

pub(crate) fn sec_from_time(t: f64) -> f64 {
    (time_within_day(t) / 1000.0).floor().rem_euclid(60.0)
}

pub(crate) fn ms_from_time(t: f64) -> f64 {
    time_within_day(t).rem_euclid(1000.0)
}

pub(crate) fn make_time(hour: f64, min: f64, sec: f64, ms: f64) -> f64 {
    if !hour.is_finite() || !min.is_finite() || !sec.is_finite() || !ms.is_finite() {
        return f64::NAN;
    }
    let h = hour.trunc();
    let m = min.trunc();
    let s = sec.trunc();
    let milli = ms.trunc();
    h * 3_600_000.0 + m * 60_000.0 + s * 1000.0 + milli
}

pub(crate) fn make_day(year: f64, month: f64, date: f64) -> f64 {
    if !year.is_finite() || !month.is_finite() || !date.is_finite() {
        return f64::NAN;
    }
    let y = year.trunc();
    let m = month.trunc();
    let dt = date.trunc();
    let ym = y + (m / 12.0).floor();
    let mn = m.rem_euclid(12.0);

    let month_starts = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let day_count = day_from_year(ym);
    let leap = if days_in_year(ym) == 366.0 && mn >= 2.0 {
        1.0
    } else {
        0.0
    };
    day_count + month_starts[mn as usize] as f64 + leap + dt - 1.0
}

pub(crate) fn make_date(day: f64, time: f64) -> f64 {
    if !day.is_finite() || !time.is_finite() {
        return f64::NAN;
    }
    day * MS_PER_DAY + time
}

pub(crate) fn time_clip(time: f64) -> f64 {
    if !time.is_finite() || time.abs() > 8.64e15 {
        return f64::NAN;
    }
    let t = time.trunc();
    if t == 0.0 { 0.0_f64 } else { t }
}

pub(crate) fn local_tza() -> f64 {
    use chrono::Local;
    let now = Local::now();
    now.offset().local_minus_utc() as f64 * 1000.0
}

pub(crate) fn local_time(t: f64) -> f64 {
    t + local_tza()
}

pub(crate) fn utc_time(t: f64) -> f64 {
    t - local_tza()
}

pub(crate) fn now_ms() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as f64)
        .unwrap_or(f64::NAN)
}

pub(crate) fn day_name(wd: f64) -> &'static str {
    match wd as i32 {
        0 => "Sun",
        1 => "Mon",
        2 => "Tue",
        3 => "Wed",
        4 => "Thu",
        5 => "Fri",
        _ => "Sat",
    }
}

pub(crate) fn month_name(m: f64) -> &'static str {
    match m as i32 {
        0 => "Jan",
        1 => "Feb",
        2 => "Mar",
        3 => "Apr",
        4 => "May",
        5 => "Jun",
        6 => "Jul",
        7 => "Aug",
        8 => "Sep",
        9 => "Oct",
        10 => "Nov",
        _ => "Dec",
    }
}

pub(crate) fn format_date_string(t: f64) -> String {
    let lt = local_time(t);
    let wd = week_day(lt);
    let y = year_from_time(lt);
    let m = month_from_time(lt);
    let d = date_from_time(lt);
    let h = hour_from_time(lt);
    let min = min_from_time(lt);
    let s = sec_from_time(lt);

    let offset_ms = local_tza();
    let offset_min = (offset_ms / 60_000.0) as i32;
    let sign = if offset_min >= 0 { '+' } else { '-' };
    let abs_offset = offset_min.unsigned_abs();
    let oh = abs_offset / 60;
    let om = abs_offset % 60;

    let tz_abbr = chrono::Local::now().format("%Z").to_string();
    format!(
        "{} {} {:02} {:04} {:02}:{:02}:{:02} GMT{}{:02}{:02} ({})",
        day_name(wd),
        month_name(m),
        d as i32,
        y as i32,
        h as i32,
        min as i32,
        s as i32,
        sign,
        oh,
        om,
        tz_abbr
    )
}

pub(crate) fn format_iso_string(t: f64) -> String {
    let y = year_from_time(t);
    let m = month_from_time(t) as i32 + 1;
    let d = date_from_time(t) as i32;
    let h = hour_from_time(t) as i32;
    let min = min_from_time(t) as i32;
    let s = sec_from_time(t) as i32;
    let ms = ms_from_time(t) as i32;
    let yi = y as i64;
    if (0..=9999).contains(&yi) {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            yi, m, d, h, min, s, ms
        )
    } else if yi >= 0 {
        format!(
            "+{:06}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            yi, m, d, h, min, s, ms
        )
    } else {
        format!(
            "-{:06}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            yi.unsigned_abs(),
            m,
            d,
            h,
            min,
            s,
            ms
        )
    }
}

pub(crate) fn format_utc_string(t: f64) -> String {
    let wd = week_day(t);
    let y = year_from_time(t);
    let m = month_from_time(t);
    let d = date_from_time(t);
    let h = hour_from_time(t);
    let min = min_from_time(t);
    let s = sec_from_time(t);
    format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
        day_name(wd),
        d as i32,
        month_name(m),
        y as i32,
        h as i32,
        min as i32,
        s as i32
    )
}

pub(crate) fn format_date_only_string(t: f64) -> String {
    let lt = local_time(t);
    let wd = week_day(lt);
    let y = year_from_time(lt);
    let m = month_from_time(lt);
    let d = date_from_time(lt);
    format!(
        "{} {} {:02} {:04}",
        day_name(wd),
        month_name(m),
        d as i32,
        y as i32
    )
}

pub(crate) fn format_time_only_string(t: f64) -> String {
    let lt = local_time(t);
    let h = hour_from_time(lt);
    let min = min_from_time(lt);
    let s = sec_from_time(lt);

    let offset_ms = local_tza();
    let offset_min = (offset_ms / 60_000.0) as i32;
    let sign = if offset_min >= 0 { '+' } else { '-' };
    let abs_offset = offset_min.unsigned_abs();
    let oh = abs_offset / 60;
    let om = abs_offset % 60;

    let tz_abbr = chrono::Local::now().format("%Z").to_string();
    format!(
        "{:02}:{:02}:{:02} GMT{}{:02}{:02} ({})",
        h as i32, min as i32, s as i32, sign, oh, om, tz_abbr
    )
}

pub(crate) fn parse_date_string(s: &str) -> f64 {
    let s = s.trim();
    if s.is_empty() {
        return f64::NAN;
    }

    // Try ISO 8601 format
    if let Some(t) = parse_iso_date(s) {
        return t;
    }

    // Try toString() format: "Wed Jan 29 2026 12:34:56 GMT+0100 (CET)"
    if let Some(t) = parse_tostring_format(s) {
        return t;
    }

    f64::NAN
}

fn parse_iso_date(s: &str) -> Option<f64> {
    let bytes = s.as_bytes();
    let len = bytes.len();

    let (year, pos) = parse_iso_year(s)?;

    if pos >= len {
        // Year only
        let d = make_day(year as f64, 0.0, 1.0);
        return Some(time_clip(make_date(d, 0.0)));
    }

    if bytes[pos] != b'-' {
        return None;
    }
    let pos = pos + 1;
    let month: i32 = s.get(pos..pos + 2)?.parse().ok()?;
    if !(1..=12).contains(&month) {
        return None;
    }
    let pos = pos + 2;

    if pos >= len {
        let d = make_day(year as f64, (month - 1) as f64, 1.0);
        return Some(time_clip(make_date(d, 0.0)));
    }

    if bytes[pos] != b'-' {
        return None;
    }
    let pos = pos + 1;
    let day_val: i32 = s.get(pos..pos + 2)?.parse().ok()?;
    if !(1..=31).contains(&day_val) {
        return None;
    }
    let pos = pos + 2;

    if pos >= len {
        // Date only = UTC
        let d = make_day(year as f64, (month - 1) as f64, day_val as f64);
        return Some(time_clip(make_date(d, 0.0)));
    }

    if bytes[pos] != b'T' && bytes[pos] != b't' {
        return None;
    }
    let pos = pos + 1;

    let hour: i32 = s.get(pos..pos + 2)?.parse().ok()?;
    let pos = pos + 2;
    if pos >= len || bytes[pos] != b':' {
        return None;
    }
    let pos = pos + 1;
    let minute: i32 = s.get(pos..pos + 2)?.parse().ok()?;
    let pos = pos + 2;

    let (second, ms_val, pos) = if pos < len && bytes[pos] == b':' {
        let pos = pos + 1;
        let sec: i32 = s.get(pos..pos + 2)?.parse().ok()?;
        let pos = pos + 2;
        if pos < len && bytes[pos] == b'.' {
            let pos = pos + 1;
            let frac_start = pos;
            let mut frac_end = pos;
            while frac_end < len && bytes[frac_end].is_ascii_digit() {
                frac_end += 1;
            }
            let frac_str = s.get(frac_start..frac_end)?;
            let ms = match frac_str.len() {
                1 => frac_str.parse::<i32>().ok()? * 100,
                2 => frac_str.parse::<i32>().ok()? * 10,
                3 => frac_str.parse::<i32>().ok()?,
                n if n > 3 => frac_str[..3].parse::<i32>().ok()?,
                _ => 0,
            };
            (sec, ms, frac_end)
        } else {
            (sec, 0, pos)
        }
    } else {
        (0, 0, pos)
    };

    let d = make_day(year as f64, (month - 1) as f64, day_val as f64);
    let time = make_time(hour as f64, minute as f64, second as f64, ms_val as f64);
    let dt = make_date(d, time);

    // Timezone
    if pos >= len {
        // No timezone = local time
        return Some(time_clip(utc_time(dt)));
    }

    let ch = bytes[pos];
    if ch == b'Z' || ch == b'z' {
        return Some(time_clip(dt));
    }

    if ch == b'+' || ch == b'-' {
        let sign: f64 = if ch == b'+' { 1.0 } else { -1.0 };
        let pos = pos + 1;
        let tz_hour: f64 = s.get(pos..pos + 2)?.parse().ok()?;
        let pos = pos + 2;
        let tz_min: f64 = if pos < len && bytes[pos] == b':' {
            s.get(pos + 1..pos + 3)?.parse().ok()?
        } else if pos + 1 < len && bytes[pos].is_ascii_digit() {
            s.get(pos..pos + 2)?.parse().ok()?
        } else {
            0.0
        };
        let offset = sign * (tz_hour * 60.0 + tz_min) * 60_000.0;
        return Some(time_clip(dt - offset));
    }

    None
}

fn parse_iso_year(s: &str) -> Option<(i64, usize)> {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    if bytes[0] == b'+' || bytes[0] == b'-' {
        // Extended year ±YYYYYY
        let sign: i64 = if bytes[0] == b'+' { 1 } else { -1 };
        let yr: i64 = s.get(1..7)?.parse().ok()?;
        Some((sign * yr, 7))
    } else {
        let yr: i64 = s.get(0..4)?.parse().ok()?;
        Some((yr, 4))
    }
}

fn parse_tostring_format(s: &str) -> Option<f64> {
    // "Wed Jan 29 2026 12:34:56 GMT+0100 (CET)"
    // or "Wed Jan 29 2026 12:34:56 GMT+0100"
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 5 {
        return None;
    }

    let month_idx = match parts[1] {
        "Jan" => 0,
        "Feb" => 1,
        "Mar" => 2,
        "Apr" => 3,
        "May" => 4,
        "Jun" => 5,
        "Jul" => 6,
        "Aug" => 7,
        "Sep" => 8,
        "Oct" => 9,
        "Nov" => 10,
        "Dec" => 11,
        _ => return None,
    };

    let day_val: i32 = parts[2].parse().ok()?;
    let year: i64 = parts[3].parse().ok()?;
    let time_parts: Vec<&str> = parts[4].split(':').collect();
    if time_parts.len() != 3 {
        return None;
    }
    let hour: i32 = time_parts[0].parse().ok()?;
    let min: i32 = time_parts[1].parse().ok()?;
    let sec: i32 = time_parts[2].parse().ok()?;

    let d = make_day(year as f64, month_idx as f64, day_val as f64);
    let time = make_time(hour as f64, min as f64, sec as f64, 0.0);
    let dt = make_date(d, time);

    if parts.len() > 5 && parts[5].starts_with("GMT") {
        let tz = &parts[5][3..];
        if tz.is_empty() {
            return Some(time_clip(dt));
        }
        let sign: f64 = if tz.starts_with('+') { 1.0 } else { -1.0 };
        let tz = &tz[1..];
        if tz.len() >= 4 {
            let tz_h: f64 = tz[..2].parse().ok()?;
            let tz_m: f64 = tz[2..4].parse().ok()?;
            let offset = sign * (tz_h * 60.0 + tz_m) * 60_000.0;
            return Some(time_clip(dt - offset));
        }
    }

    // Assume local
    Some(time_clip(utc_time(dt)))
}

pub(crate) fn find_json_colon(s: &str) -> Option<usize> {
    let mut in_string = false;
    let mut escape = false;
    for (i, ch) in s.chars().enumerate() {
        if escape {
            escape = false;
            continue;
        }
        if ch == '\\' && in_string {
            escape = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if !in_string && ch == ':' {
            return Some(i);
        }
    }
    None
}

fn is_uri_unreserved(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '!' | '~' | '*' | '\'' | '(' | ')')
}

fn is_uri_reserved(c: char) -> bool {
    matches!(
        c,
        ';' | '/' | '?' | ':' | '@' | '&' | '=' | '+' | '$' | ',' | '#'
    )
}

fn percent_encode_byte(b: u8, out: &mut String) {
    use std::fmt::Write;
    write!(out, "%{:02X}", b).unwrap();
}

pub(crate) fn encode_uri_string(
    code_units: &[u16],
    preserve_reserved: bool,
) -> Result<String, String> {
    let mut result = String::new();
    let mut i = 0;
    while i < code_units.len() {
        let cu = code_units[i];
        if cu <= 0x7F {
            let c = cu as u8 as char;
            if is_uri_unreserved(c) || (preserve_reserved && is_uri_reserved(c)) {
                result.push(c);
            } else {
                let mut buf = [0u8; 4];
                let encoded = c.encode_utf8(&mut buf);
                for b in encoded.as_bytes() {
                    percent_encode_byte(*b, &mut result);
                }
            }
            i += 1;
        } else if (0xD800..=0xDBFF).contains(&cu) {
            if i + 1 >= code_units.len() || !(0xDC00..=0xDFFF).contains(&code_units[i + 1]) {
                return Err("URI malformed".to_string());
            }
            let hi = cu as u32;
            let lo = code_units[i + 1] as u32;
            let cp = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
            let c = char::from_u32(cp).ok_or_else(|| "URI malformed".to_string())?;
            let mut buf = [0u8; 4];
            let encoded = c.encode_utf8(&mut buf);
            for b in encoded.as_bytes() {
                percent_encode_byte(*b, &mut result);
            }
            i += 2;
        } else if (0xDC00..=0xDFFF).contains(&cu) {
            return Err("URI malformed".to_string());
        } else {
            let c = char::from_u32(cu as u32).ok_or_else(|| "URI malformed".to_string())?;
            let mut buf = [0u8; 4];
            let encoded = c.encode_utf8(&mut buf);
            for b in encoded.as_bytes() {
                percent_encode_byte(*b, &mut result);
            }
            i += 1;
        }
    }
    Ok(result)
}

pub(crate) fn decode_uri_string(
    code_units: &[u16],
    preserve_reserved: bool,
) -> Result<Vec<u16>, String> {
    let mut result: Vec<u16> = Vec::new();
    let len = code_units.len();
    let mut i = 0;

    while i < len {
        let cu = code_units[i];
        if cu != 0x25 {
            // '%'
            result.push(cu);
            i += 1;
            continue;
        }

        if i + 2 >= len {
            return Err("URI malformed".to_string());
        }
        let h1 = cu16_to_hex_val(code_units[i + 1])?;
        let l1 = cu16_to_hex_val(code_units[i + 2])?;
        let first_byte = (h1 << 4) | l1;
        i += 3;

        if first_byte <= 0x7F {
            let c = first_byte as u8 as char;
            if preserve_reserved && is_uri_reserved(c) {
                result.push(0x25); // '%'
                result.push(code_units[i - 2]);
                result.push(code_units[i - 1]);
            } else {
                result.push(first_byte as u16);
            }
            continue;
        }

        let expected_len = if first_byte & 0xE0 == 0xC0 {
            2
        } else if first_byte & 0xF0 == 0xE0 {
            3
        } else if first_byte & 0xF8 == 0xF0 {
            4
        } else {
            return Err("URI malformed".to_string());
        };

        let mut utf8_bytes = vec![first_byte as u8];
        let start_i = i - 3;
        for _ in 1..expected_len {
            if i >= len || code_units[i] != 0x25 || i + 2 >= len {
                return Err("URI malformed".to_string());
            }
            let hh = cu16_to_hex_val(code_units[i + 1])?;
            let ll = cu16_to_hex_val(code_units[i + 2])?;
            let cont = (hh << 4) | ll;
            if cont & 0xC0 != 0x80 {
                return Err("URI malformed".to_string());
            }
            utf8_bytes.push(cont as u8);
            i += 3;
        }

        let s = std::str::from_utf8(&utf8_bytes).map_err(|_| "URI malformed".to_string())?;
        let c = s
            .chars()
            .next()
            .ok_or_else(|| "URI malformed".to_string())?;

        let cp = c as u32;
        if (0xD800..=0xDFFF).contains(&cp) {
            return Err("URI malformed".to_string());
        }

        let min_cp: u32 = match expected_len {
            2 => 0x80,
            3 => 0x800,
            4 => 0x10000,
            _ => unreachable!(),
        };
        if cp < min_cp {
            return Err("URI malformed".to_string());
        }

        if preserve_reserved && is_uri_reserved(c) {
            // Keep original percent-encoded form
            for idx in start_i..i {
                result.push(code_units[idx]);
            }
        } else {
            // Encode char as UTF-16 code units
            let mut buf = [0u16; 2];
            let encoded = c.encode_utf16(&mut buf);
            for unit in encoded.iter() {
                result.push(*unit);
            }
        }
    }

    Ok(result)
}

fn cu16_to_hex_val(cu: u16) -> Result<u8, String> {
    if cu > 0x7F {
        return Err("URI malformed".to_string());
    }
    hex_val(cu as u8)
}

fn parse_hex_byte(h: u8, l: u8) -> Result<u8, String> {
    let hi = hex_val(h)?;
    let lo = hex_val(l)?;
    Ok((hi << 4) | lo)
}

fn hex_val(b: u8) -> Result<u8, String> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err("URI malformed".to_string()),
    }
}
