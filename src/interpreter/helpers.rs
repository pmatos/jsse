use super::*;

// Resolves a spec "relative index" argument (already passed through ToIntegerOrInfinity)
// against a length, per the clamp used by e.g. Array.prototype.slice, String.prototype.slice,
// and the %TypedArray%.prototype equivalents:
//   if n < 0, max(len + n, 0); else min(n, len)
pub(crate) fn resolve_relative_index(n: f64, len: usize) -> usize {
    let len = len as f64;
    if n < 0.0 {
        (len + n).max(0.0) as usize
    } else {
        n.min(len) as usize
    }
}

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
        JsValue::BigInt(b) => b.value != num_bigint::BigInt::from(0),
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

// §12 WhiteSpace | LineTerminator — the set trimmed from a StrNumericLiteral and
// by String.prototype.trim / parseInt / parseFloat. Kept here, in the spec
// conversions module, as the single canonical predicate every consumer shares.
// (Distinct from Rust's `char::is_whitespace`, which adds U+0085 and omits U+FEFF.)
pub(crate) fn is_ecma_whitespace(ch: char) -> bool {
    matches!(
        ch,
        '\u{0009}'
            | '\u{000A}'
            | '\u{000B}'
            | '\u{000C}'
            | '\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{FEFF}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200A}' | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}'
    )
}

// Parse the digits of a §7.1.4.1 NonDecimalIntegerLiteral (0x / 0o / 0b) as an
// exact integer, then convert it to f64 once. Parsing through the exact decimal
// representation gives Rust's float parser the full mathematical value to round,
// avoiding both fixed-width overflow and intermediate floating-point rounding.
// Empty or invalid input → NaN.
fn radix_digits_to_f64(digits: &str, radix: u32) -> f64 {
    if digits.is_empty() || !digits.chars().all(|ch| ch.is_digit(radix)) {
        return f64::NAN;
    }
    num_bigint::BigUint::parse_bytes(digits.as_bytes(), radix)
        .and_then(|exact| exact.to_string().parse::<f64>().ok())
        .unwrap_or(f64::NAN)
}

// §7.1.4.1.1 StringToNumber (uses §7.1.4.1.2 RoundMVResult via f64::parse)
fn string_to_number(s: &JsString) -> f64 {
    let rust_str = s.to_rust_string();
    let trimmed = rust_str.trim_matches(is_ecma_whitespace);
    if trimmed.is_empty() {
        return 0.0;
    }
    // NonDecimalIntegerLiteral: no sign is permitted, so a leading '+'/'-' keeps
    // the string out of these branches and it falls through to StrDecimalLiteral.
    if let Some(rest) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        return radix_digits_to_f64(rest, 16);
    }
    if let Some(rest) = trimmed
        .strip_prefix("0o")
        .or_else(|| trimmed.strip_prefix("0O"))
    {
        return radix_digits_to_f64(rest, 8);
    }
    if let Some(rest) = trimmed
        .strip_prefix("0b")
        .or_else(|| trimmed.strip_prefix("0B"))
    {
        return radix_digits_to_f64(rest, 2);
    }
    // StrDecimalLiteral: the only Infinity token is exactly "Infinity" (with an
    // optional sign). Every other inf/nan spelling Rust's f64 parser would accept
    // must be NaN, so reject them before handing off to `parse`.
    match trimmed {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    let unsigned = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);
    if unsigned.eq_ignore_ascii_case("inf")
        || unsigned.eq_ignore_ascii_case("infinity")
        || unsigned.eq_ignore_ascii_case("nan")
    {
        return f64::NAN;
    }
    trimmed.parse::<f64>().unwrap_or(f64::NAN)
}

pub(crate) fn to_js_string(val: &JsValue) -> String {
    match val {
        JsValue::BigInt(b) => b.value.to_string(),
        _ => format!("{val}"),
    }
}

/// Convert a JsValue to UTF-16 code units, preserving lone surrogates for strings.
pub(crate) fn js_value_to_code_units(val: &JsValue) -> Vec<u16> {
    match val {
        JsValue::String(s) => s.code_units.to_vec(),
        _ => to_js_string(val).encode_utf16().collect(),
    }
}

/// Convert a JsValue to a property key string. For symbols, uses the id-based
/// format to ensure uniqueness. For other types, same as to_js_string.
pub(crate) fn to_property_key_string(val: &JsValue) -> JsPropertyKey {
    match val {
        JsValue::String(s) => JsPropertyKey::from_js_string(s),
        JsValue::Symbol(s) => s.to_property_key(),
        _ => JsPropertyKey::from(format!("{val}")),
    }
}

pub(crate) fn is_string(val: &JsValue) -> bool {
    matches!(val, JsValue::String(_))
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
        (JsValue::BigInt(a), JsValue::BigInt(b)) => bigint_ops::equal(&a.value, &b.value),
        (JsValue::Object(a), JsValue::Object(b)) => a.id == b.id,
        _ => false,
    }
}

pub(crate) fn typeof_val<'a>(val: &JsValue, objects: &super::object_arena::ObjectArena) -> &'a str {
    match val {
        JsValue::Undefined => "undefined",
        JsValue::Null => "object",
        JsValue::Boolean(_) => "boolean",
        JsValue::Number(_) => "number",
        JsValue::String(_) => "string",
        JsValue::Symbol(_) => "symbol",
        JsValue::BigInt(_) => "bigint",
        JsValue::Object(o) => {
            if let Some(obj) = objects.get(o.id) {
                let b = obj.borrow();
                if b.is_htmldda {
                    return "undefined";
                }
                if b.callable.is_some() {
                    return "function";
                }
            }
            "object"
        }
    }
}

use std::collections::HashMap;

fn json_quote_units(units: &[u16]) -> String {
    let mut result = String::with_capacity(units.len() + 2);
    result.push('"');
    let mut i = 0;
    while i < units.len() {
        let cu = units[i];
        match cu {
            0x0022 => result.push_str("\\\""),
            0x005C => result.push_str("\\\\"),
            0x0008 => result.push_str("\\b"),
            0x000C => result.push_str("\\f"),
            0x000A => result.push_str("\\n"),
            0x000D => result.push_str("\\r"),
            0x0009 => result.push_str("\\t"),
            c if c < 0x0020 => {
                result.push_str(&format!("\\u{:04x}", c));
            }
            c if (0xD800..=0xDBFF).contains(&c) => {
                if i + 1 < units.len() && (0xDC00..=0xDFFF).contains(&units[i + 1]) {
                    let hi = c as u32;
                    let lo = units[i + 1] as u32;
                    let cp = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
                    if let Some(ch) = char::from_u32(cp) {
                        result.push(ch);
                    }
                    i += 1;
                } else {
                    result.push_str(&format!("\\u{:04x}", c));
                }
            }
            c if (0xDC00..=0xDFFF).contains(&c) => {
                result.push_str(&format!("\\u{:04x}", c));
            }
            c => {
                if let Some(ch) = char::from_u32(c as u32) {
                    result.push(ch);
                }
            }
        }
        i += 1;
    }
    result.push('"');
    result
}

fn json_quote_js_string(s: &JsString) -> String {
    json_quote_units(&s.code_units)
}

// Proxy-aware IsArray (§7.2.2)
pub(crate) fn is_array_value(interp: &mut Interpreter, obj_id: u64) -> Result<bool, JsValue> {
    let snapshot = interp.get_object_cell(obj_id).map(|cell| {
        let b = cell.borrow();
        let tid = if b.is_proxy() {
            b.proxy_target_id()
        } else {
            None
        };
        (
            b.is_proxy_revoked(),
            b.is_proxy(),
            tid,
            b.class_name.clone(),
        )
    });
    if let Some((is_revoked, is_proxy, target_id, class)) = snapshot {
        if is_revoked {
            return Err(interp.create_error(
                "TypeError",
                "Cannot perform 'IsArray' on a proxy that has been revoked",
            ));
        }
        if is_proxy {
            if let Some(tid) = target_id {
                return is_array_value(interp, tid);
            }
            return Ok(false);
        }
        return Ok(class == "Array");
    }
    Ok(false)
}

pub(crate) fn sort_own_keys(keys: Vec<JsPropertyKey>) -> Vec<JsPropertyKey> {
    let mut indices: Vec<(u64, usize)> = Vec::new();
    let mut strings: Vec<(JsPropertyKey, usize)> = Vec::new();
    for (pos, k) in keys.iter().enumerate() {
        if let Ok(n) = k.parse::<u64>()
            && k.eq_str(&n.to_string())
        {
            indices.push((n, pos));
            continue;
        }
        strings.push((k.clone(), pos));
    }
    indices.sort_by_key(|&(n, _)| n);
    let mut result: Vec<JsPropertyKey> = Vec::with_capacity(keys.len());
    for (n, _) in indices {
        result.push(JsPropertyKey::from(n.to_string()));
    }
    for (s, _) in strings {
        result.push(s);
    }
    result
}

pub(crate) fn enumerable_own_keys(
    interp: &mut Interpreter,
    obj_id: u64,
) -> Result<Vec<JsPropertyKey>, JsValue> {
    if let Some(obj) = interp.get_object_cell(obj_id) {
        if obj.borrow().is_proxy() || obj.borrow().is_proxy_revoked() {
            let target_val = interp.get_proxy_target_val(obj_id);
            match interp.invoke_proxy_trap(obj_id, "ownKeys", vec![target_val.clone()]) {
                Ok(Some(v)) => {
                    interp.validate_ownkeys_invariant(&v, &target_val)?;
                    let mut keys = Vec::new();
                    if let JsValue::Object(arr) = &v {
                        const MAX_PROXY_OWNKEYS_RESULT_LEN: usize = 1_000_000;
                        let len = match interp.get_property_on_id(arr.id, "length") {
                            JsValue::Number(n) if n.is_finite() && n > 0.0 => {
                                let len = n.floor() as usize;
                                if len > MAX_PROXY_OWNKEYS_RESULT_LEN {
                                    return Err(interp.create_type_error(
                                        "'ownKeys' on proxy: trap result length exceeds supported limit",
                                    ));
                                }
                                len
                            }
                            _ => 0,
                        };
                        for i in 0..len {
                            let k = interp.get_property_on_id(arr.id, &i.to_string());
                            if let JsValue::String(s) = k {
                                let key = JsPropertyKey::from_js_string(&s);
                                let key_val = JsValue::String(s);
                                match interp.invoke_proxy_trap(
                                    obj_id,
                                    "getOwnPropertyDescriptor",
                                    vec![target_val.clone(), key_val],
                                ) {
                                    Ok(Some(desc_val)) => {
                                        if let JsValue::Object(dobj) = &desc_val {
                                            let enum_val =
                                                interp.get_property_on_id(dobj.id, "enumerable");
                                            if interp.to_boolean_val(&enum_val) {
                                                keys.push(key.clone());
                                            }
                                        }
                                    }
                                    Ok(None) => {
                                        if let JsValue::Object(ref t) = target_val
                                            && let Some(tobj) = interp.get_object_cell(t.id)
                                            && let Some(d) = tobj.borrow().properties.get(&key)
                                            && d.enumerable != Some(false)
                                        {
                                            keys.push(key);
                                        }
                                    }
                                    Err(e) => return Err(e),
                                }
                            }
                        }
                    }
                    return Ok(keys);
                }
                Ok(None) => {
                    // No ownKeys trap — delegate to the target
                    if let JsValue::Object(ref t) = target_val {
                        return enumerable_own_keys(interp, t.id);
                    }
                    return Ok(Vec::new());
                }
                Err(e) => return Err(e),
            }
        }
        let b = obj.borrow();
        // String exotic object: character indices come first
        let mut result: Vec<JsPropertyKey> = Vec::new();
        if let Some(JsValue::String(ref s)) = b.primitive_value {
            let len = s.len();
            for i in 0..len {
                result.push(JsPropertyKey::from(i.to_string()));
            }
        }
        // TypedArray [[OwnPropertyKeys]]: virtual indexed properties are enumerable
        if let Some(ta) = b.typed_array_info() {
            for i in 0..ta.array_length {
                result.push(JsPropertyKey::from(i.to_string()));
            }
        }
        let is_string_wrapper = matches!(b.primitive_value, Some(JsValue::String(_)));
        let keys: Vec<JsPropertyKey> = b
            .property_order
            .iter()
            .filter(|k| {
                if result.contains(k) {
                    return false;
                }
                if is_string_wrapper && k.eq_str("length") {
                    return false;
                }
                !k.is_symbol()
                    && b.properties
                        .get(k)
                        .is_some_and(|d| d.enumerable != Some(false))
            })
            .cloned()
            .collect();
        if !result.is_empty() {
            result.extend(sort_own_keys(keys));
            return Ok(result);
        }
        return Ok(sort_own_keys(keys));
    }
    Ok(Vec::new())
}

pub(crate) fn json_stringify_full(
    interp: &mut Interpreter,
    val: &JsValue,
    replacer: &Option<JsValue>,
    space: &str,
) -> Result<Option<String>, JsValue> {
    let mut stack = Vec::new();
    let mut property_list: Option<Vec<JsPropertyKey>> = None;
    let mut replacer_fn: Option<JsValue> = None;

    if let Some(rep) = replacer
        && let JsValue::Object(o) = rep
        && let Some(obj) = interp.get_object_cell(o.id)
    {
        if obj.borrow().callable.is_some() {
            replacer_fn = Some(rep.clone());
        } else if is_array_value(interp, o.id)? {
            let mut keys = Vec::new();
            let obj_val = JsValue::Object(crate::types::JsObject { id: o.id });
            let len_val = match interp.get_object_property(o.id, "length", &obj_val) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::Undefined,
            };
            let len = {
                let n = interp.to_number_value(&len_val)?;
                n as usize
            };
            for i in 0..len {
                let item = match interp.get_object_property(o.id, &i.to_string(), &obj_val) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::Undefined,
                };
                let key_str = match &item {
                    JsValue::String(s) => Some(JsPropertyKey::from_js_string(s)),
                    JsValue::Number(n) => Some(JsPropertyKey::from(number_ops::to_string(*n))),
                    JsValue::Object(oo) => {
                        if let Some(inner) = interp.get_object_cell(oo.id) {
                            let cn = inner.borrow().class_name.clone();
                            if cn == "String" || cn == "Number" {
                                {
                                    let s = interp.to_string_value(&item)?;
                                    Some(JsPropertyKey::from(s))
                                }
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
                    && !keys.contains(&k)
                {
                    keys.push(k);
                }
            }
            property_list = Some(keys);
        }
    }

    let wrapper_id = interp.create_object_id();
    interp
        .get_object_cell_expect(wrapper_id)
        .borrow_mut()
        .insert_value("".to_string(), val.clone());
    let holder_id = wrapper_id;

    let root_key = JsPropertyKey::from_str("");
    json_stringify_internal(
        interp,
        holder_id,
        &root_key,
        val,
        &mut stack,
        &replacer_fn,
        &property_list,
        space,
        "",
    )
}

#[allow(clippy::too_many_arguments)]
fn json_stringify_internal(
    interp: &mut Interpreter,
    holder_id: u64,
    key: &JsPropertyKey,
    val: &JsValue,
    stack: &mut Vec<u64>,
    replacer_fn: &Option<JsValue>,
    property_list: &Option<Vec<JsPropertyKey>>,
    gap: &str,
    indent: &str,
) -> Result<Option<String>, JsValue> {
    let mut value = val.clone();

    // Step 2: If Type(value) is Object or BigInt, check for toJSON
    let check_tojson = matches!(&value, JsValue::Object(_) | JsValue::BigInt(_));
    if check_tojson {
        let to_json = if let JsValue::Object(o) = &value {
            match interp.get_object_property(o.id, "toJSON", &value) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Err(e),
                _ => JsValue::Undefined,
            }
        } else if let JsValue::BigInt(_) = &value {
            let obj_val = match interp.to_object(&value) {
                Completion::Normal(v) => v,
                _ => JsValue::Undefined,
            };
            if let JsValue::Object(o) = &obj_val {
                // Use original BigInt value as receiver for proper getter behavior
                match interp.get_object_property(o.id, "toJSON", &value) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Err(e),
                    _ => JsValue::Undefined,
                }
            } else {
                JsValue::Undefined
            }
        } else {
            JsValue::Undefined
        };
        if let JsValue::Object(fobj) = &to_json
            && let Some(fdata) = interp.get_object_cell(fobj.id)
            && fdata.borrow().callable.is_some()
        {
            let key_val = JsValue::String(key.to_js_string());
            match interp.call_function(&to_json, &value, &[key_val]) {
                Completion::Normal(v) => value = v,
                Completion::Throw(e) => return Err(e),
                _ => {}
            }
        }
    }

    // Step 3: Apply replacer function
    if let Some(rep) = replacer_fn {
        let holder_val = JsValue::Object(crate::types::JsObject { id: holder_id });
        let key_val = JsValue::String(key.to_js_string());
        match interp.call_function(rep, &holder_val, &[key_val, value.clone()]) {
            Completion::Normal(v) => value = v,
            Completion::Throw(e) => return Err(e),
            _ => {}
        }
    }

    // Step 4: Unwrap wrapper objects per spec (ToNumber/ToString trigger valueOf/toString)
    if let JsValue::Object(o) = &value {
        let class = if let Some(cell) = interp.get_object_cell(o.id) {
            cell.borrow().class_name.clone()
        } else {
            String::new()
        };
        match class.as_str() {
            "Number" => {
                let n = interp.to_number_value(&value)?;
                value = JsValue::Number(n)
            }
            "String" => {
                let s = interp.to_string_value(&value)?;
                value = JsValue::String(JsString::from_str(&s))
            }
            "Boolean" => {
                if let Some(cell) = interp.get_object_cell(o.id)
                    && let Some(pv) = cell.borrow().primitive_value.clone()
                {
                    value = pv;
                }
            }
            "BigInt" => {
                if let Some(cell) = interp.get_object_cell(o.id)
                    && let Some(pv) = cell.borrow().primitive_value.clone()
                {
                    value = pv;
                }
            }
            _ => {}
        }
    }

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
        JsValue::String(s) => Ok(Some(json_quote_js_string(s))),
        JsValue::BigInt(_) => {
            Err(interp.create_error("TypeError", "Do not know how to serialize a BigInt"))
        }
        JsValue::Object(o) => {
            if let Some(obj) = interp.get_object_cell(o.id) {
                if obj.borrow().is_raw_json
                    && let Some(raw) = obj.borrow().get_property_value("rawJSON")
                {
                    return Ok(Some(to_js_string(&raw)));
                }
                if obj.borrow().callable.is_some() {
                    return Ok(None);
                }
                let obj_id = obj.borrow().id.unwrap();
                if stack.contains(&obj_id) {
                    return Err(
                        interp.create_error("TypeError", "Converting circular structure to JSON")
                    );
                }
                stack.push(obj_id);

                let is_array = is_array_value(interp, obj_id)?;
                let obj_val = JsValue::Object(crate::types::JsObject { id: obj_id });
                let new_indent = format!("{}{}", indent, gap);

                let result = if is_array {
                    let len_val = match interp.get_object_property(obj_id, "length", &obj_val) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => {
                            stack.pop();
                            return Err(e);
                        }
                        _ => JsValue::Undefined,
                    };
                    let len = match interp.to_number_value(&len_val) {
                        Ok(n) => n as usize,
                        Err(e) => {
                            stack.pop();
                            return Err(e);
                        }
                    };
                    let mut items = Vec::new();
                    for i in 0..len {
                        let ikey = JsPropertyKey::from(i.to_string());
                        let v = match interp.get_object_property(obj_id, &ikey, &obj_val) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                stack.pop();
                                return Err(e);
                            }
                            _ => JsValue::Undefined,
                        };
                        match json_stringify_internal(
                            interp,
                            obj_id,
                            &ikey,
                            &v,
                            stack,
                            replacer_fn,
                            property_list,
                            gap,
                            &new_indent,
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
                        Ok(Some(format!(
                            "[\n{}{}\n{}]",
                            new_indent,
                            items.join(&sep),
                            indent
                        )))
                    }
                } else {
                    let keys: Vec<JsPropertyKey> = if let Some(pl) = property_list {
                        pl.clone()
                    } else {
                        match enumerable_own_keys(interp, obj_id) {
                            Ok(k) => k,
                            Err(e) => {
                                stack.pop();
                                return Err(e);
                            }
                        }
                    };
                    let mut entries = Vec::new();
                    for k in &keys {
                        let v = match interp.get_object_property(obj_id, k, &obj_val) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => {
                                stack.pop();
                                return Err(e);
                            }
                            _ => JsValue::Undefined,
                        };
                        if let Some(sv) = json_stringify_internal(
                            interp,
                            obj_id,
                            k,
                            &v,
                            stack,
                            replacer_fn,
                            property_list,
                            gap,
                            &new_indent,
                        )? {
                            let quoted_key = json_quote_js_string(&k.to_js_string());
                            if gap.is_empty() {
                                entries.push(format!("{}:{}", quoted_key, sv));
                            } else {
                                entries.push(format!("{}: {}", quoted_key, sv));
                            }
                        }
                    }
                    if entries.is_empty() {
                        Ok(Some("{}".to_string()))
                    } else if gap.is_empty() {
                        Ok(Some(format!("{{{}}}", entries.join(","))))
                    } else {
                        let sep = format!(",\n{}", new_indent);
                        Ok(Some(format!(
                            "{{\n{}{}\n{}}}",
                            new_indent,
                            entries.join(&sep),
                            indent
                        )))
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

pub(crate) fn json_trim(s: &str) -> &str {
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

pub(crate) type SourceTextMap = HashMap<(u64, JsPropertyKey), String>;

pub(crate) fn json_parse_value(interp: &mut Interpreter, s: &str) -> Completion {
    json_parse_value_inner(interp, s, None, s)
}

pub(crate) fn json_parse_value_with_source(
    interp: &mut Interpreter,
    s: &str,
) -> (Completion, SourceTextMap) {
    let mut source_map = SourceTextMap::default();
    let result = json_parse_value_inner(interp, s, Some(&mut source_map), s);
    (result, source_map)
}

fn json_parse_value_inner(
    interp: &mut Interpreter,
    s: &str,
    mut source_map: Option<&mut SourceTextMap>,
    root_source: &str,
) -> Completion {
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
        return Completion::Normal(JsValue::String(json_unescape_js_string(inner)));
    }
    if json_is_valid_number(s)
        && let Ok(n) = s.parse::<f64>()
    {
        return Completion::Normal(JsValue::Number(n));
    }
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1];
        if let Some(token) = json_invalid_comma_token(inner, ']') {
            return json_unexpected_token_error(interp, token, root_source);
        }
        let items = json_split_items(inner);
        let mut parsed_items: Vec<(JsValue, String)> = Vec::new();
        for item in &items {
            let trimmed_src = json_trim(item).to_string();
            match json_parse_value_inner(interp, item, source_map.as_deref_mut(), root_source) {
                Completion::Normal(v) => parsed_items.push((v, trimmed_src)),
                other => return other,
            }
        }
        let vals: Vec<JsValue> = parsed_items.iter().map(|(v, _)| v.clone()).collect();
        let arr_val = interp.create_array(vals);
        if let JsValue::Object(ref arr_obj) = arr_val
            && let Some(ref mut smap) = source_map
        {
            let arr_id = arr_obj.id;
            for (i, (v, src)) in parsed_items.iter().enumerate() {
                if is_json_primitive(v) {
                    smap.insert((arr_id, JsPropertyKey::from(i.to_string())), src.clone());
                }
            }
        }
        return Completion::Normal(arr_val);
    }
    if s.starts_with('{') && s.ends_with('}') {
        let inner = &s[1..s.len() - 1];
        if let Some(token) = json_invalid_comma_token(inner, '}') {
            return json_unexpected_token_error(interp, token, root_source);
        }
        let pairs = json_split_items(inner);
        let obj_id = interp.create_object_id();
        for pair in &pairs {
            let pair = pair.trim();
            if pair.is_empty() {
                continue;
            }
            let colon_pos = match find_json_colon(pair) {
                Some(pos) => pos,
                None => {
                    let err = interp.create_error("SyntaxError", "Unexpected token in JSON");
                    return Completion::Throw(err);
                }
            };
            let key_str = pair[..colon_pos].trim();
            let val_str = pair[colon_pos + 1..].trim();
            let key = if key_str.starts_with('"') && key_str.ends_with('"') && key_str.len() >= 2 {
                let inner = &key_str[1..key_str.len() - 1];
                if json_validate_string(inner).is_err() {
                    let err = interp.create_error("SyntaxError", "Unexpected token in JSON");
                    return Completion::Throw(err);
                }
                JsPropertyKey::from_js_string(&json_unescape_js_string(inner))
            } else {
                let err = interp.create_error("SyntaxError", "Unexpected token in JSON");
                return Completion::Throw(err);
            };
            let val_src = json_trim(val_str).to_string();
            match json_parse_value_inner(interp, val_str, source_map.as_deref_mut(), root_source) {
                Completion::Normal(v) => {
                    if let Some(ref mut smap) = source_map
                        && is_json_primitive(&v)
                    {
                        smap.insert((obj_id, key.clone()), val_src);
                    }
                    interp
                        .get_object_cell_expect(obj_id)
                        .borrow_mut()
                        .insert_value(key, v);
                }
                other => return other,
            }
        }
        return Completion::Normal(JsValue::Object(crate::types::JsObject { id: obj_id }));
    }
    if let Some(token) = s.chars().next() {
        return json_unexpected_token_error(interp, token, root_source);
    }
    let err = interp.create_error("SyntaxError", "Unexpected token in JSON");
    Completion::Throw(err)
}

fn json_unexpected_token_error(interp: &mut Interpreter, token: char, source: &str) -> Completion {
    const MAX_SOURCE_CHARS: usize = 20;
    const TRUNCATED_SOURCE_CHARS: usize = 10;

    let (source, ellipsis) = if source.chars().nth(MAX_SOURCE_CHARS).is_some() {
        let prefix_end = source
            .char_indices()
            .nth(TRUNCATED_SOURCE_CHARS)
            .map_or(source.len(), |(index, _)| index);
        (&source[..prefix_end], "...")
    } else {
        (source, "")
    };
    let message = format!(
        "Unexpected token '{}', \"{}\"{} is not valid JSON",
        token, source, ellipsis
    );
    Completion::Throw(interp.create_error("SyntaxError", &message))
}

pub(crate) fn is_json_primitive(val: &JsValue) -> bool {
    matches!(
        val,
        JsValue::Null | JsValue::Boolean(_) | JsValue::Number(_) | JsValue::String(_)
    )
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

fn json_unescape_js_string(s: &str) -> JsString {
    let mut result = Vec::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('"') => result.push(b'"' as u16),
                Some('\\') => result.push(b'\\' as u16),
                Some('/') => result.push(b'/' as u16),
                Some('b') => result.push(0x0008),
                Some('f') => result.push(0x000C),
                Some('n') => result.push(b'\n' as u16),
                Some('r') => result.push(b'\r' as u16),
                Some('t') => result.push(b'\t' as u16),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Ok(code) = u16::from_str_radix(&hex, 16) {
                        result.push(code);
                    }
                }
                Some(c) => {
                    result.push(b'\\' as u16);
                    result.extend(c.encode_utf16(&mut [0; 2]).iter().copied());
                }
                None => result.push(b'\\' as u16),
            }
        } else {
            result.extend(ch.encode_utf16(&mut [0; 2]).iter().copied());
        }
    }
    JsString::from_vec(result)
}

fn json_internalize_apply<K: PropertyKeyLike + ?Sized>(
    interp: &mut Interpreter,
    obj_id: u64,
    key: &K,
    new_val: JsValue,
) -> Result<(), JsValue> {
    let is_proxy = interp
        .get_object_cell(obj_id)
        .map(|c| c.borrow().is_proxy() || c.borrow().is_proxy_revoked())
        .unwrap_or(false);

    if is_proxy {
        let target_val = interp.get_proxy_target_val(obj_id);
        if let JsValue::Undefined = &new_val {
            // Delete via proxy deleteProperty trap
            let key_val = JsValue::String(key.to_js_property_key().to_js_string());
            match interp.invoke_proxy_trap(obj_id, "deleteProperty", vec![target_val, key_val]) {
                Ok(Some(v)) => {
                    if !interp.to_boolean_val(&v) {
                        return Err(interp.create_type_error(
                            "'deleteProperty' on proxy: trap returned falsish",
                        ));
                    }
                }
                Ok(None) => {
                    // No trap, delete on target directly
                    if let JsValue::Object(t) = &interp.get_proxy_target_val(obj_id)
                        && let Some(tobj) = interp.get_object_cell(t.id)
                    {
                        tobj.borrow_mut().remove_property(key);
                    }
                }
                Err(e) => return Err(e),
            }
        } else {
            // CreateDataProperty via proxy defineProperty trap
            let key_val = JsValue::String(key.to_js_property_key().to_js_string());
            let desc_obj_id = interp.create_object_id();
            interp
                .get_object_cell_expect(desc_obj_id)
                .borrow_mut()
                .insert_value("value".to_string(), new_val.clone());
            interp
                .get_object_cell_expect(desc_obj_id)
                .borrow_mut()
                .insert_value("writable".to_string(), JsValue::Boolean(true));
            interp
                .get_object_cell_expect(desc_obj_id)
                .borrow_mut()
                .insert_value("enumerable".to_string(), JsValue::Boolean(true));
            interp
                .get_object_cell_expect(desc_obj_id)
                .borrow_mut()
                .insert_value("configurable".to_string(), JsValue::Boolean(true));
            let desc_val = JsValue::Object(crate::types::JsObject { id: desc_obj_id });
            match interp.invoke_proxy_trap(
                obj_id,
                "defineProperty",
                vec![target_val, key_val, desc_val],
            ) {
                Ok(Some(v)) => {
                    if !interp.to_boolean_val(&v) {
                        return Err(interp.create_type_error(
                            "'defineProperty' on proxy: trap returned falsish",
                        ));
                    }
                }
                Ok(None) => {
                    // No trap, define on target directly
                    if let JsValue::Object(t) = &interp.get_proxy_target_val(obj_id)
                        && let Some(tobj) = interp.get_object_cell(t.id)
                    {
                        tobj.borrow_mut()
                            .insert_value(key.to_js_property_key(), new_val);
                    }
                }
                Err(e) => return Err(e),
            }
        }
        return Ok(());
    }

    // Non-proxy path
    if let Some(cell) = interp.get_object_cell(obj_id) {
        let configurable = cell
            .borrow()
            .properties
            .get(key)
            .and_then(|d| d.configurable)
            .unwrap_or(true);
        if !configurable {
            return Ok(());
        }
        if let JsValue::Undefined = &new_val {
            cell.borrow_mut().remove_property(key);
            // Also clear dense array storage so get_property doesn't find stale values
            if let Some(key_str) = key.as_property_key_str()
                && let Ok(idx) = key_str.parse::<usize>()
            {
                let mut b = cell.borrow_mut();
                if let Some(elems) = b.array_elements_mut()
                    && idx < elems.len()
                {
                    elems[idx] = JsValue::Undefined;
                }
            }
        } else {
            cell.borrow_mut()
                .insert_value(key.to_js_property_key(), new_val);
        }
    }
    Ok(())
}

pub(crate) fn json_internalize<K: PropertyKeyLike + ?Sized>(
    interp: &mut Interpreter,
    holder: &JsValue,
    name: &K,
    reviver: &JsValue,
    source_map: &Option<SourceTextMap>,
) -> Completion {
    let val = if let JsValue::Object(o) = holder {
        match interp.get_object_property(o.id, name, holder) {
            Completion::Normal(v) => v,
            Completion::Throw(e) => return Completion::Throw(e),
            _ => JsValue::Undefined,
        }
    } else {
        JsValue::Undefined
    };

    let walked = if let JsValue::Object(o) = &val {
        let obj_id = o.id;
        let is_array = match is_array_value(interp, obj_id) {
            Ok(v) => v,
            Err(e) => return Completion::Throw(e),
        };
        if is_array {
            let len_val = match interp.get_object_property(obj_id, "length", &val) {
                Completion::Normal(v) => v,
                Completion::Throw(e) => return Completion::Throw(e),
                _ => JsValue::Undefined,
            };
            let len = match interp.to_number_value(&len_val) {
                Ok(n) => n as usize,
                Err(e) => return Completion::Throw(e),
            };
            for i in 0..len {
                let key = i.to_string();
                match json_internalize(interp, &val, &key, reviver, source_map) {
                    Completion::Normal(new_val) => {
                        if let Err(e) = json_internalize_apply(interp, obj_id, &key, new_val) {
                            return Completion::Throw(e);
                        }
                    }
                    Completion::Throw(e) => return Completion::Throw(e),
                    _ => {}
                }
            }
        } else {
            let keys = match enumerable_own_keys(interp, obj_id) {
                Ok(k) => k,
                Err(e) => return Completion::Throw(e),
            };
            for key in keys {
                match json_internalize(interp, &val, &key, reviver, source_map) {
                    Completion::Normal(new_val) => {
                        if let Err(e) = json_internalize_apply(interp, obj_id, &key, new_val) {
                            return Completion::Throw(e);
                        }
                    }
                    Completion::Throw(e) => return Completion::Throw(e),
                    _ => {}
                }
            }
        }
        val.clone()
    } else {
        val.clone()
    };

    // Build context argument for reviver
    let context = {
        let ctx_id = interp.create_object_id();
        if is_json_primitive(&walked)
            && let Some(smap) = source_map
            && let JsValue::Object(o) = holder
            && let Some(src) = smap.get(&(o.id, name.to_js_property_key()))
        {
            // Verify the source text matches the actual value
            // (forward modifications make source invalid)
            let source_matches = match &walked {
                JsValue::Null => src == "null",
                JsValue::Boolean(true) => src == "true",
                JsValue::Boolean(false) => src == "false",
                JsValue::Number(n) => src
                    .parse::<f64>()
                    .is_ok_and(|parsed| (parsed.is_nan() && n.is_nan()) || parsed == *n),
                JsValue::String(s)
                    // Source includes quotes, parse it to compare
                    if src.starts_with('"') && src.ends_with('"') => {
                        let inner = &src[1..src.len() - 1];
                        json_unescape_js_string(inner) == *s
                    }
                _ => false,
            };
            if source_matches {
                interp
                    .get_object_cell_expect(ctx_id)
                    .borrow_mut()
                    .insert_value(
                        "source".to_string(),
                        JsValue::String(JsString::from_str(src)),
                    );
            }
        }
        let id = ctx_id;
        JsValue::Object(crate::types::JsObject { id })
    };
    let key_val = JsValue::String(name.to_js_property_key().to_js_string());
    interp.call_function(reviver, holder, &[key_val, walked, context])
}

fn json_is_valid_number(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let mut i = 0;
    if bytes[i] == b'-' {
        i += 1;
        if i >= bytes.len() {
            return false;
        }
    }
    if bytes[i] == b'0' {
        i += 1;
        // Leading zeros: "01", "00" etc. are invalid JSON
        if i < bytes.len() && bytes[i].is_ascii_digit() {
            return false;
        }
    } else if bytes[i].is_ascii_digit() {
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    } else {
        return false;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        if i >= bytes.len() || !bytes[i].is_ascii_digit() {
            return false;
        }
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        i += 1;
        if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
            i += 1;
        }
        if i >= bytes.len() || !bytes[i].is_ascii_digit() {
            return false;
        }
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    i == bytes.len()
}

fn json_invalid_comma_token(inner: &str, closing_token: char) -> Option<char> {
    let trimmed = json_trim(inner);
    if trimmed.is_empty() {
        return None;
    }
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    let mut prev_was_comma = true; // true initially to detect leading comma
    for ch in inner.chars() {
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
            prev_was_comma = false;
            continue;
        }
        if in_string {
            continue;
        }
        match ch {
            '[' | '{' => {
                depth += 1;
                prev_was_comma = false;
            }
            ']' | '}' => {
                depth -= 1;
                prev_was_comma = false;
            }
            ',' if depth == 0 => {
                if prev_was_comma {
                    return Some(','); // double comma or leading comma
                }
                prev_was_comma = true;
            }
            ' ' | '\t' | '\n' | '\r' => {}
            _ => {
                prev_was_comma = false;
            }
        }
    }
    if !prev_was_comma {
        return None;
    }

    let before_comma = trimmed.strip_suffix(',').map(json_trim).unwrap_or_default();
    if closing_token == '}' && before_comma.ends_with(':') {
        Some(',')
    } else {
        Some(closing_token)
    }
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
    365.0 * (y - 1970) as f64 + (y - 1969).div_euclid(4) as f64 - (y - 1901).div_euclid(100) as f64
        + (y - 1601).div_euclid(400) as f64
}

pub(crate) fn time_from_year(y: f64) -> f64 {
    day_from_year(y) * MS_PER_DAY
}

pub(crate) fn year_from_time(t: f64) -> f64 {
    if t.is_nan() || t.is_infinite() {
        return f64::NAN;
    }
    let a = (t / MS_PER_DAY / 366.0 + 1970.0).floor() as i64 - 1;
    let b = (t / MS_PER_DAY / 365.0 + 1970.0).ceil() as i64 + 1;
    let mut lo = a.min(b);
    let mut hi = a.max(b);
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
    (time_within_day(t) / 3_600_000.0).floor().rem_euclid(24.0) + 0.0
}

pub(crate) fn min_from_time(t: f64) -> f64 {
    (time_within_day(t) / 60_000.0).floor().rem_euclid(60.0) + 0.0
}

pub(crate) fn sec_from_time(t: f64) -> f64 {
    (time_within_day(t) / 1000.0).floor().rem_euclid(60.0) + 0.0
}

pub(crate) fn ms_from_time(t: f64) -> f64 {
    time_within_day(t).rem_euclid(1000.0) + 0.0
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

/// Shared final step of every `Date.prototype.set*` method: combine a day
/// number and a time-within-day into an absolute time, convert back from
/// local time to UTC when `is_local`, and clip to the valid Date range.
pub(crate) fn make_date_clipped(day: f64, time: f64, is_local: bool) -> f64 {
    let combined = make_date(day, time);
    time_clip(if is_local {
        utc_time(combined)
    } else {
        combined
    })
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

fn format_year_string(y: f64) -> String {
    let yi = y as i64;
    if (0..=9999).contains(&yi) {
        format!("{:04}", yi)
    } else if yi < 0 {
        format!("-{:04}", yi.unsigned_abs())
    } else {
        format!("+{}", yi)
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
        "{} {} {:02} {} {:02}:{:02}:{:02} GMT{}{:02}{:02} ({})",
        day_name(wd),
        month_name(m),
        d as i32,
        format_year_string(y),
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
        "{}, {:02} {} {} {:02}:{:02}:{:02} GMT",
        day_name(wd),
        d as i32,
        month_name(m),
        format_year_string(y),
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
        "{} {} {:02} {}",
        day_name(wd),
        month_name(m),
        d as i32,
        format_year_string(y)
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

    // Try space-separated relaxed format: "1997-3-8 1:1:1"
    if let Some(t) = parse_space_separated_date(s) {
        return t;
    }

    // Try toUTCString() format: "Thu, 01 Jan 1970 00:00:00 GMT"
    if let Some(t) = parse_utcstring_format(s) {
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
            return None;
        };
        let offset = sign * (tz_hour * 60.0 + tz_min) * 60_000.0;
        return Some(time_clip(dt - offset));
    }

    None
}

fn parse_space_separated_date(s: &str) -> Option<f64> {
    let bytes = s.as_bytes();
    let len = bytes.len();

    let (year, pos) = parse_iso_year(s)?;

    if pos >= len || bytes[pos] != b'-' {
        return None;
    }
    let pos = pos + 1;

    // Month: 1 or 2 digits
    let (month, pos) = parse_one_or_two_digits(bytes, pos)?;
    if !(1..=12).contains(&month) {
        return None;
    }

    if pos >= len || bytes[pos] != b'-' {
        return None;
    }
    let pos = pos + 1;

    // Day: 1 or 2 digits
    let (day_val, pos) = parse_one_or_two_digits(bytes, pos)?;
    if !(1..=31).contains(&day_val) {
        return None;
    }

    // Must have space separator (not T)
    if pos >= len || bytes[pos] != b' ' {
        return None;
    }
    let pos = pos + 1;

    // Must not be just trailing space
    if pos >= len {
        return None;
    }

    // Hour: 1 or 2 digits
    let (hour, pos) = parse_one_or_two_digits(bytes, pos)?;

    // Must have colon after hour (hour-only is NaN)
    if pos >= len || bytes[pos] != b':' {
        return None;
    }
    let pos = pos + 1;

    // Minute: 1 or 2 digits
    let (minute, pos) = parse_one_or_two_digits(bytes, pos)?;

    let (second, ms_val, pos) = if pos < len && bytes[pos] == b':' {
        let pos = pos + 1;
        let (sec, pos) = parse_one_or_two_digits(bytes, pos)?;
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
        if pos + 1 == len {
            return Some(time_clip(dt));
        }
        return None;
    }

    if ch == b'+' || ch == b'-' {
        let sign: f64 = if ch == b'+' { 1.0 } else { -1.0 };
        let pos = pos + 1;
        if pos + 2 > len {
            return None;
        }
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

fn parse_one_or_two_digits(bytes: &[u8], pos: usize) -> Option<(i32, usize)> {
    if pos >= bytes.len() || !bytes[pos].is_ascii_digit() {
        return None;
    }
    if pos + 1 < bytes.len() && bytes[pos + 1].is_ascii_digit() {
        let val = (bytes[pos] - b'0') as i32 * 10 + (bytes[pos + 1] - b'0') as i32;
        Some((val, pos + 2))
    } else {
        let val = (bytes[pos] - b'0') as i32;
        Some((val, pos + 1))
    }
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
        if sign == -1 && yr == 0 {
            return None;
        }
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

fn parse_utcstring_format(s: &str) -> Option<f64> {
    // "Thu, 01 Jan 1970 00:00:00 GMT"
    let parts: Vec<&str> = s.split_whitespace().collect();
    if parts.len() < 5 {
        return None;
    }
    // Day name must end with comma
    let day_name = parts[0];
    if !day_name.ends_with(',') {
        return None;
    }
    let day_val: i32 = parts[1].parse().ok()?;
    let month_idx = match parts[2] {
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

    // toUTCString is always UTC
    Some(time_clip(dt))
}

pub(crate) fn find_json_colon(s: &str) -> Option<usize> {
    let mut in_string = false;
    let mut escape = false;
    for (i, ch) in s.char_indices() {
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
            let c = first_byte as char;
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

        let mut utf8_bytes = vec![first_byte];
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
            utf8_bytes.push(cont);
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
            result.extend_from_slice(&code_units[start_i..i]);
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

fn hex_val(b: u8) -> Result<u8, String> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err("URI malformed".to_string()),
    }
}

#[cfg(test)]
mod resolve_relative_index_tests {
    use super::resolve_relative_index;

    #[test]
    fn in_range_positive_index_is_unchanged() {
        assert_eq!(resolve_relative_index(3.0, 10), 3);
    }

    #[test]
    fn negative_index_counts_back_from_length() {
        assert_eq!(resolve_relative_index(-3.0, 10), 7);
    }

    #[test]
    fn positive_index_beyond_length_clamps_to_length() {
        assert_eq!(resolve_relative_index(100.0, 10), 10);
    }

    #[test]
    fn negative_index_beyond_length_clamps_to_zero() {
        assert_eq!(resolve_relative_index(-100.0, 10), 0);
    }

    #[test]
    fn negative_index_exactly_at_length_boundary_clamps_to_zero() {
        // n == -(len) - 1 is the boundary where two previously-duplicated
        // implementations of this clamp disagreed on `<` vs `<=`; both landed on 0.
        assert_eq!(resolve_relative_index(-11.0, 10), 0);
    }

    #[test]
    fn positive_infinity_clamps_to_length() {
        assert_eq!(resolve_relative_index(f64::INFINITY, 10), 10);
    }

    #[test]
    fn negative_infinity_clamps_to_zero() {
        assert_eq!(resolve_relative_index(f64::NEG_INFINITY, 10), 0);
    }

    #[test]
    fn zero_length_clamps_everything_to_zero() {
        assert_eq!(resolve_relative_index(-1.0, 0), 0);
        assert_eq!(resolve_relative_index(5.0, 0), 0);
    }
}

#[cfg(test)]
mod make_date_clipped_tests {
    use super::{make_date, make_date_clipped, time_clip, utc_time};

    #[test]
    fn utc_variant_matches_plain_make_date_then_clip() {
        for (day, time) in [(0.0, 0.0), (19858.0, 3_723_000.0), (-100_000.0, 12_345.0)] {
            assert_eq!(
                make_date_clipped(day, time, false),
                time_clip(make_date(day, time))
            );
        }
    }

    #[test]
    fn local_variant_additionally_converts_from_local_to_utc() {
        for (day, time) in [(0.0, 0.0), (19858.0, 3_723_000.0), (-100_000.0, 12_345.0)] {
            assert_eq!(
                make_date_clipped(day, time, true),
                time_clip(utc_time(make_date(day, time)))
            );
        }
    }

    #[test]
    fn nan_day_or_time_propagates_as_nan() {
        assert!(make_date_clipped(f64::NAN, 0.0, false).is_nan());
        assert!(make_date_clipped(0.0, f64::NAN, false).is_nan());
        assert!(make_date_clipped(f64::NAN, 0.0, true).is_nan());
    }

    #[test]
    fn out_of_range_day_clips_to_nan() {
        // 1e9 days is far beyond the +/-100,000,000-day valid Date range.
        assert!(make_date_clipped(1e9, 0.0, false).is_nan());
        assert!(make_date_clipped(1e9, 0.0, true).is_nan());
    }

    #[test]
    fn negative_zero_result_normalizes_to_positive_zero() {
        let v = make_date_clipped(0.0, 0.0, false);
        assert_eq!(v, 0.0);
        assert!(v.is_sign_positive());
    }
}

#[cfg(test)]
mod string_to_number_tests {
    // §7.1.4.1 StringToNumber. Expected values below are the independent source
    // of truth from ECMA-262 §12 (WhiteSpace) and §7.1.4.1 (StrNumericLiteral),
    // cross-checked against node's `Number(...)`.
    use super::string_to_number;
    use crate::types::JsString;

    fn n(s: &str) -> f64 {
        string_to_number(&JsString::from_str(s))
    }

    #[test]
    fn trims_exactly_the_ecmascript_whitespace_set() {
        // In the ECMAScript StrWhiteSpace set (trimmed): must yield 1.
        for ws in [
            "\u{0009}", // TAB
            "\u{000A}", // LF
            "\u{000B}", // VT
            "\u{000C}", // FF
            "\u{000D}", // CR
            "\u{0020}", // SP
            "\u{00A0}", // NBSP
            "\u{FEFF}", // ZWNBSP
            "\u{1680}", // OGHAM SPACE MARK
            "\u{2000}", // EN QUAD
            "\u{200A}", // HAIR SPACE
            "\u{2028}", // LINE SEPARATOR
            "\u{2029}", // PARAGRAPH SEPARATOR
            "\u{202F}", // NNBSP
            "\u{205F}", // MMSP
            "\u{3000}", // IDEOGRAPHIC SPACE
        ] {
            assert_eq!(
                n(&format!("{ws}1{ws}")),
                1.0,
                "U+{:04X} must be trimmed",
                ws.chars().next().unwrap() as u32
            );
        }
        assert_eq!(n("\t\n\r 5 \t\n\r"), 5.0);
    }

    #[test]
    fn does_not_trim_non_ecmascript_whitespace() {
        // U+0085 NEL is in Rust's White_Space but NOT ECMAScript's set.
        assert!(n("\u{0085}1").is_nan());
        // U+200B ZERO WIDTH SPACE is not whitespace in either.
        assert!(n("\u{200B}1").is_nan());
    }

    #[test]
    fn infinity_is_case_sensitive_and_only_the_full_word() {
        assert_eq!(n("Infinity"), f64::INFINITY);
        assert_eq!(n("+Infinity"), f64::INFINITY);
        assert_eq!(n("-Infinity"), f64::NEG_INFINITY);
        // Every other inf/nan spelling Rust's float parser accepts must be NaN.
        for s in [
            "inf", "+inf", "-inf", "INF", "infinity", "INFINITY", "nan", "NaN", "NAN",
        ] {
            assert!(n(s).is_nan(), "Number({s:?}) must be NaN");
        }
    }

    #[test]
    fn non_decimal_integer_literals() {
        assert_eq!(n("0x10"), 16.0);
        assert_eq!(n("0X1F"), 31.0);
        assert_eq!(n("0o17"), 15.0);
        assert_eq!(n("0b101"), 5.0);
        // Large hex must round to the nearest f64, not overflow to NaN.
        assert_eq!(n("0x10000000000000000"), 2f64.powi(64));
        // Convert the exact integer once: incremental f64 accumulation can
        // double-round these values one ULP below their correct result.
        assert_eq!(n("0x6269e107215582e"), 443_215_406_813_239_360.0);
        assert_eq!(n("0x200000000000011"), 2f64.powi(57) + 32.0);
        // Empty digits, bad digits, and a leading sign are all NaN.
        for s in [
            "0x", "0o", "0b", "0xG", "0o8", "0b2", "0x1_0", "0o1_0", "0b1_0", "+0x1", "-0x1",
        ] {
            assert!(n(s).is_nan(), "Number({s:?}) must be NaN");
        }
    }

    #[test]
    fn decimals_and_empty() {
        assert_eq!(n(""), 0.0);
        assert_eq!(n("   "), 0.0);
        assert_eq!(n("  12.5  "), 12.5);
        assert_eq!(n("1e3"), 1000.0);
        assert_eq!(n(".5"), 0.5);
        assert_eq!(n("5."), 5.0);
        assert_eq!(n("-0"), 0.0);
        assert!(n("-0").is_sign_negative());
    }
}
