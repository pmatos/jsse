use super::super::*;
use super::regexp::get_substitution;
use crate::interpreter::helpers::is_ecma_whitespace;
use icu_normalizer::{ComposingNormalizerBorrowed, DecomposingNormalizerBorrowed};
use std::borrow::Cow;

// `? RequireObjectCoercible(this value)` then `? ToString(O)`, keeping the
// exact code units (lone surrogates included). A String wrapper goes through
// ToPrimitive like any other object, so an own or inherited toString /
// @@toPrimitive override is observed.
fn this_js_string(interp: &mut Interpreter, this: &JsValue) -> Result<JsString, Completion> {
    if this.is_nullish() {
        return Err(Completion::Throw(interp.create_type_error(
            "String.prototype method called on null or undefined",
        )));
    }
    interp.to_js_string(this).map_err(Completion::Throw)
}

fn to_str(interp: &mut Interpreter, val: &JsValue) -> Result<String, Completion> {
    match interp.to_string_value(val) {
        Ok(s) => Ok(s),
        Err(e) => Err(Completion::Throw(e)),
    }
}

fn to_num(interp: &mut Interpreter, val: &JsValue) -> Result<f64, Completion> {
    match interp.to_number_value(val) {
        Ok(n) => Ok(n),
        Err(e) => Err(Completion::Throw(e)),
    }
}

// §7.1.5 `? ToIntegerOrInfinity(argument)` in the local `to_num`-style shape
// (Completion errors) used throughout the String prototype builtins.
fn to_int_or_inf(interp: &mut Interpreter, val: &JsValue) -> Result<f64, Completion> {
    match interp.to_integer_or_infinity_value(val) {
        Ok(n) => Ok(n),
        Err(e) => Err(Completion::Throw(e)),
    }
}

// clamp(pos, 0, len) for an already-integral (or ±∞) position.
fn clamp_index(pos: f64, len: usize) -> usize {
    pos.clamp(0.0, len as f64) as usize
}

// §6.1.4.1 StringIndexOf over UTF-16 code units, so lone surrogates only match
// themselves; `None` is ~not-found~.
fn string_index_of(s: &[u16], search: &[u16], from: usize) -> Option<usize> {
    if search.is_empty() {
        return (from <= s.len()).then_some(from);
    }
    s.get(from..)?
        .windows(search.len())
        .position(|window| window == search)
        .map(|i| i + from)
}

// §6.1.4.2 StringLastIndexOf over UTF-16 code units; `from` is clamped to
// `s.len()` by the caller.
fn string_last_index_of(s: &[u16], search: &[u16], from: usize) -> Option<usize> {
    let max_start = from.min(s.len().checked_sub(search.len())?);
    (0..=max_start)
        .rev()
        .find(|&i| &s[i..i + search.len()] == search)
}

// Apply a `&str` transform (case mapping, normalization) to each well-formed
// run of `units`. Lone surrogates pass through unchanged: they are their own
// case mapping and normal form, and neither cased nor case-ignorable, so
// splitting a run at one leaves the transform's context the same.
fn map_well_formed(units: &[u16], mut transform: impl FnMut(&str) -> Cow<'_, str>) -> Vec<u16> {
    let mut out = Vec::with_capacity(units.len());
    let mut run = String::with_capacity(units.len());
    for decoded in char::decode_utf16(units.iter().copied()) {
        match decoded {
            Ok(ch) => run.push(ch),
            Err(lone) => {
                if !run.is_empty() {
                    out.extend(transform(&run).encode_utf16());
                    run.clear();
                }
                out.push(lone.unpaired_surrogate());
            }
        }
    }
    if !run.is_empty() {
        out.extend(transform(&run).encode_utf16());
    }
    out
}

// TrimString over code units; every WhiteSpace and LineTerminator is a single
// BMP code unit. Returns `s` itself when there is nothing to trim.
fn trim_string(s: JsString, trim_start: bool, trim_end: bool) -> JsString {
    let is_space = |unit: &&u16| char::from_u32(u32::from(**unit)).is_some_and(is_ecma_whitespace);
    let units = s.as_units();
    let start = if trim_start {
        units.iter().take_while(is_space).count()
    } else {
        0
    };
    let end = if trim_end {
        units.len() - units[start..].iter().rev().take_while(is_space).count()
    } else {
        units.len()
    };
    if start == 0 && end == units.len() {
        return s;
    }
    JsString::from_vec(units[start..end].to_vec())
}

// §22.1.3.17.1 StringPaddingBuiltinsImpl over code units.
fn string_pad(
    interp: &mut Interpreter,
    this_val: &JsValue,
    args: &[JsValue],
    at_start: bool,
) -> Completion {
    let s = propagate!(this_js_string(interp, this_val));
    let max_length = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
    let int_max = propagate!(interp.to_integer_or_infinity_value(&max_length)) as usize;
    let len = s.len();
    // Step 4 returns before step 6 converts the fill string.
    if int_max <= len {
        return Completion::Normal(JsValue::string(s));
    }
    let fill = match args.get(1) {
        Some(v) if !v.is_undefined() => propagate!(interp.to_js_string(v)),
        _ => JsString::from_str(" "),
    };
    if fill.is_empty() {
        return Completion::Normal(JsValue::string(s));
    }
    let pad = fill.as_units().iter().copied().cycle().take(int_max - len);
    let units = s.as_units().iter().copied();
    let result: Vec<u16> = if at_start {
        pad.chain(units).collect()
    } else {
        units.chain(pad).collect()
    };
    Completion::Normal(JsValue::string(JsString::from_vec(result)))
}

// replace/replaceAll steps 5-6: `None` when the replace value is callable,
// otherwise ? ToString(replaceValue) as the GetSubstitution template.
fn replace_template(
    interp: &mut Interpreter,
    replace_value: &JsValue,
) -> Result<Option<JsString>, Completion> {
    if interp.is_callable(replace_value) {
        return Ok(None);
    }
    interp
        .to_js_string(replace_value)
        .map(Some)
        .map_err(Completion::Throw)
}

// Appends the replacement for the match of the search string at
// `position..end` of `string` in replace/replaceAll to `out`: the replacer's
// result through ToString, or GetSubstitution of `template` when the replace
// value is not callable.
fn append_replacement(
    interp: &mut Interpreter,
    out: &mut Vec<u16>,
    replacer: &JsValue,
    template: Option<&JsString>,
    string: &JsString,
    position: usize,
    end: usize,
) -> Result<(), Completion> {
    let matched = &string.as_units()[position..end];
    if let Some(template) = template {
        let substitution = get_substitution(
            interp,
            matched,
            string.as_units(),
            position,
            end,
            &[],
            &JsValue::UNDEFINED,
            template.as_units(),
        )
        .map_err(Completion::Throw)?;
        out.extend_from_slice(&substitution);
        return Ok(());
    }
    let call_args = [
        JsValue::string(JsString::from_vec(matched.to_vec())),
        JsValue::number(position as f64),
        JsValue::string(string.clone()),
    ];
    let value = match interp.call_function(replacer, &JsValue::UNDEFINED, &call_args) {
        Completion::Normal(v) => v,
        other => return Err(other),
    };
    interp
        .append_js_string(out, &value)
        .map_err(Completion::Throw)
}

// toLocaleLowerCase/toLocaleUpperCase: case mapping for the requested locale.
fn locale_case_map(
    interp: &mut Interpreter,
    this_val: &JsValue,
    args: &[JsValue],
    upper: bool,
) -> Completion {
    let s = propagate!(this_js_string(interp, this_val));
    let locales_arg = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
    let locale_list = propagate!(interp.intl_canonicalize_locale_list(&locales_arg));
    let resolved = interp.intl_resolve_locale(&locale_list);
    let langid: icu::locale::LanguageIdentifier =
        resolved.parse().unwrap_or_else(|_| "und".parse().unwrap());
    let cm = icu::casemap::CaseMapper::new();
    let mapped = map_well_formed(s.as_units(), |run| {
        if upper {
            cm.uppercase_to_string(run, &langid)
        } else {
            cm.lowercase_to_string(run, &langid)
        }
    });
    Completion::Normal(JsValue::string(JsString::from_vec(mapped)))
}

// Annex B CreateHTML: `<tag attribute="value">s</tag>`, with `"` in the value
// escaped as `&quot;`.
fn create_html(s: &JsString, tag: &str, attribute: Option<(&str, &JsString)>) -> JsString {
    let mut html: Vec<u16> = format!("<{tag}").encode_utf16().collect();
    if let Some((name, value)) = attribute {
        html.extend(format!(" {name}=\"").encode_utf16());
        for &unit in value.as_units().iter() {
            if unit == u16::from(b'"') {
                html.extend("&quot;".encode_utf16());
            } else {
                html.push(unit);
            }
        }
        html.push(u16::from(b'"'));
    }
    html.push(u16::from(b'>'));
    html.extend_from_slice(s.as_units());
    html.extend(format!("</{tag}>").encode_utf16());
    JsString::from_vec(html)
}

// §7.2.8 IsRegExp(argument) — uses [[Get]] for Symbol.match (invokes getters)
fn is_regexp(interp: &mut Interpreter, obj_id: u64, obj_val: &JsValue) -> Result<bool, JsValue> {
    if let Some(match_key) = interp.get_symbol_key("match") {
        match interp.get_object_property(obj_id, &match_key, obj_val) {
            Completion::Normal(v) if !(v).is_undefined() => {
                return Ok(interp.to_boolean_val(&v));
            }
            Completion::Normal(_) => {}
            Completion::Throw(e) => return Err(e),
            _ => {}
        }
    }
    Ok(interp
        .get_object_cell(obj_id)
        .map(|obj| obj.borrow().class_name == "RegExp")
        .unwrap_or(false))
}

// Steps 3-4 of includes/startsWith/endsWith: throw a TypeError when the search
// argument IsRegExp.
fn reject_regexp(interp: &mut Interpreter, arg: &JsValue, method: &str) -> Result<(), Completion> {
    if let Some(object_id) = arg.as_object_id()
        && is_regexp(interp, object_id, arg).map_err(Completion::Throw)?
    {
        return Err(Completion::Throw(interp.create_type_error(&format!(
            "First argument to String.prototype.{method} must not be a regular expression"
        ))));
    }
    Ok(())
}

impl Interpreter {
    pub(crate) fn setup_string_prototype(&mut self) {
        let proto_id = self.create_object_id();
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .class_name = "String".to_string();
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .primitive_value = Some(JsValue::string(JsString::from_str("")));

        #[allow(clippy::type_complexity)]
        let methods: Vec<(
            &str,
            usize,
            Rc<dyn Fn(&mut Interpreter, &JsValue, &[JsValue]) -> Completion>,
        )> = vec![
            (
                "charAt",
                1,
                Rc::new(|interp, this_val, args| {
                    let js_str = propagate!(this_js_string(interp, this_val));
                    let pos = match args.first() {
                        Some(v) => match to_int_or_inf(interp, v) {
                            Ok(n) => n,
                            Err(c) => return c,
                        },
                        None => 0.0,
                    };
                    let units = js_str.as_units();
                    let idx = pos as isize;
                    if idx < 0 || idx as usize >= units.len() {
                        return Completion::Normal(JsValue::string(JsString::from_str("")));
                    }
                    Completion::Normal(JsValue::string(JsString::from_vec(vec![
                        units[idx as usize],
                    ])))
                }),
            ),
            (
                "charCodeAt",
                1,
                Rc::new(|interp, this_val, args| {
                    let js_str = propagate!(this_js_string(interp, this_val));
                    let pos = match args.first() {
                        Some(v) => match to_int_or_inf(interp, v) {
                            Ok(n) => n,
                            Err(c) => return c,
                        },
                        None => 0.0,
                    };
                    let units = js_str.as_units();
                    let idx = pos as isize;
                    if idx < 0 || idx as usize >= units.len() {
                        return Completion::Normal(JsValue::number(f64::NAN));
                    }
                    Completion::Normal(JsValue::number(units[idx as usize] as f64))
                }),
            ),
            (
                "codePointAt",
                1,
                Rc::new(|interp, this_val, args| {
                    let js_str = propagate!(this_js_string(interp, this_val));
                    let pos = match args.first() {
                        Some(v) => match to_int_or_inf(interp, v) {
                            Ok(n) => n,
                            Err(c) => return c,
                        },
                        None => 0.0,
                    };
                    let units = js_str.as_units();
                    let idx = pos as isize;
                    if idx < 0 || idx as usize >= units.len() {
                        return Completion::Normal(JsValue::UNDEFINED);
                    }
                    let idx = idx as usize;
                    let code = units[idx];
                    if (0xD800..=0xDBFF).contains(&code) && idx + 1 < units.len() {
                        let trail = units[idx + 1];
                        if (0xDC00..=0xDFFF).contains(&trail) {
                            let cp =
                                ((code as u32 - 0xD800) << 10) + (trail as u32 - 0xDC00) + 0x10000;
                            return Completion::Normal(JsValue::number(cp as f64));
                        }
                    }
                    Completion::Normal(JsValue::number(code as f64))
                }),
            ),
            (
                "indexOf",
                1,
                Rc::new(|interp, this_val, args| {
                    let s = propagate!(this_js_string(interp, this_val));
                    let search_arg = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let search = propagate!(interp.to_js_string(&search_arg));
                    let position = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                    let pos = propagate!(interp.to_integer_or_infinity_value(&position));
                    let units = s.as_units();
                    let start = clamp_index(pos, units.len());
                    let index = string_index_of(units, search.as_units(), start);
                    Completion::Normal(JsValue::number(index.map_or(-1.0, |i| i as f64)))
                }),
            ),
            (
                "lastIndexOf",
                1,
                Rc::new(|interp, this_val, args| {
                    let s = propagate!(this_js_string(interp, this_val));
                    let search_arg = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let search = propagate!(interp.to_js_string(&search_arg));
                    let position = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                    let num_pos = propagate!(interp.to_number_value(&position));
                    let units = s.as_units();
                    let start = if num_pos.is_nan() {
                        units.len()
                    } else {
                        clamp_index(to_integer_or_infinity(num_pos), units.len())
                    };
                    let index = string_last_index_of(units, search.as_units(), start);
                    Completion::Normal(JsValue::number(index.map_or(-1.0, |i| i as f64)))
                }),
            ),
            (
                "includes",
                1,
                Rc::new(|interp, this_val, args| {
                    let s = propagate!(this_js_string(interp, this_val));
                    let search_arg = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    propagate!(reject_regexp(interp, &search_arg, "includes"));
                    let search = propagate!(interp.to_js_string(&search_arg));
                    let position = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                    let pos = propagate!(interp.to_integer_or_infinity_value(&position));
                    let units = s.as_units();
                    let start = clamp_index(pos, units.len());
                    let found = string_index_of(units, search.as_units(), start).is_some();
                    Completion::Normal(JsValue::boolean(found))
                }),
            ),
            (
                "startsWith",
                1,
                Rc::new(|interp, this_val, args| {
                    let s = propagate!(this_js_string(interp, this_val));
                    let search_arg = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    propagate!(reject_regexp(interp, &search_arg, "startsWith"));
                    let search = propagate!(interp.to_js_string(&search_arg));
                    let position = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                    let pos = propagate!(interp.to_integer_or_infinity_value(&position));
                    let units = s.as_units();
                    let start = clamp_index(pos, units.len());
                    let result = units[start..].starts_with(search.as_units());
                    Completion::Normal(JsValue::boolean(result))
                }),
            ),
            (
                "endsWith",
                1,
                Rc::new(|interp, this_val, args| {
                    let s = propagate!(this_js_string(interp, this_val));
                    let search_arg = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    propagate!(reject_regexp(interp, &search_arg, "endsWith"));
                    let search = propagate!(interp.to_js_string(&search_arg));
                    let units = s.as_units();
                    let end = match args.get(1) {
                        Some(v) if !v.is_undefined() => clamp_index(
                            propagate!(interp.to_integer_or_infinity_value(v)),
                            units.len(),
                        ),
                        _ => units.len(),
                    };
                    let result = units[..end].ends_with(search.as_units());
                    Completion::Normal(JsValue::boolean(result))
                }),
            ),
            (
                "slice",
                2,
                Rc::new(|interp, this_val, args| {
                    let js_str = propagate!(this_js_string(interp, this_val));
                    let units = js_str.as_units();
                    let from = match resolve_start_index(interp, args.first(), units.len()) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    let to = match resolve_end_index(interp, args.get(1), units.len()) {
                        Ok(v) => v,
                        Err(c) => return c,
                    };
                    if from >= to {
                        return Completion::Normal(JsValue::string(JsString::from_str("")));
                    }
                    Completion::Normal(JsValue::string(JsString::from_vec(
                        units[from..to].to_vec(),
                    )))
                }),
            ),
            (
                "substring",
                2,
                Rc::new(|interp, this_val, args| {
                    let js_str = propagate!(this_js_string(interp, this_val));
                    let units = js_str.as_units();
                    let len = units.len() as f64;
                    let int_start = match args.first() {
                        Some(v) => match to_int_or_inf(interp, v) {
                            Ok(n) => n,
                            Err(c) => return c,
                        },
                        None => 0.0,
                    };
                    let int_end = match args.get(1) {
                        Some(v) if !(v).is_undefined() => match to_int_or_inf(interp, v) {
                            Ok(n) => n,
                            Err(c) => return c,
                        },
                        _ => len,
                    };
                    let final_start = int_start.max(0.0).min(len) as usize;
                    let final_end = int_end.max(0.0).min(len) as usize;
                    let (from, to) = if final_start <= final_end {
                        (final_start, final_end)
                    } else {
                        (final_end, final_start)
                    };
                    Completion::Normal(JsValue::string(JsString::from_vec(
                        units[from..to].to_vec(),
                    )))
                }),
            ),
            (
                "toLowerCase",
                0,
                Rc::new(|interp, this_val, _args| {
                    let s = propagate!(this_js_string(interp, this_val));
                    let lower = map_well_formed(s.as_units(), |run| Cow::Owned(run.to_lowercase()));
                    Completion::Normal(JsValue::string(JsString::from_vec(lower)))
                }),
            ),
            (
                "toUpperCase",
                0,
                Rc::new(|interp, this_val, _args| {
                    let s = propagate!(this_js_string(interp, this_val));
                    let upper = map_well_formed(s.as_units(), |run| Cow::Owned(run.to_uppercase()));
                    Completion::Normal(JsValue::string(JsString::from_vec(upper)))
                }),
            ),
            (
                "toLocaleLowerCase",
                0,
                Rc::new(|interp, this_val, args| locale_case_map(interp, this_val, args, false)),
            ),
            (
                "toLocaleUpperCase",
                0,
                Rc::new(|interp, this_val, args| locale_case_map(interp, this_val, args, true)),
            ),
            (
                "trim",
                0,
                Rc::new(|interp, this_val, _args| {
                    let s = propagate!(this_js_string(interp, this_val));
                    Completion::Normal(JsValue::string(trim_string(s, true, true)))
                }),
            ),
            (
                "trimStart",
                0,
                Rc::new(|interp, this_val, _args| {
                    let s = propagate!(this_js_string(interp, this_val));
                    Completion::Normal(JsValue::string(trim_string(s, true, false)))
                }),
            ),
            (
                "trimEnd",
                0,
                Rc::new(|interp, this_val, _args| {
                    let s = propagate!(this_js_string(interp, this_val));
                    Completion::Normal(JsValue::string(trim_string(s, false, true)))
                }),
            ),
            (
                "repeat",
                1,
                Rc::new(|interp, this_val, args| {
                    let s = propagate!(this_js_string(interp, this_val));
                    let count = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let n = propagate!(interp.to_integer_or_infinity_value(&count));
                    if n < 0.0 || n == f64::INFINITY {
                        return Completion::Throw(interp.create_range_error("Invalid count value"));
                    }
                    let repeated = s.as_units().repeat(n as usize);
                    Completion::Normal(JsValue::string(JsString::from_vec(repeated)))
                }),
            ),
            (
                "padStart",
                1,
                Rc::new(|interp, this_val, args| string_pad(interp, this_val, args, true)),
            ),
            (
                "padEnd",
                1,
                Rc::new(|interp, this_val, args| string_pad(interp, this_val, args, false)),
            ),
            (
                "concat",
                1,
                Rc::new(|interp, this_val, args| {
                    let s = propagate!(this_js_string(interp, this_val));
                    let mut tail: Vec<u16> = Vec::new();
                    for arg in args {
                        propagate!(interp.append_js_string(&mut tail, arg));
                    }
                    Completion::Normal(JsValue::string(s.concat_units(&tail)))
                }),
            ),
            (
                "split",
                2,
                Rc::new(|interp, this_val, args| {
                    // RequireObjectCoercible
                    if (this_val).is_nullish() {
                        return Completion::Throw(interp.create_type_error(
                            "String.prototype.split called on null or undefined",
                        ));
                    }
                    let separator = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    // Check for Symbol.split on the separator
                    if let Some(object_id) = separator.as_object_id()
                        && let Some(key) = interp.get_symbol_key("split")
                    {
                        let method = match interp.get_object_property(object_id, &key, &separator) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            other => return other,
                        };
                        if !(method).is_nullish() {
                            let this_str = this_val.clone();
                            let limit = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                            return interp.call_function(&method, &separator, &[this_str, limit]);
                        }
                    }
                    let s = propagate!(interp.to_js_string(this_val));
                    let limit_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                    // Per spec: ToUint32(limit) — NaN/Infinity/2^32/etc map to 0
                    let limit: u32 = if (limit_arg).is_undefined() {
                        0xFFFF_FFFF // 2^32 - 1
                    } else {
                        match to_num(interp, &limit_arg) {
                            Ok(n) => crate::types::number_ops::to_uint32(n),
                            Err(c) => return c,
                        }
                    };
                    // Per spec step 7: ToString(separator) BEFORE limit check
                    let sep = if separator.is_undefined() {
                        None
                    } else {
                        Some(propagate!(interp.to_js_string(&separator)))
                    };
                    if limit == 0 {
                        return Completion::Normal(interp.create_array(vec![]));
                    }
                    let Some(sep) = sep else {
                        return Completion::Normal(interp.create_array(vec![JsValue::string(s)]));
                    };
                    let limit = limit as usize;
                    let s_units = s.as_units();
                    let sep_units = sep.as_units();

                    if sep_units.is_empty() {
                        // Split into individual UTF-16 code units
                        let parts = s_units
                            .iter()
                            .take(limit)
                            .map(|&unit| JsValue::string(JsString::from_vec(vec![unit])))
                            .collect();
                        return Completion::Normal(interp.create_array(parts));
                    }

                    let mut parts = Vec::new();
                    let mut p = 0usize; // start of unmatched portion
                    while let Some(q) = string_index_of(s_units, sep_units, p) {
                        parts.push(JsValue::string(JsString::from_vec(s_units[p..q].to_vec())));
                        if parts.len() >= limit {
                            return Completion::Normal(interp.create_array(parts));
                        }
                        p = q + sep_units.len();
                    }
                    parts.push(JsValue::string(JsString::from_vec(s_units[p..].to_vec())));
                    Completion::Normal(interp.create_array(parts))
                }),
            ),
            (
                "replace",
                2,
                Rc::new(|interp, this_val, args| {
                    if (this_val).is_nullish() {
                        return Completion::Throw(interp.create_type_error(
                            "String.prototype.replace called on null or undefined",
                        ));
                    }
                    let search_value = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    if let Some(object_id) = search_value.as_object_id()
                        && let Some(key) = interp.get_symbol_key("replace")
                    {
                        let method =
                            match interp.get_object_property(object_id, &key, &search_value) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                other => return other,
                            };
                        if !(method).is_nullish() {
                            let replace_val = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                            return interp.call_function(
                                &method,
                                &search_value,
                                &[this_val.clone(), replace_val],
                            );
                        }
                    }
                    let s = propagate!(interp.to_js_string(this_val));
                    let search = propagate!(interp.to_js_string(&search_value));
                    let replace_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                    let template = propagate!(replace_template(interp, &replace_arg));
                    let s_units = s.as_units();
                    let search_units = search.as_units();
                    let Some(pos) = string_index_of(s_units, search_units, 0) else {
                        return Completion::Normal(JsValue::string(s.clone()));
                    };
                    let end = pos + search_units.len();
                    let mut result = Vec::with_capacity(s_units.len());
                    result.extend_from_slice(&s_units[..pos]);
                    propagate!(append_replacement(
                        interp,
                        &mut result,
                        &replace_arg,
                        template.as_ref(),
                        &s,
                        pos,
                        end
                    ));
                    result.extend_from_slice(&s_units[end..]);
                    Completion::Normal(JsValue::string(JsString::from_vec(result)))
                }),
            ),
            (
                "replaceAll",
                2,
                Rc::new(|interp, this_val, args| {
                    if (this_val).is_nullish() {
                        return Completion::Throw(interp.create_type_error(
                            "String.prototype.replaceAll called on null or undefined",
                        ));
                    }
                    let search_value = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    if let Some(object_id) = search_value.as_object_id() {
                        // Step 2a: IsRegExp check
                        let is_regexp = if let Some(match_key) = interp.get_symbol_key("match") {
                            match interp.get_object_property(object_id, &match_key, &search_value) {
                                Completion::Normal(v) if !(v).is_undefined() => {
                                    interp.to_boolean_val(&v)
                                }
                                Completion::Normal(_) => interp
                                    .get_object_cell(object_id)
                                    .map(|obj| obj.borrow().class_name == "RegExp")
                                    .unwrap_or(false),
                                Completion::Throw(e) => return Completion::Throw(e),
                                other => return other,
                            }
                        } else {
                            interp
                                .get_object_cell(object_id)
                                .map(|obj| obj.borrow().class_name == "RegExp")
                                .unwrap_or(false)
                        };
                        if is_regexp {
                            // Step 2b: Get flags via getter
                            let flags_val =
                                match interp.get_object_property(object_id, "flags", &search_value)
                                {
                                    Completion::Normal(v) => v,
                                    Completion::Throw(e) => return Completion::Throw(e),
                                    other => return other,
                                };
                            let flags = propagate!(interp.to_string_value(&flags_val));
                            if !flags.contains('g') {
                                return Completion::Throw(interp.create_type_error(
                                    "String.prototype.replaceAll called with a non-global RegExp argument",
                                ));
                            }
                        }
                        // Step 2c-d: GetMethod(searchValue, @@replace)
                        if let Some(key) = interp.get_symbol_key("replace") {
                            let method =
                                match interp.get_object_property(object_id, &key, &search_value) {
                                    Completion::Normal(v) => v,
                                    Completion::Throw(e) => return Completion::Throw(e),
                                    other => return other,
                                };
                            if !(method).is_nullish() {
                                let replace_val =
                                    args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                                return interp.call_function(
                                    &method,
                                    &search_value,
                                    &[this_val.clone(), replace_val],
                                );
                            }
                        }
                    }
                    let s = propagate!(interp.to_js_string(this_val));
                    let search = propagate!(interp.to_js_string(&search_value));
                    let replace_arg = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                    // Per spec step 6a: if not functional, ToString(replaceValue) once upfront
                    let template = propagate!(replace_template(interp, &replace_arg));
                    let s_units = s.as_units();
                    let search_units = search.as_units();
                    let advance_by = search_units.len().max(1);
                    let mut positions = Vec::new();
                    let mut next = string_index_of(s_units, search_units, 0);
                    while let Some(pos) = next {
                        positions.push(pos);
                        next = string_index_of(s_units, search_units, pos + advance_by);
                    }
                    if positions.is_empty() {
                        return Completion::Normal(JsValue::string(s.clone()));
                    }

                    let mut result = Vec::with_capacity(s_units.len());
                    let mut end_of_last_match = 0;
                    for pos in positions {
                        result.extend_from_slice(&s_units[end_of_last_match..pos]);
                        let end = pos + search_units.len();
                        propagate!(append_replacement(
                            interp,
                            &mut result,
                            &replace_arg,
                            template.as_ref(),
                            &s,
                            pos,
                            end
                        ));
                        end_of_last_match = end;
                    }
                    result.extend_from_slice(&s_units[end_of_last_match..]);
                    Completion::Normal(JsValue::string(JsString::from_vec(result)))
                }),
            ),
            (
                "at",
                1,
                Rc::new(|interp, this_val, args| {
                    let js_str = propagate!(this_js_string(interp, this_val));
                    let units = js_str.as_units();
                    let idx = match args.first() {
                        Some(v) => match to_int_or_inf(interp, v) {
                            Ok(n) => n,
                            Err(c) => return c,
                        },
                        None => 0.0,
                    };
                    let actual = match resolve_element_index(idx, units.len()) {
                        Some(k) => k,
                        None => return Completion::Normal(JsValue::UNDEFINED),
                    };
                    Completion::Normal(JsValue::string(JsString::from_vec(vec![units[actual]])))
                }),
            ),
            (
                "search",
                1,
                Rc::new(|interp, this_val, args| {
                    if (this_val).is_nullish() {
                        return Completion::Throw(interp.create_type_error(
                            "String.prototype.search called on null or undefined",
                        ));
                    }
                    let regexp = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    if let Some(object_id) = regexp.as_object_id()
                        && let Some(key) = interp.get_symbol_key("search")
                    {
                        let method = match interp.get_object_property(object_id, &key, &regexp) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            other => return other,
                        };
                        if !(method).is_nullish() {
                            let this_str = this_val.clone();
                            return interp.call_function(&method, &regexp, &[this_str]);
                        }
                    }
                    let s = match to_str(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let source = if (regexp).is_undefined() {
                        String::new()
                    } else {
                        match to_str(interp, &regexp) {
                            Ok(s) => s,
                            Err(c) => return c,
                        }
                    };
                    // Create a RegExp and call @@search
                    let rx = interp.create_regexp(&source, "");
                    if let Some(regexp_id) = rx.as_object_id()
                        && let Some(key) = interp.get_symbol_key("search")
                    {
                        let method = interp.get_property_on_id(regexp_id, &key);
                        if !(method).is_nullish() {
                            let this_str = JsValue::string(JsString::from_str(&s));
                            return interp.call_function(&method, &rx, &[this_str]);
                        }
                    }
                    // Fallback
                    if let Ok(re) = regex::Regex::new(&source)
                        && let Some(m) = re.find(&s)
                    {
                        return Completion::Normal(JsValue::number(m.start() as f64));
                    }
                    Completion::Normal(JsValue::number(-1.0))
                }),
            ),
            (
                "match",
                1,
                Rc::new(|interp, this_val, args| {
                    if (this_val).is_nullish() {
                        return Completion::Throw(interp.create_type_error(
                            "String.prototype.match called on null or undefined",
                        ));
                    }
                    let regexp = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    if let Some(object_id) = regexp.as_object_id()
                        && let Some(key) = interp.get_symbol_key("match")
                    {
                        let method = match interp.get_object_property(object_id, &key, &regexp) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            other => return other,
                        };
                        if !(method).is_nullish() {
                            let this_str = this_val.clone();
                            return interp.call_function(&method, &regexp, &[this_str]);
                        }
                    }
                    let s = match to_str(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let source = if args.is_empty() || (regexp).is_undefined() {
                        String::new()
                    } else {
                        match to_str(interp, &regexp) {
                            Ok(s) => s,
                            Err(c) => return c,
                        }
                    };
                    // Create a RegExp and call @@match
                    let rx = interp.create_regexp(&source, "");
                    if let Some(regexp_id) = rx.as_object_id()
                        && let Some(key) = interp.get_symbol_key("match")
                    {
                        let method = interp.get_property_on_id(regexp_id, &key);
                        if !(method).is_nullish() {
                            let this_str = JsValue::string(JsString::from_str(&s));
                            return interp.call_function(&method, &rx, &[this_str]);
                        }
                    }
                    // Fallback
                    if let Ok(re) = regex::Regex::new(&source) {
                        if let Some(m) = re.find(&s) {
                            let matched = JsValue::string(JsString::from_str(m.as_str()));
                            let result = interp.create_array(vec![matched]);
                            if let Some(result_id) = result.as_object_id()
                                && let Some(robj) = interp.get_object_cell(result_id)
                            {
                                robj.borrow_mut().insert_value(
                                    "index".to_string(),
                                    JsValue::number(m.start() as f64),
                                );
                                robj.borrow_mut().insert_value(
                                    "input".to_string(),
                                    JsValue::string(JsString::from_str(&s)),
                                );
                            }
                            Completion::Normal(result)
                        } else {
                            Completion::Normal(JsValue::NULL)
                        }
                    } else {
                        Completion::Normal(JsValue::NULL)
                    }
                }),
            ),
            (
                "matchAll",
                1,
                Rc::new(|interp, this_val, args| {
                    if (this_val).is_nullish() {
                        return Completion::Throw(interp.create_type_error(
                            "String.prototype.matchAll called on null or undefined",
                        ));
                    }
                    let regexp = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    if let Some(object_id) = regexp.as_object_id() {
                        // IsRegExp check
                        let is_regexp = if let Some(match_key) = interp.get_symbol_key("match") {
                            match interp.get_object_property(object_id, &match_key, &regexp) {
                                Completion::Normal(v) if !(v).is_undefined() => {
                                    interp.to_boolean_val(&v)
                                }
                                Completion::Normal(_) => interp
                                    .get_object_cell(object_id)
                                    .map(|obj| obj.borrow().class_name == "RegExp")
                                    .unwrap_or(false),
                                Completion::Throw(e) => return Completion::Throw(e),
                                other => return other,
                            }
                        } else {
                            interp
                                .get_object_cell(object_id)
                                .map(|obj| obj.borrow().class_name == "RegExp")
                                .unwrap_or(false)
                        };
                        if is_regexp {
                            let flags_val =
                                match interp.get_object_property(object_id, "flags", &regexp) {
                                    Completion::Normal(v) => v,
                                    Completion::Throw(e) => return Completion::Throw(e),
                                    other => return other,
                                };
                            let flags = propagate!(interp.to_string_value(&flags_val));
                            if !flags.contains('g') {
                                return Completion::Throw(interp.create_type_error(
                                    "String.prototype.matchAll called with a non-global RegExp argument",
                                ));
                            }
                        }
                        if let Some(key) = interp.get_symbol_key("matchAll") {
                            let method = match interp.get_object_property(object_id, &key, &regexp)
                            {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                other => return other,
                            };
                            if !(method).is_nullish() {
                                let this_str = this_val.clone();
                                return interp.call_function(&method, &regexp, &[this_str]);
                            }
                        }
                    }
                    // Step 3: Let S be ? ToString(O)
                    let s = match to_str(interp, this_val) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    // Step 4: Let R be ? ToString(regexp)
                    // Per spec: RegExpCreate(regexp, "g") — undefined -> empty pattern
                    let source = if (regexp).is_undefined() {
                        String::new()
                    } else {
                        match to_str(interp, &regexp) {
                            Ok(s) => s,
                            Err(c) => return c,
                        }
                    };
                    let rx = interp.create_regexp(&source, "g");
                    if let Some(regexp_id) = rx.as_object_id()
                        && let Some(key) = interp.get_symbol_key("matchAll")
                    {
                        let method = match interp.get_object_property(regexp_id, &key, &rx) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            other => return other,
                        };
                        if !(method).is_nullish() {
                            let this_str = JsValue::string(JsString::from_str(&s));
                            return interp.call_function(&method, &rx, &[this_str]);
                        }
                    }
                    Completion::Throw(interp.create_type_error("matchAll requires a global RegExp"))
                }),
            ),
            (
                "normalize",
                0,
                Rc::new(|interp, this_val, args| {
                    let s = propagate!(this_js_string(interp, this_val));
                    let form = match args.first() {
                        Some(v) if !(v).is_undefined() => match to_str(interp, v) {
                            Ok(s) => s,
                            Err(c) => return c,
                        },
                        _ => "NFC".to_string(),
                    };
                    let normalize: fn(&str) -> Cow<'_, str> = match form.as_str() {
                        "NFC" => |run| ComposingNormalizerBorrowed::new_nfc().normalize(run),
                        "NFD" => |run| DecomposingNormalizerBorrowed::new_nfd().normalize(run),
                        "NFKC" => |run| ComposingNormalizerBorrowed::new_nfkc().normalize(run),
                        "NFKD" => |run| DecomposingNormalizerBorrowed::new_nfkd().normalize(run),
                        _ => {
                            return Completion::Throw(interp.create_range_error(&format!(
                                "The normalization form should be one of NFC, NFD, NFKC, NFKD. Got: {form}"
                            )));
                        }
                    };
                    let normalized = map_well_formed(s.as_units(), normalize);
                    Completion::Normal(JsValue::string(JsString::from_vec(normalized)))
                }),
            ),
            (
                "localeCompare",
                1,
                Rc::new(|interp, this_val, args| {
                    // The collator compares decoded text, so lone surrogates
                    // still reach it as U+FFFD.
                    let s = propagate!(this_js_string(interp, this_val)).to_rust_string();
                    let that = match args.first() {
                        Some(v) => match to_str(interp, v) {
                            Ok(s) => s,
                            Err(c) => return c,
                        },
                        None => "undefined".to_string(),
                    };
                    let locales = args.get(1).cloned().unwrap_or(JsValue::UNDEFINED);
                    let options = args.get(2).cloned().unwrap_or(JsValue::UNDEFINED);
                    match interp.intl_locale_compare(&s, &that, &locales, &options) {
                        Ok(result) => Completion::Normal(JsValue::number(result)),
                        Err(e) => Completion::Throw(e),
                    }
                }),
            ),
            (
                "isWellFormed",
                0,
                Rc::new(|interp, this_val, _args| {
                    let js_str = propagate!(this_js_string(interp, this_val));
                    let units = js_str.as_units();
                    let mut i = 0;
                    while i < units.len() {
                        let cu = units[i];
                        if (0xD800..=0xDBFF).contains(&cu) {
                            if i + 1 >= units.len() || !(0xDC00..=0xDFFF).contains(&units[i + 1]) {
                                return Completion::Normal(JsValue::boolean(false));
                            }
                            i += 2;
                        } else if (0xDC00..=0xDFFF).contains(&cu) {
                            return Completion::Normal(JsValue::boolean(false));
                        } else {
                            i += 1;
                        }
                    }
                    Completion::Normal(JsValue::boolean(true))
                }),
            ),
            (
                "toWellFormed",
                0,
                Rc::new(|interp, this_val, _args| {
                    let js_str = propagate!(this_js_string(interp, this_val));
                    let units = js_str.as_units();
                    let mut result = Vec::with_capacity(units.len());
                    let mut i = 0;
                    while i < units.len() {
                        let cu = units[i];
                        if (0xD800..=0xDBFF).contains(&cu) {
                            if i + 1 < units.len() && (0xDC00..=0xDFFF).contains(&units[i + 1]) {
                                result.push(cu);
                                result.push(units[i + 1]);
                                i += 2;
                            } else {
                                result.push(0xFFFD);
                                i += 1;
                            }
                        } else if (0xDC00..=0xDFFF).contains(&cu) {
                            result.push(0xFFFD);
                            i += 1;
                        } else {
                            result.push(cu);
                            i += 1;
                        }
                    }
                    Completion::Normal(JsValue::string(JsString::from_vec(result)))
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

        // toString and valueOf: realm-aware so cross-realm calls throw the right realm's TypeError
        {
            let realm_id = self.current_realm_id;
            let tostring_fn = self.create_function(JsFunction::native(
                "toString".to_string(),
                0,
                move |interp, this_val, _args| {
                    if this_val.is_string() {
                        return Completion::Normal(this_val.clone());
                    }
                    if let Some(object_id) = this_val.as_object_id()
                        && let Some(obj) = interp.get_object_cell(object_id)
                        && obj.borrow().class_name == "String"
                        && let Some(ref primitive) = obj.borrow().primitive_value
                    {
                        return Completion::Normal(primitive.clone());
                    }
                    Completion::Throw(interp.create_error_in_realm(
                        realm_id,
                        "TypeError",
                        "String.prototype.toString requires that 'this' be a String",
                    ))
                },
            ));
            self.get_object_cell_expect(proto_id)
                .borrow_mut()
                .insert_builtin("toString".to_string(), tostring_fn);

            let valueof_fn = self.create_function(JsFunction::native(
                "valueOf".to_string(),
                0,
                move |interp, this_val, _args| {
                    if this_val.is_string() {
                        return Completion::Normal(this_val.clone());
                    }
                    if let Some(object_id) = this_val.as_object_id()
                        && let Some(obj) = interp.get_object_cell(object_id)
                        && obj.borrow().class_name == "String"
                        && let Some(ref primitive) = obj.borrow().primitive_value
                    {
                        return Completion::Normal(primitive.clone());
                    }
                    Completion::Throw(interp.create_error_in_realm(
                        realm_id,
                        "TypeError",
                        "String.prototype.valueOf requires that 'this' be a String",
                    ))
                },
            ));
            self.get_object_cell_expect(proto_id)
                .borrow_mut()
                .insert_builtin("valueOf".to_string(), valueof_fn);
        }

        // Annex B: HTML methods
        let html_no_arg = [
            ("big", "big"),
            ("blink", "blink"),
            ("bold", "b"),
            ("fixed", "tt"),
            ("italics", "i"),
            ("small", "small"),
            ("strike", "strike"),
            ("sub", "sub"),
            ("sup", "sup"),
        ];
        for (name, tag) in html_no_arg {
            let fn_val = self.create_function(JsFunction::Native(
                name.to_string(),
                0,
                Rc::new(move |interp, this, _args| {
                    let s = propagate!(this_js_string(interp, this));
                    Completion::Normal(JsValue::string(create_html(&s, tag, None)))
                }),
                false,
            ));
            self.get_object_cell_expect(proto_id)
                .borrow_mut()
                .insert_builtin(name.to_string(), fn_val);
        }

        let html_one_arg = [
            ("anchor", "a", "name"),
            ("fontcolor", "font", "color"),
            ("fontsize", "font", "size"),
            ("link", "a", "href"),
        ];
        for (name, tag, attr) in html_one_arg {
            let fn_val = self.create_function(JsFunction::Native(
                name.to_string(),
                1,
                Rc::new(move |interp, this, args| {
                    let s = propagate!(this_js_string(interp, this));
                    let arg = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                    let value = propagate!(interp.to_js_string(&arg));
                    Completion::Normal(JsValue::string(create_html(&s, tag, Some((attr, &value)))))
                }),
                false,
            ));
            self.get_object_cell_expect(proto_id)
                .borrow_mut()
                .insert_builtin(name.to_string(), fn_val);
        }

        // Annex B: substr(start, length)
        {
            let fn_val = self.create_function(JsFunction::Native(
                "substr".to_string(),
                2,
                Rc::new(|interp, this, args| {
                    let js_str = match this_js_string(interp, this) {
                        Ok(s) => s,
                        Err(c) => return c,
                    };
                    let units = js_str.as_units();
                    let len = units.len() as f64;
                    let int_start = match args.first() {
                        Some(v) => match to_int_or_inf(interp, v) {
                            Ok(n) => n,
                            Err(c) => return c,
                        },
                        None => 0.0,
                    };
                    let start = if int_start == f64::NEG_INFINITY {
                        0
                    } else if int_start < 0.0 {
                        (len + int_start).max(0.0) as usize
                    } else {
                        int_start.min(len) as usize
                    };
                    let result_len = match args.get(1) {
                        Some(v) if !(v).is_undefined() => {
                            let int_length = match to_int_or_inf(interp, v) {
                                Ok(n) => n,
                                Err(c) => return c,
                            };
                            int_length.max(0.0).min(len) as usize
                        }
                        _ => units.len(),
                    };
                    let end = start.saturating_add(result_len).min(units.len());
                    Completion::Normal(JsValue::string(JsString::from_vec(
                        units[start..end].to_vec(),
                    )))
                }),
                false,
            ));
            self.get_object_cell_expect(proto_id)
                .borrow_mut()
                .insert_builtin("substr".to_string(), fn_val);
        }

        // Aliases
        let trim_start_fn = self.get_property_on_id(proto_id, "trimStart");
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_builtin("trimLeft".to_string(), trim_start_fn);
        let trim_end_fn = self.get_property_on_id(proto_id, "trimEnd");
        self.get_object_cell_expect(proto_id)
            .borrow_mut()
            .insert_builtin("trimRight".to_string(), trim_end_fn);

        // String.prototype[@@iterator]
        let str_iter_fn = self.create_function(JsFunction::native(
            "[Symbol.iterator]".to_string(),
            0,
            |interp, this_val, _args| {
                let s = propagate!(this_js_string(interp, this_val));
                Completion::Normal(interp.create_string_iterator(s))
            },
        ));
        if let Some(key) = self.get_symbol_iterator_key() {
            self.get_object_cell_expect(proto_id)
                .borrow_mut()
                .insert_property(
                    key,
                    PropertyDescriptor::data(str_iter_fn, true, false, true),
                );
        }

        // Set String.prototype on the String constructor and wire constructor back
        if let Some(str_val) = self.get_global_var("String")
            && let Some(string_id) = str_val.as_object_id()
            && let Some(str_obj) = self.get_object_cell(string_id)
        {
            let proto_val = JsValue::object(proto_id);
            str_obj.borrow_mut().insert_property(
                "prototype".to_string(),
                PropertyDescriptor::data(proto_val.clone(), false, false, false),
            );
            self.get_object_cell_expect(proto_id)
                .borrow_mut()
                .insert_builtin("constructor".to_string(), str_val.clone());
        }

        self.realm_mut().string_prototype = Some(proto_id);
    }
}
