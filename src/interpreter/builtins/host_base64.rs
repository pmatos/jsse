use super::super::*;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn require_argument(interp: &mut Interpreter, name: &str, args: &[JsValue]) -> Result<(), JsValue> {
    if args.is_empty() {
        return Err(
            interp.create_type_error(&format!("{name}: 1 argument required, but only 0 present."))
        );
    }
    Ok(())
}

fn sextet(unit: u16) -> Option<u32> {
    let unit = u8::try_from(unit).ok()?;
    ALPHABET.iter().position(|&c| c == unit).map(|i| i as u32)
}

/// The HTML `btoa` algorithm over code units: every unit must fit in Latin-1.
fn encode_latin1(units: &[u16]) -> Option<String> {
    let bytes: Vec<u8> = units
        .iter()
        .map(|&u| u8::try_from(u).ok())
        .collect::<Option<_>>()?;
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
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

fn btoa(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    if let Err(e) = require_argument(interp, "btoa", args) {
        return Completion::Throw(e);
    }
    let input = match interp.to_js_string(&args[0]) {
        Ok(s) => s,
        Err(e) => return Completion::Throw(e),
    };
    match encode_latin1(input.as_units()) {
        Some(encoded) => Completion::Normal(JsValue::from_str(&encoded)),
        None => Completion::Throw(interp.create_dom_exception(
            "InvalidCharacterError",
            5,
            "Invalid character",
        )),
    }
}

fn atob(interp: &mut Interpreter, _this: &JsValue, args: &[JsValue]) -> Completion {
    if let Err(e) = require_argument(interp, "atob", args) {
        return Completion::Throw(e);
    }
    let input = match interp.to_js_string(&args[0]) {
        Ok(s) => s,
        Err(e) => return Completion::Throw(e),
    };
    match decode_forgiving(input.as_units()) {
        Some(decoded) => Completion::Normal(JsValue::string(JsString::from_vec(decoded))),
        None => Completion::Throw(interp.create_dom_exception(
            "InvalidCharacterError",
            5,
            "The string to be decoded is not correctly encoded.",
        )),
    }
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
