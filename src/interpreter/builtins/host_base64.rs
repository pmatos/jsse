use super::super::*;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn string_arg(interp: &mut Interpreter, name: &str, args: &[JsValue]) -> Result<JsString, JsValue> {
    let Some(arg) = args.first() else {
        return Err(
            interp.create_type_error(&format!("{name}: 1 argument required, but only 0 present."))
        );
    };
    interp.to_js_string(arg)
}

fn sextet(unit: u16) -> Option<u32> {
    match u8::try_from(unit).ok()? {
        c @ b'A'..=b'Z' => Some(u32::from(c - b'A')),
        c @ b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
        c @ b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// The HTML `btoa` algorithm over code units: every unit must fit in Latin-1.
fn encode_latin1(units: &[u16]) -> Option<Vec<u16>> {
    let mut out = Vec::with_capacity(units.len().div_ceil(3) * 4);
    for chunk in units.chunks(3) {
        let mut n = 0u32;
        for &unit in chunk {
            n = (n << 8) | u32::from(u8::try_from(unit).ok()?);
        }
        n <<= 8 * (3 - chunk.len() as u32);
        for i in 0..4 {
            out.push(if i <= chunk.len() {
                u16::from(ALPHABET[(n >> (18 - 6 * i)) as usize & 63])
            } else {
                u16::from(b'=')
            });
        }
    }
    Some(out)
}

/// The forgiving-base64 decode of the Infra Standard, yielding Latin-1 units.
fn decode_forgiving(units: &[u16]) -> Option<Vec<u16>> {
    let mut data: Vec<u16> = units
        .iter()
        .copied()
        .filter(|&u| !matches!(u, 0x09 | 0x0A | 0x0C | 0x0D | 0x20))
        .collect();
    if data.len().is_multiple_of(4) {
        for _ in 0..2 {
            if data.last() == Some(&u16::from(b'=')) {
                data.pop();
            }
        }
    }
    if data.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(data.len() * 3 / 4);
    for chunk in data.chunks(4) {
        let mut n = 0u32;
        for &unit in chunk {
            n = (n << 6) | sextet(unit)?;
        }
        n <<= 6 * (4 - chunk.len() as u32);
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        let produced = chunk.len() * 6 / 8;
        out.extend(bytes[..produced].iter().map(|&b| u16::from(b)));
    }
    Some(out)
}

fn invalid_character(interp: &mut Interpreter, message: &str) -> JsValue {
    interp.create_dom_exception("InvalidCharacterError", 5, message)
}

fn btoa_impl(interp: &mut Interpreter, args: &[JsValue]) -> Result<JsValue, JsValue> {
    let input = string_arg(interp, "btoa", args)?;
    match encode_latin1(input.as_units()) {
        Some(encoded) => Ok(JsValue::string(JsString::from_vec(encoded))),
        None => Err(invalid_character(interp, "Invalid character")),
    }
}

fn atob_impl(interp: &mut Interpreter, args: &[JsValue]) -> Result<JsValue, JsValue> {
    let input = string_arg(interp, "atob", args)?;
    match decode_forgiving(input.as_units()) {
        Some(decoded) => Ok(JsValue::string(JsString::from_vec(decoded))),
        None => Err(invalid_character(
            interp,
            "The string to be decoded is not correctly encoded.",
        )),
    }
}

fn btoa(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    btoa_impl(interp, args).map_or_else(Completion::Throw, Completion::Normal)
}

fn atob(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    atob_impl(interp, args).map_or_else(Completion::Throw, Completion::Normal)
}

impl Interpreter {
    pub(super) fn setup_base64_globals(&mut self) {
        self.register_global_fn(
            "btoa",
            BindingKind::Var,
            JsFunction::native("btoa".to_string(), 1, btoa),
        );
        self.register_global_fn(
            "atob",
            BindingKind::Var,
            JsFunction::native("atob".to_string(), 1, atob),
        );
    }
}
