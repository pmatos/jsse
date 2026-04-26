use super::super::*;

fn is_syntax_character(c: char) -> bool {
    matches!(
        c,
        '^' | '$' | '\\' | '.' | '*' | '+' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '|'
    )
}

fn is_whitespace_or_line_terminator(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\x0B' | '\x0C' | '\r' | ' ' | '\u{00A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

fn encode_for_regexp_escape(c: char) -> String {
    // Check for PUA-mapped surrogates first
    if let Some(surrogate) = pua_to_surrogate(c) {
        return format!("\\u{:04x}", surrogate);
    }
    if is_syntax_character(c) || c == '/' {
        return format!("\\{}", c);
    }
    match c {
        '\t' => "\\t".to_string(),
        '\n' => "\\n".to_string(),
        '\x0B' => "\\v".to_string(),
        '\x0C' => "\\f".to_string(),
        '\r' => "\\r".to_string(),
        _ => {
            let cp = c as u32;
            // ASCII non-alphanumeric non-underscore printable chars
            if (0x20..=0x7E).contains(&cp) && !c.is_ascii_alphanumeric() && c != '_' {
                return format!("\\x{:02x}", cp);
            }
            // Unicode whitespace/line terminators
            if is_whitespace_or_line_terminator(c)
                && !matches!(c, '\t' | '\n' | '\x0B' | '\x0C' | '\r')
            {
                if cp <= 0xFF {
                    return format!("\\x{:02x}", cp);
                }
                return format!("\\u{:04x}", cp);
            }
            c.to_string()
        }
    }
}

/// Convert a byte offset in a UTF-8 string to a UTF-16 code unit offset
fn byte_offset_to_utf16(s: &str, byte_offset: usize) -> usize {
    let mut utf16_offset = 0;
    for c in s[..byte_offset].chars() {
        // PUA characters that map to surrogates count as 1 UTF-16 code unit
        if pua_to_surrogate(c).is_some() {
            utf16_offset += 1;
        } else {
            utf16_offset += c.len_utf16();
        }
    }
    utf16_offset
}

/// Convert a UTF-16 code unit offset to a byte offset in a UTF-8 string
fn utf16_to_byte_offset(s: &str, utf16_offset: usize) -> usize {
    let mut utf16_count = 0;
    for (byte_idx, c) in s.char_indices() {
        if utf16_count >= utf16_offset {
            return byte_idx;
        }
        // PUA characters that map to surrogates count as 1 UTF-16 code unit
        if pua_to_surrogate(c).is_some() {
            utf16_count += 1;
        } else {
            utf16_count += c.len_utf16();
        }
    }
    s.len()
}

fn to_uint32_f64(n: f64) -> u32 {
    if n.is_nan() || n.is_infinite() || n == 0.0 {
        return 0;
    }
    let int_val = n.signum() * n.abs().floor();
    // Modulo 2^32
    let modulo = int_val % 4294967296.0;
    let modulo = if modulo < 0.0 {
        modulo + 4294967296.0
    } else {
        modulo
    };
    modulo as u32
}

// Surrogate code points (U+D800-U+DFFF) can't be represented as Rust chars.
// We remap them to Supplementary PUA-A (U+F0000+) so regex matching works
// on strings containing lone surrogates.
const SURROGATE_START: u32 = 0xD800;
const SURROGATE_END: u32 = 0xDFFF;
const SURROGATE_PUA_BASE: u32 = 0xF0000;

fn pua_aware_utf16_len(s: &str) -> usize {
    s.chars()
        .map(|c| {
            if pua_to_surrogate(c).is_some() {
                1
            } else {
                c.len_utf16()
            }
        })
        .sum()
}

fn is_surrogate(cp: u32) -> bool {
    (SURROGATE_START..=SURROGATE_END).contains(&cp)
}

fn surrogate_to_pua(cp: u32) -> char {
    char::from_u32(SURROGATE_PUA_BASE + (cp - SURROGATE_START)).unwrap()
}

fn pua_to_surrogate(c: char) -> Option<u16> {
    let cp = c as u32;
    if (SURROGATE_PUA_BASE..=SURROGATE_PUA_BASE + (SURROGATE_END - SURROGATE_START)).contains(&cp) {
        Some((cp - SURROGATE_PUA_BASE + SURROGATE_START) as u16)
    } else {
        None
    }
}

/// Convert a JsString (UTF-16) to a Rust String for regex matching.
/// Lone surrogates are mapped to PUA characters so they survive the conversion
/// and can be matched by patterns that also use the PUA mapping.
pub(crate) fn js_string_to_regex_input(code_units: &[u16]) -> String {
    js_string_to_regex_input_mode(code_units, true)
}

fn js_string_to_regex_input_non_unicode(code_units: &[u16]) -> String {
    js_string_to_regex_input_mode(code_units, false)
}

fn js_string_to_regex_input_mode(code_units: &[u16], unicode: bool) -> String {
    let mut result = String::new();
    let mut i = 0;
    while i < code_units.len() {
        let cu = code_units[i];
        if (0xD800..=0xDBFF).contains(&cu) {
            if unicode && i + 1 < code_units.len() && (0xDC00..=0xDFFF).contains(&code_units[i + 1])
            {
                let cp =
                    ((cu as u32 - 0xD800) << 10) + (code_units[i + 1] as u32 - 0xDC00) + 0x10000;
                if let Some(c) = char::from_u32(cp) {
                    result.push(c);
                }
                i += 2;
            } else {
                result.push(surrogate_to_pua(cu as u32));
                i += 1;
            }
        } else if (0xDC00..=0xDFFF).contains(&cu) {
            result.push(surrogate_to_pua(cu as u32));
            i += 1;
        } else if let Some(c) = char::from_u32(cu as u32) {
            result.push(c);
            i += 1;
        } else {
            i += 1;
        }
    }
    result
}

/// Convert a regex match result string back from PUA to surrogates.
pub(crate) fn regex_output_to_js_string(s: &str) -> JsString {
    let mut code_units: Vec<u16> = Vec::new();
    for c in s.chars() {
        if let Some(surrogate_cu) = pua_to_surrogate(c) {
            code_units.push(surrogate_cu);
        } else {
            let mut buf = [0u16; 2];
            let encoded = c.encode_utf16(&mut buf);
            code_units.extend_from_slice(encoded);
        }
    }
    JsString { code_units }
}

/// Convert UTF-16 code units that may contain PUA-encoded surrogates back to
/// actual surrogate code units. PUA chars U+F0000-U+F07FF encode as UTF-16
/// pairs [0xDB80..=0xDB81, 0xDC00..=0xDFFF]; these map back to U+D800-U+DFFF.
pub(crate) fn pua_code_units_to_surrogates(code_units: &[u16]) -> Vec<u16> {
    let mut result = Vec::with_capacity(code_units.len());
    let mut i = 0;
    while i < code_units.len() {
        let cu = code_units[i];
        if (0xDB80..=0xDB81).contains(&cu)
            && i + 1 < code_units.len()
            && (0xDC00..=0xDFFF).contains(&code_units[i + 1])
        {
            let cp = ((cu as u32 - 0xD800) << 10) + (code_units[i + 1] as u32 - 0xDC00) + 0x10000;
            if (SURROGATE_PUA_BASE..=SURROGATE_PUA_BASE + (SURROGATE_END - SURROGATE_START))
                .contains(&cp)
            {
                let surrogate = (cp - SURROGATE_PUA_BASE + SURROGATE_START) as u16;
                result.push(surrogate);
                i += 2;
                continue;
            }
        }
        result.push(cu);
        i += 1;
    }
    result
}

/// Encode UTF-16 code units as WTF-8 bytes.
/// Surrogate pairs → 4-byte UTF-8; lone surrogates → 3-byte WTF-8 (ED xx xx);
/// BMP → standard 1-3 byte UTF-8.
fn js_code_units_to_wtf8(code_units: &[u16]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(code_units.len() * 3);
    let mut i = 0;
    while i < code_units.len() {
        let cu = code_units[i];
        if (0xD800..=0xDBFF).contains(&cu)
            && i + 1 < code_units.len()
            && (0xDC00..=0xDFFF).contains(&code_units[i + 1])
        {
            let cp = ((cu as u32 - 0xD800) << 10) + (code_units[i + 1] as u32 - 0xDC00) + 0x10000;
            bytes.push((0xF0 | (cp >> 18)) as u8);
            bytes.push((0x80 | ((cp >> 12) & 0x3F)) as u8);
            bytes.push((0x80 | ((cp >> 6) & 0x3F)) as u8);
            bytes.push((0x80 | (cp & 0x3F)) as u8);
            i += 2;
        } else if (0xD800..=0xDFFF).contains(&cu) {
            let cp = cu as u32;
            bytes.push(0xED);
            bytes.push(((cp >> 6) & 0x3F) as u8 | 0x80);
            bytes.push((cp & 0x3F) as u8 | 0x80);
            i += 1;
        } else if cu < 0x80 {
            bytes.push(cu as u8);
            i += 1;
        } else if cu < 0x800 {
            bytes.push((0xC0 | (cu >> 6)) as u8);
            bytes.push((0x80 | (cu & 0x3F)) as u8);
            i += 1;
        } else {
            bytes.push((0xE0 | (cu >> 12)) as u8);
            bytes.push((0x80 | ((cu >> 6) & 0x3F)) as u8);
            bytes.push((0x80 | (cu & 0x3F)) as u8);
            i += 1;
        }
    }
    bytes
}

/// Convert a WTF-8 byte offset to a UTF-16 code unit offset.
fn wtf8_byte_offset_to_utf16(bytes: &[u8], target_byte: usize) -> usize {
    let mut utf16 = 0;
    let mut i = 0;
    while i < target_byte && i < bytes.len() {
        let b = bytes[i];
        if b < 0x80 {
            utf16 += 1;
            i += 1;
        } else if b < 0xC0 {
            i += 1;
        } else if b < 0xE0 {
            utf16 += 1;
            i += 2;
        } else if b < 0xF0 {
            // 3-byte: BMP char or WTF-8 surrogate — both are 1 UTF-16 unit
            utf16 += 1;
            i += 3;
        } else {
            utf16 += 2;
            i += 4;
        }
    }
    utf16
}

/// Convert a UTF-16 code unit offset to a WTF-8 byte offset.
fn utf16_to_wtf8_byte_offset(bytes: &[u8], target_utf16: usize) -> usize {
    let mut utf16 = 0;
    let mut i = 0;
    while i < bytes.len() && utf16 < target_utf16 {
        let b = bytes[i];
        if b < 0x80 {
            utf16 += 1;
            i += 1;
        } else if b < 0xC0 {
            i += 1;
        } else if b < 0xE0 {
            utf16 += 1;
            i += 2;
        } else if b < 0xF0 {
            // 3-byte: BMP char or WTF-8 surrogate — both are 1 UTF-16 unit
            utf16 += 1;
            i += 3;
        } else {
            utf16 += 2;
            i += 4;
        }
    }
    i
}

/// Convert a WTF-8 byte slice to a PUA-encoded string.
/// WTF-8 surrogates (ED [A0-BF] xx) become PUA chars; everything else stays as UTF-8.
fn wtf8_slice_to_pua_string(bytes: &[u8]) -> String {
    let mut result = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b < 0x80 {
            result.push(b as char);
            i += 1;
        } else if b < 0xC0 {
            i += 1;
        } else if b < 0xE0 {
            if i + 1 < bytes.len() {
                let cp = ((b as u32 & 0x1F) << 6) | (bytes[i + 1] as u32 & 0x3F);
                if let Some(c) = char::from_u32(cp) {
                    result.push(c);
                }
            }
            i += 2;
        } else if b == 0xED && i + 2 < bytes.len() && bytes[i + 1] >= 0xA0 {
            let cp = ((b as u32 & 0x0F) << 12)
                | ((bytes[i + 1] as u32 & 0x3F) << 6)
                | (bytes[i + 2] as u32 & 0x3F);
            result.push(surrogate_to_pua(cp));
            i += 3;
        } else if b < 0xF0 {
            if i + 2 < bytes.len() {
                let cp = ((b as u32 & 0x0F) << 12)
                    | ((bytes[i + 1] as u32 & 0x3F) << 6)
                    | (bytes[i + 2] as u32 & 0x3F);
                if let Some(c) = char::from_u32(cp) {
                    result.push(c);
                }
            }
            i += 3;
        } else {
            if i + 3 < bytes.len() {
                let cp = ((b as u32 & 0x07) << 18)
                    | ((bytes[i + 1] as u32 & 0x3F) << 12)
                    | ((bytes[i + 2] as u32 & 0x3F) << 6)
                    | (bytes[i + 3] as u32 & 0x3F);
                if let Some(c) = char::from_u32(cp) {
                    result.push(c);
                }
            }
            i += 4;
        }
    }
    result
}

fn is_cs_property(content: &str) -> bool {
    if let Some(eq_pos) = content.find('=') {
        let prop = &content[..eq_pos];
        let val = &content[eq_pos + 1..];
        matches!(prop, "General_Category" | "gc") && matches!(val, "Cs" | "Surrogate")
    } else {
        matches!(content, "Cs" | "Surrogate")
    }
}

fn is_co_property(content: &str) -> bool {
    if let Some(eq_pos) = content.find('=') {
        let prop = &content[..eq_pos];
        let val = &content[eq_pos + 1..];
        matches!(prop, "General_Category" | "gc") && matches!(val, "Co" | "Private_Use")
    } else {
        matches!(content, "Co" | "Private_Use")
    }
}

fn to_regex_input_with_units(
    interp: &mut Interpreter,
    val: &JsValue,
) -> Result<(String, Vec<u16>), JsValue> {
    match val {
        JsValue::String(s) => Ok((
            js_string_to_regex_input(&s.code_units),
            s.code_units.clone(),
        )),
        _ => {
            let s = interp.to_string_value(val)?;
            let js = JsString::from_str(&s);
            Ok((js_string_to_regex_input(&js.code_units), js.code_units))
        }
    }
}

fn escape_regexp_pattern_code_units(code_units: &[u16]) -> JsString {
    if code_units.is_empty() {
        return JsString::from_str("(?:)");
    }
    let mut result: Vec<u16> = Vec::with_capacity(code_units.len());
    let mut i = 0;
    let len = code_units.len();
    let mut in_char_class = false;
    while i < len {
        let cu = code_units[i];
        match cu {
            0x005C => {
                result.push(cu);
                i += 1;
                if i < len {
                    let next = code_units[i];
                    match next {
                        0x000A => result.extend_from_slice(&[0x006E]),
                        0x000D => result.extend_from_slice(&[0x0072]),
                        0x2028 => {
                            result.extend_from_slice(&[0x0075, 0x0032, 0x0030, 0x0032, 0x0038])
                        }
                        0x2029 => {
                            result.extend_from_slice(&[0x0075, 0x0032, 0x0030, 0x0032, 0x0039])
                        }
                        _ => result.push(next),
                    }
                    i += 1;
                }
            }
            0x005B if !in_char_class => {
                in_char_class = true;
                result.push(cu);
                i += 1;
            }
            0x005D if in_char_class => {
                in_char_class = false;
                result.push(cu);
                i += 1;
            }
            0x002F if !in_char_class => {
                result.extend_from_slice(&[0x005C, 0x002F]);
                i += 1;
            }
            0x000A => {
                result.extend_from_slice(&[0x005C, 0x006E]);
                i += 1;
            }
            0x000D => {
                result.extend_from_slice(&[0x005C, 0x0072]);
                i += 1;
            }
            0x2028 => {
                result.extend_from_slice(&[0x005C, 0x0075, 0x0032, 0x0030, 0x0032, 0x0038]);
                i += 1;
            }
            0x2029 => {
                result.extend_from_slice(&[0x005C, 0x0075, 0x0032, 0x0030, 0x0032, 0x0039]);
                i += 1;
            }
            _ => {
                result.push(cu);
                i += 1;
            }
        }
    }
    JsString { code_units: result }
}

// ECMAScript binary Unicode properties (with aliases) — §Table 67
const VALID_BINARY_PROPERTIES: &[&str] = &[
    "ASCII",
    "ASCII_Hex_Digit",
    "AHex",
    "Alphabetic",
    "Alpha",
    "Any",
    "Assigned",
    "Bidi_Control",
    "Bidi_C",
    "Bidi_Mirrored",
    "Bidi_M",
    "Case_Ignorable",
    "CI",
    "Cased",
    "Changes_When_Casefolded",
    "CWCF",
    "Changes_When_Casemapped",
    "CWCM",
    "Changes_When_Lowercased",
    "CWL",
    "Changes_When_NFKC_Casefolded",
    "CWKCF",
    "Changes_When_Titlecased",
    "CWT",
    "Changes_When_Uppercased",
    "CWU",
    "Dash",
    "Default_Ignorable_Code_Point",
    "DI",
    "Deprecated",
    "Dep",
    "Diacritic",
    "Dia",
    "Emoji",
    "Emoji_Component",
    "EComp",
    "Emoji_Modifier",
    "EMod",
    "Emoji_Modifier_Base",
    "EBase",
    "Emoji_Presentation",
    "EPres",
    "Extended_Pictographic",
    "ExtPict",
    "Extender",
    "Ext",
    "Grapheme_Base",
    "Gr_Base",
    "Grapheme_Extend",
    "Gr_Ext",
    "Hex_Digit",
    "Hex",
    "IDS_Binary_Operator",
    "IDSB",
    "IDS_Trinary_Operator",
    "IDST",
    "IDS_Unary_Operator",
    "IDSU",
    "ID_Continue",
    "IDC",
    "ID_Start",
    "IDS",
    "Ideographic",
    "Ideo",
    "Join_Control",
    "Join_C",
    "Logical_Order_Exception",
    "LOE",
    "Lowercase",
    "Lower",
    "Math",
    "Noncharacter_Code_Point",
    "NChar",
    "Pattern_Syntax",
    "Pat_Syn",
    "Pattern_White_Space",
    "Pat_WS",
    "Quotation_Mark",
    "QMark",
    "Radical",
    "Regional_Indicator",
    "RI",
    "Sentence_Terminal",
    "STerm",
    "Soft_Dotted",
    "SD",
    "Terminal_Punctuation",
    "Term",
    "Unified_Ideograph",
    "UIdeo",
    "Uppercase",
    "Upper",
    "Variation_Selector",
    "VS",
    "White_Space",
    "space",
    "WSpace",
    "XID_Continue",
    "XIDC",
    "XID_Start",
    "XIDS",
    // ES2024+ Sequence properties (only valid in `v` flag mode but fancy_regex won't see them)
    "Basic_Emoji",
    "Emoji_Keycap_Sequence",
    "RGI_Emoji_Flag_Sequence",
    "RGI_Emoji_Modifier_Sequence",
    "RGI_Emoji_Tag_Sequence",
    "RGI_Emoji_ZWJ_Sequence",
    "RGI_Emoji",
];

// Non-binary properties (require =value)
const NONBINARY_PROPERTIES: &[&str] = &[
    "General_Category",
    "gc",
    "Script",
    "sc",
    "Script_Extensions",
    "scx",
];

// Valid General_Category values (long names, short names, aliases)
const VALID_GC_VALUES: &[&str] = &[
    "Cased_Letter",
    "LC",
    "Close_Punctuation",
    "Pe",
    "Connector_Punctuation",
    "Pc",
    "Control",
    "Cc",
    "cntrl",
    "Currency_Symbol",
    "Sc",
    "Dash_Punctuation",
    "Pd",
    "Decimal_Number",
    "Nd",
    "digit",
    "Enclosing_Mark",
    "Me",
    "Final_Punctuation",
    "Pf",
    "Format",
    "Cf",
    "Initial_Punctuation",
    "Pi",
    "Letter",
    "L",
    "Letter_Number",
    "Nl",
    "Line_Separator",
    "Zl",
    "Lowercase_Letter",
    "Ll",
    "Mark",
    "M",
    "Combining_Mark",
    "Math_Symbol",
    "Sm",
    "Modifier_Letter",
    "Lm",
    "Modifier_Symbol",
    "Sk",
    "Nonspacing_Mark",
    "Mn",
    "Number",
    "N",
    "Open_Punctuation",
    "Ps",
    "Other",
    "C",
    "Other_Letter",
    "Lo",
    "Other_Number",
    "No",
    "Other_Punctuation",
    "Po",
    "Other_Symbol",
    "So",
    "Paragraph_Separator",
    "Zp",
    "Private_Use",
    "Co",
    "Punctuation",
    "P",
    "punct",
    "Separator",
    "Z",
    "Space_Separator",
    "Zs",
    "Spacing_Mark",
    "Mc",
    "Surrogate",
    "Cs",
    "Symbol",
    "S",
    "Titlecase_Letter",
    "Lt",
    "Unassigned",
    "Cn",
    "Uppercase_Letter",
    "Lu",
];

fn validate_unicode_property_escape(content: &str) -> Result<(), String> {
    if content.is_empty() {
        return Err(format!("Invalid property escape: \\p{{{}}}", content));
    }
    // Reject spaces/loose matching
    if content.chars().any(|c| c == ' ' || c == '\t') {
        return Err(format!("Invalid property escape: \\p{{{}}}", content));
    }

    if let Some(eq_pos) = content.find('=') {
        // PropertyName=Value form
        let prop_name = &content[..eq_pos];
        let prop_value = &content[eq_pos + 1..];
        if prop_name.is_empty() || prop_value.is_empty() {
            return Err(format!("Invalid property escape: \\p{{{}}}", content));
        }
        // Must be a non-binary property
        if !NONBINARY_PROPERTIES.contains(&prop_name) {
            return Err(format!("Invalid property escape: \\p{{{}}}", content));
        }
        // For General_Category, validate the value
        if (prop_name == "General_Category" || prop_name == "gc")
            && !VALID_GC_VALUES.contains(&prop_value)
        {
            return Err(format!("Invalid property escape: \\p{{{}}}", content));
        }
        // For Script/Script_Extensions, let fancy_regex validate the value
        Ok(())
    } else {
        // Lone value: must be a valid binary property OR a valid GC value
        if VALID_BINARY_PROPERTIES.contains(&content) {
            return Ok(());
        }
        if VALID_GC_VALUES.contains(&content) {
            return Ok(());
        }
        // Reject non-binary properties used without a value
        if NONBINARY_PROPERTIES.contains(&content) {
            return Err(format!("Invalid property escape: \\p{{{}}}", content));
        }
        // Unknown property — reject
        Err(format!("Invalid property escape: \\p{{{}}}", content))
    }
}

/// A v-flag character class result: set of single codepoint ranges + multi-codepoint strings.
#[derive(Clone)]
struct VClassSet {
    ranges: Vec<(u32, u32)>,
    strings: Vec<String>,
}

impl VClassSet {
    fn new() -> Self {
        VClassSet {
            ranges: Vec::new(),
            strings: Vec::new(),
        }
    }

    fn add_codepoint(&mut self, cp: u32) {
        self.ranges.push((cp, cp));
    }

    fn add_range(&mut self, lo: u32, hi: u32) {
        self.ranges.push((lo, hi));
    }

    fn add_string(&mut self, s: String) {
        if s.chars().count() == 1 {
            self.ranges.push((
                s.chars().next().unwrap() as u32,
                s.chars().next().unwrap() as u32,
            ));
        } else {
            self.strings.push(s);
        }
    }

    fn normalize_ranges(&mut self) {
        if self.ranges.is_empty() {
            return;
        }
        self.ranges.sort();
        let mut merged = vec![self.ranges[0]];
        for &(lo, hi) in &self.ranges[1..] {
            let last = merged.last_mut().unwrap();
            if lo <= last.1 + 1 {
                last.1 = last.1.max(hi);
            } else {
                merged.push((lo, hi));
            }
        }
        self.ranges = merged;
    }

    fn union(&self, other: &VClassSet) -> VClassSet {
        let mut result = self.clone();
        result.ranges.extend_from_slice(&other.ranges);
        result.strings.extend(other.strings.iter().cloned());
        result.normalize_ranges();
        result.dedup_strings();
        result
    }

    fn intersect(&self, other: &VClassSet) -> VClassSet {
        let mut result = VClassSet::new();
        let mut a = self.clone();
        a.normalize_ranges();
        let mut b = other.clone();
        b.normalize_ranges();
        // Intersect codepoint ranges
        let (mut ai, mut bi) = (0, 0);
        while ai < a.ranges.len() && bi < b.ranges.len() {
            let (a_lo, a_hi) = a.ranges[ai];
            let (b_lo, b_hi) = b.ranges[bi];
            let lo = a_lo.max(b_lo);
            let hi = a_hi.min(b_hi);
            if lo <= hi {
                result.ranges.push((lo, hi));
            }
            if a_hi < b_hi {
                ai += 1;
            } else {
                bi += 1;
            }
        }
        // Intersect strings: keep strings that appear in both
        let b_strings: HashSet<&str> = b.strings.iter().map(|s| s.as_str()).collect();
        for s in &a.strings {
            if b_strings.contains(s.as_str()) {
                result.strings.push(s.clone());
            }
        }
        result
    }

    fn difference(&self, other: &VClassSet) -> VClassSet {
        let mut a = self.clone();
        a.normalize_ranges();
        let mut b = other.clone();
        b.normalize_ranges();
        // Subtract b's ranges from a's ranges
        let mut result_ranges: Vec<(u32, u32)> = Vec::new();
        for &(a_lo, a_hi) in &a.ranges {
            let mut lo = a_lo;
            for &(b_lo, b_hi) in &b.ranges {
                if b_hi < lo || b_lo > a_hi {
                    continue;
                }
                if b_lo > lo {
                    result_ranges.push((lo, b_lo - 1));
                }
                lo = b_hi + 1;
            }
            if lo <= a_hi {
                result_ranges.push((lo, a_hi));
            }
        }
        // Subtract b's strings from a's strings
        let b_strings: HashSet<&str> = b.strings.iter().map(|s| s.as_str()).collect();
        let result_strings: Vec<String> = a
            .strings
            .iter()
            .filter(|s| !b_strings.contains(s.as_str()))
            .cloned()
            .collect();
        VClassSet {
            ranges: result_ranges,
            strings: result_strings,
        }
    }

    fn dedup_strings(&mut self) {
        let mut seen = HashSet::default();
        self.strings.retain(|s| seen.insert(s.clone()));
    }

    fn complement(&self) -> VClassSet {
        let mut s = self.clone();
        s.normalize_ranges();
        VClassSet {
            ranges: complement_ranges(&s.ranges),
            strings: Vec::new(), // complement doesn't apply to strings
        }
    }

    fn to_regex_pattern(&self) -> String {
        let mut s = self.clone();
        s.normalize_ranges();
        s.dedup_strings();
        // Sort strings longest first for greedy matching
        s.strings.sort_by_key(|b| std::cmp::Reverse(b.len()));

        let has_ranges = !s.ranges.is_empty();
        let has_strings = !s.strings.is_empty();

        if !has_ranges && !has_strings {
            // Empty set: match nothing
            return "(?!)".to_string();
        }

        let mut char_class = String::new();
        if has_ranges {
            char_class.push('[');
            for &(lo, hi) in &s.ranges {
                append_unicode_range(&mut char_class, lo, hi);
            }
            char_class.push(']');
        }

        if has_strings && has_ranges {
            let mut parts = vec![char_class];
            for st in &s.strings {
                let mut escaped = String::new();
                for ch in st.chars() {
                    push_literal_char(&mut escaped, ch, false);
                }
                parts.push(escaped);
            }
            format!("(?:{})", parts.join("|"))
        } else if has_strings {
            let parts: Vec<String> = s
                .strings
                .iter()
                .map(|st| {
                    let mut escaped = String::new();
                    for ch in st.chars() {
                        push_literal_char(&mut escaped, ch, false);
                    }
                    escaped
                })
                .collect();
            if parts.len() == 1 {
                parts[0].clone()
            } else {
                format!("(?:{})", parts.join("|"))
            }
        } else {
            char_class
        }
    }
}

/// Parse a single class escape atom inside a v-flag character class, returning its codepoint value.
fn parse_v_class_escape(chars: &[char], i: &mut usize) -> Option<u32> {
    if *i >= chars.len() {
        return None;
    }
    let next = chars[*i];
    *i += 1;
    match next {
        'n' => Some('\n' as u32),
        'r' => Some('\r' as u32),
        't' => Some('\t' as u32),
        'f' => Some(0x0C),
        'v' => Some(0x0B),
        '0' => Some(0),
        'x' if *i + 1 < chars.len()
            && chars[*i].is_ascii_hexdigit()
            && chars[*i + 1].is_ascii_hexdigit() =>
        {
            let hex: String = chars[*i..*i + 2].iter().collect();
            *i += 2;
            u32::from_str_radix(&hex, 16).ok()
        }
        'u' => {
            if *i < chars.len() && chars[*i] == '{' {
                *i += 1;
                let start = *i;
                while *i < chars.len() && chars[*i] != '}' {
                    *i += 1;
                }
                if *i < chars.len() {
                    let hex: String = chars[start..*i].iter().collect();
                    *i += 1;
                    return u32::from_str_radix(&hex, 16).ok();
                }
                None
            } else if *i + 3 < chars.len()
                && chars[*i].is_ascii_hexdigit()
                && chars[*i + 1].is_ascii_hexdigit()
                && chars[*i + 2].is_ascii_hexdigit()
                && chars[*i + 3].is_ascii_hexdigit()
            {
                let hex: String = chars[*i..*i + 4].iter().collect();
                *i += 4;
                u32::from_str_radix(&hex, 16).ok()
            } else {
                None
            }
        }
        c if is_syntax_character(c) || c == '/' => Some(c as u32),
        _ => Some(next as u32),
    }
}

const V_FLAG_CLASS_MAX_DEPTH: usize = 256;

/// Parse a v-flag character class starting right after the opening `[`.
/// Returns (VClassSet, new_index_after_closing_bracket).
fn parse_v_flag_class(
    chars: &[char],
    start: usize,
    flags: &str,
) -> Result<(VClassSet, usize), String> {
    parse_v_flag_class_with_depth(chars, start, flags, 0)
}

fn parse_v_flag_class_with_depth(
    chars: &[char],
    start: usize,
    flags: &str,
    depth: usize,
) -> Result<(VClassSet, usize), String> {
    if depth >= V_FLAG_CLASS_MAX_DEPTH {
        return Err("v-flag character class nesting exceeds implementation limit".to_string());
    }

    let len = chars.len();
    let mut i = start;
    let negated = i < len && chars[i] == '^';
    if negated {
        i += 1;
    }

    // Parse the first operand
    let mut result = parse_v_class_operand(chars, &mut i, flags, depth)?;

    // Check for set operations
    while i < len && chars[i] != ']' {
        if i + 1 < len && chars[i] == '-' && chars[i + 1] == '-' {
            // Difference: --
            i += 2;
            let rhs = parse_v_class_operand(chars, &mut i, flags, depth)?;
            result = result.difference(&rhs);
        } else if i + 1 < len && chars[i] == '&' && chars[i + 1] == '&' {
            // Intersection: &&
            i += 2;
            let rhs = parse_v_class_operand(chars, &mut i, flags, depth)?;
            result = result.intersect(&rhs);
        } else if i < len && chars[i] == '[' {
            // Nested class in union position
            i += 1;
            let (set, new_i) = parse_v_flag_class_with_depth(chars, i, flags, depth + 1)?;
            i = new_i;
            result = result.union(&set);
        } else {
            // Union: implicit (just more atoms)
            let atom = parse_v_class_atom(chars, &mut i, flags)?;
            result = result.union(&atom);
        }
    }

    if i < len && chars[i] == ']' {
        i += 1; // consume closing ]
    }

    if negated {
        result = result.complement();
    }

    Ok((result, i))
}

/// Parse a class operand: either a nested [...] or a sequence of atoms (union).
fn parse_v_class_operand(
    chars: &[char],
    i: &mut usize,
    flags: &str,
    depth: usize,
) -> Result<VClassSet, String> {
    let len = chars.len();
    if *i < len && chars[*i] == '[' {
        // Nested character class
        *i += 1;
        let (set, new_i) = parse_v_flag_class_with_depth(chars, *i, flags, depth + 1)?;
        *i = new_i;
        return Ok(set);
    }

    // Parse atoms until we hit ], --, or &&
    let mut result = VClassSet::new();
    while *i < len {
        let c = chars[*i];
        if c == ']' {
            break;
        }
        // Check for -- or && (set operation boundary)
        if *i + 1 < len
            && ((c == '-' && chars[*i + 1] == '-') || (c == '&' && chars[*i + 1] == '&'))
        {
            break;
        }
        if c == '[' {
            // Nested class in union position
            *i += 1;
            let (set, new_i) = parse_v_flag_class_with_depth(chars, *i, flags, depth + 1)?;
            *i = new_i;
            result = result.union(&set);
            continue;
        }
        let atom = parse_v_class_atom(chars, i, flags)?;
        result = result.union(&atom);
    }
    Ok(result)
}

/// Parse a single atom in a v-flag character class. May return ranges or strings.
fn parse_v_class_atom(chars: &[char], i: &mut usize, flags: &str) -> Result<VClassSet, String> {
    let len = chars.len();
    let mut result = VClassSet::new();

    if *i >= len {
        return Ok(result);
    }

    let c = chars[*i];

    if c == '\\' && *i + 1 < len {
        let next = chars[*i + 1];
        match next {
            // Character class escapes
            'd' => {
                *i += 2;
                result.add_range('0' as u32, '9' as u32);
                return check_range(chars, i, result, flags);
            }
            'D' => {
                *i += 2;
                result.add_range(0, '0' as u32 - 1);
                result.add_range('9' as u32 + 1, 0xD7FF);
                result.add_range(0xE000, 0x10FFFF);
                return Ok(result);
            }
            'w' => {
                *i += 2;
                result.add_range('A' as u32, 'Z' as u32);
                result.add_range('a' as u32, 'z' as u32);
                result.add_range('0' as u32, '9' as u32);
                result.add_codepoint('_' as u32);
                return Ok(result);
            }
            'W' => {
                *i += 2;
                // Everything except [A-Za-z0-9_]
                result.add_range(0, '0' as u32 - 1);
                result.add_range('9' as u32 + 1, 'A' as u32 - 1);
                result.add_range('Z' as u32 + 1, '_' as u32 - 1);
                result.add_range('_' as u32 + 1, 'a' as u32 - 1);
                result.add_range('z' as u32 + 1, 0xD7FF);
                result.add_range(0xE000, 0x10FFFF);
                return Ok(result);
            }
            's' => {
                *i += 2;
                for &cp in &[
                    0x09u32, 0x0A, 0x0B, 0x0C, 0x0D, 0x20, 0xA0, 0x1680, 0x2028, 0x2029, 0x202F,
                    0x205F, 0x3000, 0xFEFF,
                ] {
                    result.add_codepoint(cp);
                }
                result.add_range(0x2000, 0x200A);
                return Ok(result);
            }
            'S' => {
                *i += 2;
                // Complement of \s — complex, approximate with full range minus whitespace
                let mut ws_set = VClassSet::new();
                for &cp in &[
                    0x09u32, 0x0A, 0x0B, 0x0C, 0x0D, 0x20, 0xA0, 0x1680, 0x2028, 0x2029, 0x202F,
                    0x205F, 0x3000, 0xFEFF,
                ] {
                    ws_set.add_codepoint(cp);
                }
                ws_set.add_range(0x2000, 0x200A);
                let all = VClassSet {
                    ranges: vec![(0, 0xD7FF), (0xE000, 0x10FFFF)],
                    strings: Vec::new(),
                };
                return Ok(all.difference(&ws_set));
            }
            // Property escape
            'p' | 'P' if *i + 2 < len && chars[*i + 2] == '{' => {
                let negated = next == 'P';
                let start = *i + 3;
                if let Some(end) = chars[start..].iter().position(|&ch| ch == '}') {
                    let content: String = chars[start..start + end].iter().collect();
                    *i = start + end + 1;
                    // Check if it's a property-of-strings
                    if let Some((singles, multi_strs)) =
                        crate::emoji_strings::lookup_string_property(&content)
                    {
                        if negated {
                            return Err(format!(
                                "Invalid property escape: \\P{{{}}} cannot negate a property of strings",
                                content
                            ));
                        }
                        for &cp in singles {
                            result.add_codepoint(cp);
                        }
                        for s in multi_strs {
                            result.add_string(s.to_string());
                        }
                        return Ok(result);
                    }
                    // Regular property
                    if let Some(ranges) = crate::unicode_tables::lookup_property(&content) {
                        let ranges_to_use = if negated {
                            complement_ranges(ranges)
                        } else {
                            ranges.to_vec()
                        };
                        for &(lo, hi) in &ranges_to_use {
                            result.add_range(lo, hi);
                        }
                    }
                    return check_range(chars, i, result, flags);
                }
                return Err("Invalid property escape: unterminated".to_string());
            }
            // \q{str1|str2|...} string literal
            'q' if *i + 2 < len && chars[*i + 2] == '{' => {
                let start = *i + 3;
                let mut j = start;
                let mut depth = 1;
                while j < len && depth > 0 {
                    if chars[j] == '{' {
                        depth += 1;
                    } else if chars[j] == '}' {
                        depth -= 1;
                    } else if chars[j] == '\\' && j + 1 < len {
                        j += 1;
                    }
                    if depth > 0 {
                        j += 1;
                    }
                }
                let content: String = chars[start..j].iter().collect();
                *i = j + 1; // skip closing }
                // Split on | and add each alternative
                for alt in content.split('|') {
                    let s = unescape_q_string(alt);
                    result.add_string(s);
                }
                return Ok(result);
            }
            _ => {
                // Regular escape
                *i += 1; // skip backslash
                if let Some(cp) = parse_v_class_escape(chars, i) {
                    // If lead surrogate, try to combine with following \uDC00-\uDFFF
                    let final_cp = if (0xD800..=0xDBFF).contains(&cp)
                        && *i + 1 < chars.len()
                        && chars[*i] == '\\'
                        && chars[*i + 1] == 'u'
                    {
                        let saved = *i;
                        *i += 1; // skip backslash only; parse_v_class_escape reads from after it
                        if let Some(trail) = parse_v_class_escape(chars, i)
                            && (0xDC00..=0xDFFF).contains(&trail)
                        {
                            // Combine surrogate pair into Unicode code point
                            ((cp - 0xD800) << 10) + (trail - 0xDC00) + 0x10000
                        } else {
                            *i = saved;
                            cp
                        }
                    } else {
                        cp
                    };
                    result.add_codepoint(final_cp);
                    return check_range(chars, i, result, flags);
                }
                return Ok(result);
            }
        }
    }

    // Regular character (not a backslash escape)
    *i += 1;
    let cp = c as u32;
    result.add_codepoint(cp);
    check_range(chars, i, result, flags)
}

/// After parsing a single codepoint atom, check if it's followed by `-` for a range.
fn check_range(
    chars: &[char],
    i: &mut usize,
    mut result: VClassSet,
    _flags: &str,
) -> Result<VClassSet, String> {
    let len = chars.len();
    // Check for range: cp-cp (but not --)
    if *i < len && chars[*i] == '-' && *i + 1 < len && chars[*i + 1] != '-' && chars[*i + 1] != ']'
    {
        *i += 1; // skip -
        // Parse the end of the range
        let start_cp = result.ranges.last().map(|r| r.1);
        let end_cp = if *i < len && chars[*i] == '\\' && *i + 1 < len {
            *i += 1; // skip backslash
            parse_v_class_escape(chars, i)
        } else if *i < len {
            let c = chars[*i];
            *i += 1;
            Some(c as u32)
        } else {
            None
        };
        if let (Some(lo), Some(hi)) = (start_cp, end_cp) {
            // Replace last single-codepoint entry with the range
            result.ranges.pop();
            result.add_range(lo, hi);
        }
    }
    Ok(result)
}

/// Unescape a \q{...} string content (handles \uHHHH, \u{HHHH}, \xHH, etc.)
fn unescape_q_string(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut result = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            i += 1;
            if let Some(cp) = parse_v_class_escape(&chars, &mut i)
                && let Some(ch) = char::from_u32(cp)
            {
                result.push(ch);
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

fn decode_group_name_raw(chars: &[char]) -> String {
    let mut result = String::new();
    let len = chars.len();
    let mut i = 0;
    while i < len {
        if chars[i] == '\\' && i + 1 < len && chars[i + 1] == 'u' {
            i += 2; // skip \u
            if i < len && chars[i] == '{' {
                i += 1; // skip {
                let hex_start = i;
                while i < len && chars[i] != '}' {
                    i += 1;
                }
                let hex: String = chars[hex_start..i].iter().collect();
                i += 1; // skip }
                if let Ok(cp) = u32::from_str_radix(&hex, 16)
                    && let Some(c) = char::from_u32(cp)
                {
                    result.push(c);
                }
            } else if i + 4 <= len {
                let hex: String = chars[i..i + 4].iter().collect();
                if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                    i += 4;
                    if (0xD800..=0xDBFF).contains(&cp)
                        && i + 5 < len
                        && chars[i] == '\\'
                        && chars[i + 1] == 'u'
                    {
                        let trail_hex: String = chars[i + 2..i + 6].iter().collect();
                        if let Ok(trail) = u32::from_str_radix(&trail_hex, 16)
                            && (0xDC00..=0xDFFF).contains(&trail)
                        {
                            let combined = 0x10000 + ((cp - 0xD800) << 10) + (trail - 0xDC00);
                            if let Some(c) = char::from_u32(combined) {
                                result.push(c);
                            }
                            i += 6; // skip \uXXXX trail surrogate
                            continue;
                        }
                    }
                    if let Some(c) = char::from_u32(cp) {
                        result.push(c);
                    }
                } else {
                    i += 4;
                }
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

fn sanitize_group_name(name: &str) -> String {
    let mut result = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            result.push(c);
        } else {
            result.push_str(&format!("_u{:04X}_", c as u32));
        }
    }
    if result.starts_with(|c: char| c.is_ascii_digit()) {
        result.insert(0, '_');
    }
    result
}

pub(super) struct TranslationResult {
    pub(super) pattern: String,
    dup_group_map: HashMap<String, Vec<(String, u32)>>,
    group_name_order: Vec<String>,
    needs_bytes_mode: bool,
}

/// Parse a quantifier starting at `pos` in `chars` and return
/// (min_is_zero, end_pos) where end_pos is the index after the full quantifier
/// (including optional trailing `?` for lazy).
fn parse_quantifier_min(chars: &[char], pos: usize, len: usize) -> (bool, usize) {
    let c = chars[pos];
    let mut end = pos + 1;
    let min_is_zero = match c {
        '*' => true,
        '+' => false,
        '?' => true,
        '{' => {
            let mut j = pos + 1;
            let num_start = j;
            while j < len && chars[j].is_ascii_digit() {
                j += 1;
            }
            if j > num_start && j < len && (chars[j] == '}' || chars[j] == ',') {
                let num_str: String = chars[num_start..j].iter().collect();
                let min_val: u32 = num_str.parse().unwrap_or(1);
                while j < len && chars[j] != '}' {
                    j += 1;
                }
                if j < len {
                    j += 1;
                }
                end = j;
                min_val == 0
            } else {
                end = pos + 1;
                false
            }
        }
        _ => {
            return (false, pos + 1);
        }
    };
    if c != '{' {
        end = pos + 1;
    }
    if end < len && chars[end] == '?' {
        end += 1;
    }
    (min_is_zero, end)
}

/// Find the start of a lookahead assertion `(?=` or `(?!` in the result string
/// by scanning backwards from the end, tracking nested parentheses.
fn find_lookahead_start_in_result(result: &str) -> Option<usize> {
    let bytes = result.as_bytes();
    let mut depth = 0;
    let mut i = bytes.len();
    while i > 0 {
        i -= 1;
        if bytes[i] == b')' {
            depth += 1;
        } else if bytes[i] == b'(' {
            if depth > 0 {
                depth -= 1;
            } else {
                if i + 2 < bytes.len()
                    && bytes[i + 1] == b'?'
                    && (bytes[i + 2] == b'=' || bytes[i + 2] == b'!')
                {
                    return Some(i);
                }
                return None;
            }
        }
    }
    None
}

fn translate_js_pattern(source: &str, flags: &str) -> Result<String, String> {
    translate_js_pattern_ex(source, flags).map(|r| r.pattern)
}

/// Find the closing ')' matching the '(' at position `open` in `chars`.
/// Returns None if not found.
fn find_matching_close_paren(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = open;
    let len = chars.len();
    let mut in_cc = false;
    while i < len {
        match chars[i] {
            '\\' if i + 1 < len => {
                i += 2;
                continue;
            }
            '[' if !in_cc => in_cc = true,
            ']' if in_cc => in_cc = false,
            '(' if !in_cc => depth += 1,
            ')' if !in_cc => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Check if `body` (JS pattern fragment) contains a `\k<name>` reference to any
/// name in `names`. Used to decide if it's safe to strip named groups from a copy.
fn body_has_named_backref_to(chars: &[char], names: &HashSet<String>) -> bool {
    let len = chars.len();
    let mut i = 0;
    let mut in_cc = false;
    while i < len {
        match chars[i] {
            '[' if !in_cc => in_cc = true,
            ']' if in_cc => in_cc = false,
            '\\' if !in_cc && i + 1 < len && chars[i + 1] == 'k' => {
                if i + 2 < len && chars[i + 2] == '<' {
                    let start = i + 3;
                    if let Some(end_off) = chars[start..].iter().position(|&c| c == '>') {
                        let name: String = chars[start..start + end_off].iter().collect();
                        if names.contains(&name) {
                            return true;
                        }
                    }
                }
                i += 2;
                continue;
            }
            '\\' if i + 1 < len => {
                i += 2;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    false
}

/// Check if `body` contains any named capturing group whose name is in `names`.
fn body_has_dup_named_group(chars: &[char], names: &HashSet<String>) -> bool {
    let len = chars.len();
    let mut i = 0;
    let mut in_cc = false;
    while i < len {
        match chars[i] {
            '[' if !in_cc => in_cc = true,
            ']' if in_cc => in_cc = false,
            '\\' if i + 1 < len => {
                i += 2;
                continue;
            }
            '(' if !in_cc
                && i + 2 < len
                && chars[i + 1] == '?'
                && chars[i + 2] == '<'
                && i + 3 < len
                && chars[i + 3] != '='
                && chars[i + 3] != '!' =>
            {
                let name_start = i + 3;
                if let Some(end_off) = chars[name_start..].iter().position(|&c| c == '>') {
                    let name: String = chars[name_start..name_start + end_off].iter().collect();
                    if names.contains(&name) {
                        return true;
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    false
}

/// Strip named groups from a JS pattern fragment: replace `(?<name>` with `(?:` and
/// `(?<name>` (capture) with non-capturing `(?:`. This strips ALL capturing group names
/// AND converts plain capturing groups `(` to non-capturing `(?:` too, so that the
/// group count in the source is preserved only in the last copy.
///
/// Actually: we only strip NAMED groups (replace `(?<name>` with `(`). Plain capturing
/// groups stay as capturing so the group numbering is maintained by the translator.
fn anonymize_named_groups(chars: &[char]) -> String {
    let len = chars.len();
    let mut result = String::new();
    let mut i = 0;
    let mut in_cc = false;
    while i < len {
        match chars[i] {
            '[' if !in_cc => {
                in_cc = true;
                result.push('[');
            }
            ']' if in_cc => {
                in_cc = false;
                result.push(']');
            }
            '\\' if i + 1 < len => {
                result.push('\\');
                result.push(chars[i + 1]);
                i += 2;
                continue;
            }
            '(' if !in_cc
                && i + 2 < len
                && chars[i + 1] == '?'
                && chars[i + 2] == '<'
                && i + 3 < len
                && chars[i + 3] != '='
                && chars[i + 3] != '!' =>
            {
                // Named group: (?<name>... → (?:...
                // Skip past the name
                let name_start = i + 3;
                if let Some(end_off) = chars[name_start..].iter().position(|&c| c == '>') {
                    result.push_str("(?:");
                    i = name_start + end_off + 1; // skip past '>'
                    continue;
                } else {
                    result.push(chars[i]);
                }
            }
            c => result.push(c),
        }
        i += 1;
    }
    result
}

/// Rename named groups and backreferences in a pattern body for a specific iteration.
/// `(?<name>...)` → `(?<__jsse_qi{idx}__name>...)` and `\k<name>` → `\k<__jsse_qi{idx}__name>`
/// for names in the duplicate set.
/// Also renames ALL other capturing groups (named and unnamed) with `__jsse_qi` prefix
/// so they are properly stripped from the result.
fn rename_groups_and_backrefs(chars: &[char], dup_names: &HashSet<String>, idx: u32) -> String {
    let len = chars.len();
    let mut result = String::new();
    let mut i = 0;
    let mut in_cc = false;
    let mut unnamed_counter: u32 = 0;
    while i < len {
        match chars[i] {
            '[' if !in_cc => {
                in_cc = true;
                result.push('[');
            }
            ']' if in_cc => {
                in_cc = false;
                result.push(']');
            }
            '\\' if !in_cc && i + 1 < len && chars[i + 1] == 'k' => {
                if i + 2 < len && chars[i + 2] == '<' {
                    let start = i + 3;
                    if let Some(end_off) = chars[start..].iter().position(|&c| c == '>') {
                        let name: String = chars[start..start + end_off].iter().collect();
                        if dup_names.contains(&name) {
                            result.push_str(&format!("\\k<__jsse_qi{}__{}>", idx, name));
                            i = start + end_off + 1;
                            continue;
                        }
                    }
                }
                result.push('\\');
                result.push(chars[i + 1]);
                i += 2;
                continue;
            }
            '\\' if i + 1 < len => {
                result.push('\\');
                result.push(chars[i + 1]);
                i += 2;
                continue;
            }
            '(' if !in_cc
                && i + 2 < len
                && chars[i + 1] == '?'
                && chars[i + 2] == '<'
                && i + 3 < len
                && chars[i + 3] != '='
                && chars[i + 3] != '!' =>
            {
                // Named group
                let name_start = i + 3;
                if let Some(end_off) = chars[name_start..].iter().position(|&c| c == '>') {
                    let name: String = chars[name_start..name_start + end_off].iter().collect();
                    // Rename ALL named groups with __jsse_qi prefix (not just dup ones)
                    result.push_str(&format!("(?<__jsse_qi{}__{}>", idx, name));
                    i = name_start + end_off + 1;
                    continue;
                } else {
                    result.push(chars[i]);
                }
            }
            '(' if !in_cc && (i + 1 >= len || chars[i + 1] != '?') => {
                // Unnamed capturing group — rename to __jsse_qi named group for stripping
                unnamed_counter += 1;
                result.push_str(&format!("(?<__jsse_qi{}_u{}>", idx, unnamed_counter));
                i += 1;
                continue;
            }
            c => result.push(c),
        }
        i += 1;
    }
    result
}

/// Preprocess a JS regex source to expand `(?:BODY){N}` where N >= 2, BODY contains
/// duplicate-named groups. If BODY has no backreferences to duplicate names, expands to
/// `(?:ANON_BODY){N-1}(?:BODY)`. If BODY has backreferences, uses renaming to keep
/// groups and backrefs paired per iteration.
fn expand_quantified_dup_groups(
    source: &str,
    dup_names: &HashSet<String>,
) -> Result<String, String> {
    const MAX_PREPROCESS_RECURSION_DEPTH: usize = 256;
    expand_quantified_dup_groups_with_depth(source, dup_names, 0, MAX_PREPROCESS_RECURSION_DEPTH)
}

fn expand_quantified_dup_groups_with_depth(
    source: &str,
    dup_names: &HashSet<String>,
    depth: usize,
    max_depth: usize,
) -> Result<String, String> {
    if depth >= max_depth {
        return Err(
            "regular expression nesting too deep for duplicate-group preprocessing".to_string(),
        );
    }

    if dup_names.is_empty() {
        return Ok(source.to_string());
    }
    let chars: Vec<char> = source.chars().collect();
    let len = chars.len();
    let mut result = String::new();
    let mut i = 0;
    let mut in_cc = false;

    while i < len {
        match chars[i] {
            '[' if !in_cc => {
                in_cc = true;
                result.push('[');
                i += 1;
            }
            ']' if in_cc => {
                in_cc = false;
                result.push(']');
                i += 1;
            }
            '\\' if i + 1 < len => {
                result.push('\\');
                result.push(chars[i + 1]);
                i += 2;
            }
            '(' if !in_cc && i + 2 < len && chars[i + 1] == '?' && chars[i + 2] == ':' => {
                // Found (?:  — check if it's followed by {N} after matching close
                let open = i;
                if let Some(close) = find_matching_close_paren(&chars, open) {
                    let body = &chars[open + 3..close]; // content between (?:  and  )
                    // Check for {N} after ')'
                    let after = close + 1;
                    if after < len && chars[after] == '{' {
                        // Parse {N} or {N,M}
                        let mut j = after + 1;
                        let num_start = j;
                        while j < len && chars[j].is_ascii_digit() {
                            j += 1;
                        }
                        if j > num_start && j < len {
                            let num_str: String = chars[num_start..j].iter().collect();
                            let n: u32 = num_str.parse().unwrap_or(0);
                            // Only expand exact {N} quantifiers (not {N,M})
                            if chars[j] == '}' && n >= 2 {
                                let quant_end = j + 1;
                                // Check optional lazy '?'
                                let quant_end = if quant_end < len && chars[quant_end] == '?' {
                                    quant_end + 1
                                } else {
                                    quant_end
                                };
                                if body_has_dup_named_group(body, dup_names) {
                                    let has_backrefs = body_has_named_backref_to(body, dup_names);
                                    // Recursively expand the body first
                                    let body_str: String = body.iter().collect();
                                    let expanded_body = expand_quantified_dup_groups_with_depth(
                                        &body_str,
                                        dup_names,
                                        depth + 1,
                                        max_depth,
                                    )?;
                                    let expanded_body_chars: Vec<char> =
                                        expanded_body.chars().collect();

                                    if has_backrefs {
                                        // Rename groups+backrefs for each non-last iteration
                                        for iter_i in 0..(n - 1) {
                                            let renamed = rename_groups_and_backrefs(
                                                &expanded_body_chars,
                                                dup_names,
                                                iter_i,
                                            );
                                            result.push_str(&format!("(?:{})", renamed));
                                        }
                                    } else {
                                        let anon_body =
                                            anonymize_named_groups(&expanded_body_chars);
                                        if n - 1 == 1 {
                                            result.push_str(&format!("(?:{})", anon_body));
                                        } else {
                                            result.push_str(&format!(
                                                "(?:{}){{{}}}",
                                                anon_body,
                                                n - 1
                                            ));
                                        }
                                    }
                                    result.push_str(&format!("(?:{})", expanded_body));
                                    i = quant_end;
                                    continue;
                                }
                            }
                        }
                    }
                    // Not a candidate for expansion — recurse into body
                    let body_str: String = body.iter().collect();
                    let expanded_body = expand_quantified_dup_groups_with_depth(
                        &body_str,
                        dup_names,
                        depth + 1,
                        max_depth,
                    )?;
                    result.push_str("(?:");
                    result.push_str(&expanded_body);
                    result.push(')');
                    // Copy any quantifier that follows
                    i = close + 1;
                    #[allow(clippy::never_loop)]
                    while i < len {
                        match chars[i] {
                            '{' => {
                                result.push('{');
                                i += 1;
                                while i < len && chars[i] != '}' {
                                    result.push(chars[i]);
                                    i += 1;
                                }
                                if i < len {
                                    result.push('}');
                                    i += 1;
                                }
                                if i < len && chars[i] == '?' {
                                    result.push('?');
                                    i += 1;
                                }
                                break;
                            }
                            '*' | '+' | '?' => {
                                result.push(chars[i]);
                                i += 1;
                                if i < len && chars[i] == '?' {
                                    result.push('?');
                                    i += 1;
                                }
                                break;
                            }
                            _ => break,
                        }
                    }
                    continue;
                } else {
                    result.push(chars[i]);
                    i += 1;
                }
            }
            c => {
                result.push(c);
                i += 1;
            }
        }
    }
    Ok(result)
}

pub(super) fn translate_js_pattern_ex(
    source: &str,
    flags: &str,
) -> Result<TranslationResult, String> {
    let mut result = String::new();
    if flags.contains('i') {
        result.push_str("(?i)");
    }
    if flags.contains('m') {
        result.push_str("(?m)");
    }

    let chars: Vec<char> = source.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut in_char_class = false;
    let mut cc_prev_was_class_escape = false;
    let mut groups_seen: u32 = 0;
    let mut open_groups: Vec<u32> = Vec::new();
    let mut open_group_names: Vec<Option<String>> = Vec::new();
    let mut group_num_to_name: HashMap<u32, String> = HashMap::default();
    let mut group_is_capturing: Vec<bool> = Vec::new();
    let mut lookbehind_depth: u32 = 0;
    let mut is_lookbehind_group: Vec<bool> = Vec::new();
    let mut is_lookahead_group: Vec<bool> = Vec::new();
    let mut needs_bytes_mode = false;
    // For capturing groups inside lookbehinds, track the position of '(' in result
    let mut group_result_start: Vec<Option<usize>> = Vec::new();
    let dot_all_base = flags.contains('s');
    // Stack for tracking dotAll state through modifier groups.
    // Each entry is Some(previous_dotall) for modifier groups that change s,
    // or None for regular groups.
    let mut dotall_stack: Vec<Option<bool>> = Vec::new();
    let mut dot_all = dot_all_base;
    let unicode = flags.contains('u') || flags.contains('v');
    let icase_base = flags.contains('i');
    let mut icase = icase_base;
    let mut icase_stack: Vec<Option<bool>> = Vec::new();
    let non_unicode_icase = |ic: bool| -> bool { ic && !unicode };

    // Pre-count total capturing groups in the pattern
    let total_groups = {
        let mut count: u32 = 0;
        let mut j = 0;
        let mut in_cc = false;
        while j < len {
            match chars[j] {
                '[' if !in_cc => {
                    in_cc = true;
                }
                ']' if in_cc => {
                    in_cc = false;
                }
                '\\' if j + 1 < len => {
                    j += 1;
                } // skip escaped char
                '(' if !in_cc => {
                    if j + 1 < len && chars[j + 1] == '?' {
                        // (?<name>...) is capturing if not (?<=) or (?<!)
                        if j + 2 < len && chars[j + 2] == '<' {
                            if j + 3 < len && (chars[j + 3] == '=' || chars[j + 3] == '!') {
                                // lookbehind, not capturing
                            } else {
                                count += 1; // named group
                            }
                        }
                        // (?:...), (?=...), (?!...) are non-capturing
                    } else {
                        count += 1; // plain capturing group
                    }
                }
                _ => {}
            }
            j += 1;
        }
        count
    };

    // Pre-scan to find all named group names and detect duplicates
    let mut all_group_names: Vec<String> = Vec::new();
    {
        let mut j = 0;
        let mut in_cc = false;
        while j < len {
            match chars[j] {
                '[' if !in_cc => in_cc = true,
                ']' if in_cc => in_cc = false,
                '\\' if j + 1 < len => {
                    j += 1;
                }
                '(' if !in_cc
                    && j + 2 < len
                    && chars[j + 1] == '?'
                    && chars[j + 2] == '<'
                    && j + 3 < len
                    && chars[j + 3] != '='
                    && chars[j + 3] != '!' =>
                {
                    // Named group — extract name
                    let name_start = j + 3;
                    let mut k = name_start;
                    while k < len && chars[k] != '>' {
                        k += 1;
                    }
                    if k < len {
                        let name = decode_group_name_raw(&chars[name_start..k]);
                        all_group_names.push(name);
                    }
                }
                _ => {}
            }
            j += 1;
        }
    }
    let mut name_count: HashMap<String, usize> = HashMap::default();
    for name in &all_group_names {
        *name_count.entry(name.clone()).or_insert(0) += 1;
    }
    let mut duplicated_names: HashSet<String> = name_count
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(name, _)| name)
        .collect();

    // Preprocess: expand (?:BODY){N} quantifiers where BODY has duplicate-named
    // groups. This ensures that captures from earlier iterations don't bleed into
    // later iterations (PCRE retains captures across quantifier iterations, but
    // ECMAScript resets them).
    let (chars, len, all_group_names) = if !duplicated_names.is_empty() {
        let preprocessed = expand_quantified_dup_groups(source, &duplicated_names)?;
        if preprocessed != source {
            let new_chars: Vec<char> = preprocessed.chars().collect();
            let new_len = new_chars.len();
            // Re-scan group names from expanded source
            let mut new_names: Vec<String> = Vec::new();
            {
                let mut j = 0;
                let mut in_cc2 = false;
                while j < new_len {
                    match new_chars[j] {
                        '[' if !in_cc2 => in_cc2 = true,
                        ']' if in_cc2 => in_cc2 = false,
                        '\\' if j + 1 < new_len => {
                            j += 1;
                        }
                        '(' if !in_cc2
                            && j + 2 < new_len
                            && new_chars[j + 1] == '?'
                            && new_chars[j + 2] == '<'
                            && j + 3 < new_len
                            && new_chars[j + 3] != '='
                            && new_chars[j + 3] != '!' =>
                        {
                            let name_start = j + 3;
                            let mut k = name_start;
                            while k < new_len && new_chars[k] != '>' {
                                k += 1;
                            }
                            if k < new_len {
                                let name = decode_group_name_raw(&new_chars[name_start..k]);
                                new_names.push(name);
                            }
                        }
                        _ => {}
                    }
                    j += 1;
                }
            }
            // Re-compute duplicated_names from expanded source (renamed groups
            // like __jsse_qi0__x are also duplicates)
            let mut new_name_count: HashMap<String, usize> = HashMap::default();
            for name in &new_names {
                *new_name_count.entry(name.clone()).or_insert(0) += 1;
            }
            duplicated_names = new_name_count
                .into_iter()
                .filter(|(_, count)| *count > 1)
                .map(|(name, _)| name)
                .collect();
            (new_chars, new_len, new_names)
        } else {
            (chars, len, all_group_names)
        }
    } else {
        (chars, len, all_group_names)
    };

    // When any named groups exist, fancy_regex requires ALL backreferences to
    // use named syntax. We auto-name unnamed capturing groups with a special
    // prefix so we can use named backreferences throughout.
    let has_named_groups = !all_group_names.is_empty();
    // Track how many times we've seen each duplicated name during translation
    let mut dup_seen_count: HashMap<String, u32> = HashMap::default();
    let mut dup_group_map: HashMap<String, Vec<(String, u32)>> = HashMap::default();
    let mut group_name_order: Vec<String> = Vec::new();
    let mut group_name_seen: HashSet<String> = HashSet::default();

    while i < len {
        let c = chars[i];

        if c == '[' && !in_char_class {
            // [^] in JS means "match any character" — translate to (?s:.)
            if i + 2 < len && chars[i + 1] == '^' && chars[i + 2] == ']' {
                result.push_str("(?s:.)");
                i += 3;
                continue;
            }
            // v-flag: use specialized parser that handles nested classes,
            // set operations (&&, --), property-of-strings, and \q{...}
            if flags.contains('v') {
                i += 1; // skip opening [
                let (vclass, new_i) = parse_v_flag_class(&chars, i, flags)?;
                i = new_i;
                result.push_str(&vclass.to_regex_pattern());
                continue;
            }
            in_char_class = true;
            result.push(c);
            i += 1;
            continue;
        }
        if c == ']' && in_char_class {
            in_char_class = false;
            cc_prev_was_class_escape = false;
            result.push(c);
            i += 1;
            continue;
        }
        // JS treats '[' as a literal inside a character class (without v-flag),
        // but fancy_regex interprets it as a nested class. Escape it.
        if c == '[' && in_char_class {
            result.push_str("\\[");
            cc_prev_was_class_escape = false;
            i += 1;
            continue;
        }

        // Annex B: inside character classes in non-unicode mode, when '-' is between
        // a class escape (\d,\D,\w,\W,\s,\S) and another atom (or vice versa),
        // treat '-' as literal rather than forming a range.
        if c == '-' && in_char_class && !unicode {
            let next_is_class_escape = i + 2 < len
                && chars[i + 1] == '\\'
                && matches!(chars[i + 2], 'd' | 'D' | 'w' | 'W' | 's' | 'S');
            if cc_prev_was_class_escape || next_is_class_escape {
                result.push_str("\\-");
                cc_prev_was_class_escape = false;
                i += 1;
                continue;
            }
        }

        if c == '\\' && i + 1 < len {
            let next = chars[i + 1];
            if in_char_class {
                cc_prev_was_class_escape = false;
            }
            match next {
                // Named backreference: \k<name> → (?P=name) or alternation for duplicates
                'k' if !in_char_class && i + 2 < len && chars[i + 2] == '<' => {
                    if !unicode && all_group_names.is_empty() {
                        // Annex B: \k without named groups is identity escape
                        if non_unicode_icase(icase) {
                            push_case_fold_guarded(&mut result, 'k', false);
                        } else {
                            push_literal_char(&mut result, 'k', false);
                        }
                        i += 2;
                    } else {
                        let start = i + 3;
                        if let Some(end) = chars[start..].iter().position(|&c| c == '>') {
                            let name = decode_group_name_raw(&chars[start..start + end]);
                            if duplicated_names.contains(&name) {
                                if let Some(variants) = dup_group_map.get(&name) {
                                    let backrefs: Vec<String> = variants
                                        .iter()
                                        .map(|(iname, _)| {
                                            format!("(?P={})", sanitize_group_name(iname))
                                        })
                                        .collect();
                                    let guards: Vec<String> = variants
                                        .iter()
                                        .map(|(iname, _)| {
                                            format!("(?(<{}>)(?!)|)", sanitize_group_name(iname))
                                        })
                                        .collect();
                                    result.push_str(&format!(
                                        "(?:{}|{})",
                                        backrefs.join("|"),
                                        guards.join("")
                                    ));
                                } else {
                                    result
                                        .push_str(&format!("(?P={})", sanitize_group_name(&name)));
                                }
                            } else {
                                let is_forward = !group_name_seen.contains(&name);
                                let is_self_ref =
                                    open_group_names.iter().any(|n| n.as_deref() == Some(&name));
                                if is_forward || is_self_ref {
                                    result.push_str("(?:)");
                                } else {
                                    let sname = sanitize_group_name(&name);
                                    result.push_str(&format!("(?(<{}>)(?P={}))", sname, sname));
                                }
                            }
                            i = start + end + 1;
                            continue;
                        }
                        result.push_str("\\k");
                        i += 2;
                    }
                }
                // \0 → null character (if not followed by digit)
                // \0NN → octal escape (Annex B, non-Unicode only)
                '0' => {
                    if i + 2 >= len || !chars[i + 2].is_ascii_digit() {
                        result.push('\0');
                        i += 2;
                    } else if !flags.contains('u') && !flags.contains('v') {
                        // Octal escape: \0 followed by octal digits
                        let mut octal_end = i + 1; // start from '0'
                        let mut octal_count = 0;
                        while octal_end < len
                            && octal_count < 3
                            && chars[octal_end] >= '0'
                            && chars[octal_end] <= '7'
                        {
                            octal_end += 1;
                            octal_count += 1;
                        }
                        let octal_str: String = chars[i + 1..octal_end].iter().collect();
                        if let Ok(val) = u32::from_str_radix(&octal_str, 8)
                            && let Some(ch) = char::from_u32(val)
                        {
                            if non_unicode_icase(icase)
                                && !in_char_class
                                && needs_case_fold_guard(ch)
                            {
                                push_case_fold_guarded(&mut result, ch, false);
                            } else {
                                push_literal_char(&mut result, ch, in_char_class);
                            }
                        }
                        i = octal_end;
                    } else {
                        result.push('\0');
                        i += 2;
                    }
                }
                // \cX → control character
                'c' if i + 2 < len && chars[i + 2].is_ascii_alphabetic() => {
                    let ctrl = (chars[i + 2] as u8 % 32) as char;
                    result.push(ctrl);
                    i += 3;
                }
                // Annex B: \c inside character class with digit or underscore
                'c' if !unicode
                    && in_char_class
                    && i + 2 < len
                    && (chars[i + 2].is_ascii_digit() || chars[i + 2] == '_') =>
                {
                    let ctrl = (chars[i + 2] as u8 % 32) as char;
                    push_literal_char(&mut result, ctrl, in_char_class);
                    i += 3;
                }
                // Annex B: \c + non-letter outside class → match literal backslash + c
                'c' if !unicode => {
                    push_literal_char(&mut result, '\\', in_char_class);
                    push_literal_char(&mut result, 'c', in_char_class);
                    i += 2;
                }
                // \xHH → hex escape
                'x' if i + 3 < len
                    && chars[i + 2].is_ascii_hexdigit()
                    && chars[i + 3].is_ascii_hexdigit() =>
                {
                    let hex: String = chars[i + 2..i + 4].iter().collect();
                    if let Ok(cp) = u32::from_str_radix(&hex, 16)
                        && let Some(ch) = char::from_u32(cp)
                    {
                        if non_unicode_icase(icase) && !in_char_class && needs_case_fold_guard(ch) {
                            push_case_fold_guarded(&mut result, ch, false);
                        } else {
                            push_literal_char(&mut result, ch, in_char_class);
                        }
                    }
                    i += 4;
                }
                // \uHHHH or \u{HHHH+}
                'u' => {
                    if unicode && i + 2 < len && chars[i + 2] == '{' {
                        // \u{HHHH+}
                        let start = i + 3;
                        if let Some(end) = chars[start..].iter().position(|&c| c == '}') {
                            let hex: String = chars[start..start + end].iter().collect();
                            if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                                if is_surrogate(cp) {
                                    push_literal_char(
                                        &mut result,
                                        surrogate_to_pua(cp),
                                        in_char_class,
                                    );
                                } else if let Some(ch) = char::from_u32(cp) {
                                    if non_unicode_icase(icase)
                                        && !in_char_class
                                        && needs_case_fold_guard(ch)
                                    {
                                        push_case_fold_guarded(&mut result, ch, false);
                                    } else {
                                        push_literal_char(&mut result, ch, in_char_class);
                                    }
                                }
                            }
                            i = start + end + 1;
                        } else {
                            result.push_str("\\u");
                            i += 2;
                        }
                    } else if i + 5 < len
                        && chars[i + 2].is_ascii_hexdigit()
                        && chars[i + 3].is_ascii_hexdigit()
                        && chars[i + 4].is_ascii_hexdigit()
                        && chars[i + 5].is_ascii_hexdigit()
                    {
                        let hex: String = chars[i + 2..i + 6].iter().collect();
                        if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                            if unicode
                                && (0xD800..=0xDBFF).contains(&cp)
                                && i + 11 < len
                                && chars[i + 6] == '\\'
                                && chars[i + 7] == 'u'
                                && chars[i + 8].is_ascii_hexdigit()
                                && chars[i + 9].is_ascii_hexdigit()
                                && chars[i + 10].is_ascii_hexdigit()
                                && chars[i + 11].is_ascii_hexdigit()
                            {
                                let trail_hex: String = chars[i + 8..i + 12].iter().collect();
                                if let Ok(trail) = u32::from_str_radix(&trail_hex, 16)
                                    && (0xDC00..=0xDFFF).contains(&trail)
                                {
                                    let combined =
                                        0x10000 + ((cp - 0xD800) << 10) + (trail - 0xDC00);
                                    if let Some(ch) = char::from_u32(combined) {
                                        push_literal_char(&mut result, ch, in_char_class);
                                    }
                                    i += 12;
                                } else {
                                    if is_surrogate(cp) {
                                        push_literal_char(
                                            &mut result,
                                            surrogate_to_pua(cp),
                                            in_char_class,
                                        );
                                    } else if let Some(ch) = char::from_u32(cp) {
                                        push_literal_char(&mut result, ch, in_char_class);
                                    }
                                    i += 6;
                                }
                            } else if is_surrogate(cp) {
                                push_literal_char(&mut result, surrogate_to_pua(cp), in_char_class);
                                i += 6;
                            } else if let Some(ch) = char::from_u32(cp) {
                                if non_unicode_icase(icase)
                                    && !in_char_class
                                    && needs_case_fold_guard(ch)
                                {
                                    push_case_fold_guarded(&mut result, ch, false);
                                } else {
                                    push_literal_char(&mut result, ch, in_char_class);
                                }
                                i += 6;
                            } else {
                                i += 6;
                            }
                        } else {
                            i += 6;
                        }
                    } else if !unicode {
                        // Annex B: incomplete \u is identity escape in non-unicode mode
                        push_literal_char(&mut result, 'u', in_char_class);
                        i += 2;
                    } else {
                        result.push_str("\\u");
                        i += 2;
                    }
                }
                // Pass through known regex escapes
                'd' | 'D' | 'w' | 'W' | 's' | 'S' | 'b' | 'B' | 'n' | 'r' | 't' | 'f' | 'v' => {
                    if next == 'v' {
                        result.push('\x0B');
                    } else if next == 's' {
                        // JS \s differs from Rust: includes FEFF, excludes 0x85 (NEL)
                        // Use explicit character class matching JS spec
                        let js_ws = "\\x{09}\\x{0A}\\x{0B}\\x{0C}\\x{0D}\\x{20}\\x{A0}\\x{1680}\\x{2000}-\\x{200A}\\x{2028}\\x{2029}\\x{202F}\\x{205F}\\x{3000}\\x{FEFF}";
                        if in_char_class {
                            result.push_str(js_ws);
                        } else {
                            result.push('[');
                            result.push_str(js_ws);
                            result.push(']');
                        }
                    } else if next == 'S' {
                        let js_ws = "\\x{09}\\x{0A}\\x{0B}\\x{0C}\\x{0D}\\x{20}\\x{A0}\\x{1680}\\x{2000}-\\x{200A}\\x{2028}\\x{2029}\\x{202F}\\x{205F}\\x{3000}\\x{FEFF}";
                        if in_char_class {
                            // Complement of JS whitespace as explicit ranges
                            result.push_str("\\x{00}-\\x{08}\\x{0E}-\\x{1F}\\x{21}-\\x{9F}\\x{A1}-\\x{167F}\\x{1681}-\\x{1FFF}\\x{200B}-\\x{2027}\\x{202A}-\\x{202E}\\x{2030}-\\x{205E}\\x{2060}-\\x{2FFF}\\x{3001}-\\x{FEFE}\\x{FF00}-\\x{10FFFF}");
                        } else {
                            result.push_str("[^");
                            result.push_str(js_ws);
                            result.push(']');
                        }
                    } else if next == 'd' {
                        // ES spec: \d = [0-9] always (Rust \d matches Unicode digits)
                        if in_char_class {
                            result.push_str("0-9");
                        } else {
                            result.push_str("[0-9]");
                        }
                    } else if next == 'D' {
                        if in_char_class {
                            result.push_str("\\x{00}-\\x{2F}\\x{3A}-\\x{10FFFF}");
                        } else {
                            result.push_str("[^0-9]");
                        }
                    } else if next == 'w' {
                        // ES spec: \w = [A-Za-z0-9_] always
                        // Under unicode + ignoreCase (current modifier), include U+017F/U+212A
                        if unicode && icase {
                            if in_char_class {
                                result.push_str("A-Za-z0-9_\\x{017F}\\x{212A}");
                            } else {
                                result.push_str("[A-Za-z0-9_\\x{017F}\\x{212A}]");
                            }
                        } else if in_char_class {
                            result.push_str("A-Za-z0-9_");
                        } else {
                            result.push_str("[A-Za-z0-9_]");
                        }
                    } else if next == 'W' {
                        if unicode && icase {
                            if in_char_class {
                                result.push_str("\\x{00}-\\x{2F}\\x{3A}-\\x{40}\\x{5B}-\\x{5E}\\x{60}\\x{7B}-\\x{017E}\\x{0180}-\\x{2129}\\x{212B}-\\x{10FFFF}");
                            } else {
                                result.push_str("[^A-Za-z0-9_\\x{017F}\\x{212A}]");
                            }
                        } else if in_char_class {
                            result.push_str("\\x{00}-\\x{2F}\\x{3A}-\\x{40}\\x{5B}-\\x{5E}\\x{60}\\x{7B}-\\x{10FFFF}");
                        } else {
                            result.push_str("[^A-Za-z0-9_]");
                        }
                    } else if next == 'b' {
                        if in_char_class {
                            // \b inside character class means backspace (U+0008)
                            result.push_str("\\x{08}");
                        } else if unicode && icase {
                            // Under unicode + ignoreCase (current modifier), include U+017F/U+212A
                            result.push_str("(?:(?<=(?-i:[A-Za-z0-9_\\x{017F}\\x{212A}]))(?!(?-i:[A-Za-z0-9_\\x{017F}\\x{212A}]))|(?<!(?-i:[A-Za-z0-9_\\x{017F}\\x{212A}]))(?=(?-i:[A-Za-z0-9_\\x{017F}\\x{212A}])))");
                        } else {
                            // JS \b always uses ASCII word chars [A-Za-z0-9_]
                            // (Rust \b uses Unicode word boundaries which is wrong for JS)
                            result.push_str("(?:(?<=(?-i:[A-Za-z0-9_]))(?!(?-i:[A-Za-z0-9_]))|(?<!(?-i:[A-Za-z0-9_]))(?=(?-i:[A-Za-z0-9_])))");
                        }
                    } else if next == 'B' {
                        if in_char_class {
                            // \B inside character class is literal B in non-unicode mode
                            result.push('B');
                        } else if unicode && icase {
                            result.push_str("(?:(?<=(?-i:[A-Za-z0-9_\\x{017F}\\x{212A}]))(?=(?-i:[A-Za-z0-9_\\x{017F}\\x{212A}]))|(?<!(?-i:[A-Za-z0-9_\\x{017F}\\x{212A}]))(?!(?-i:[A-Za-z0-9_\\x{017F}\\x{212A}])))");
                        } else {
                            result.push_str("(?:(?<=(?-i:[A-Za-z0-9_]))(?=(?-i:[A-Za-z0-9_]))|(?<!(?-i:[A-Za-z0-9_]))(?!(?-i:[A-Za-z0-9_])))");
                        }
                    } else {
                        result.push('\\');
                        result.push(next);
                    }
                    if in_char_class && matches!(next, 'd' | 'D' | 'w' | 'W' | 's' | 'S') {
                        cc_prev_was_class_escape = true;
                    }
                    i += 2;
                }
                // Numeric backreferences and octal escapes
                '1'..='9' => {
                    // Collect all digits
                    let ref_start = i + 1;
                    let mut ref_end = i + 2;
                    while ref_end < len && chars[ref_end].is_ascii_digit() {
                        ref_end += 1;
                    }
                    let ref_str: String = chars[ref_start..ref_end].iter().collect();
                    let ref_num: u32 = ref_str.parse().unwrap_or(0);
                    if ref_num <= total_groups
                        && (ref_num > groups_seen || open_groups.contains(&ref_num))
                    {
                        // Forward or self-referential backref: matches empty string
                        result.push_str("(?:)");
                        i = ref_end;
                    } else if ref_num <= total_groups {
                        // Backward backreference: use conditional to handle
                        // groups that didn't participate (e.g. (a)?\1)
                        if let Some(gname) = group_num_to_name.get(&ref_num) {
                            let sname = sanitize_group_name(gname);
                            if duplicated_names.contains(gname) {
                                if let Some(variants) = dup_group_map.get(gname) {
                                    let backrefs: Vec<String> = variants
                                        .iter()
                                        .map(|(iname, _)| {
                                            format!("(?P={})", sanitize_group_name(iname))
                                        })
                                        .collect();
                                    let guards: Vec<String> = variants
                                        .iter()
                                        .map(|(iname, _)| {
                                            format!("(?(<{}>)(?!)|)", sanitize_group_name(iname))
                                        })
                                        .collect();
                                    result.push_str(&format!(
                                        "(?:{}|{})",
                                        backrefs.join("|"),
                                        guards.join("")
                                    ));
                                } else {
                                    result
                                        .push_str(&format!("(?P={})", sanitize_group_name(gname)));
                                }
                            } else {
                                result.push_str(&format!("(?(<{}>)(?P={})|)", sname, sname));
                            }
                        } else {
                            result.push_str(&format!("(?({})", ref_num));
                            result.push('\\');
                            for &ch in &chars[ref_start..ref_end] {
                                result.push(ch);
                            }
                            result.push_str("|)");
                        }
                        i = ref_end;
                    } else if !flags.contains('u') && !flags.contains('v') {
                        // Annex B: LegacyOctalEscapeSequence or identity escape
                        if next == '8' || next == '9' {
                            // \8 and \9 are identity escapes (not valid octal)
                            if non_unicode_icase(icase)
                                && !in_char_class
                                && needs_case_fold_guard(next)
                            {
                                push_case_fold_guarded(&mut result, next, false);
                            } else {
                                push_literal_char(&mut result, next, in_char_class);
                            }
                            i += 2;
                        } else {
                            // LegacyOctalEscapeSequence grammar:
                            //   ZeroToThree OctalDigit OctalDigit  (3 digits max for 0-3)
                            //   FourToSeven OctalDigit             (2 digits max for 4-7)
                            let first_digit = next;
                            let max_digits = if first_digit <= '3' { 3 } else { 2 };
                            let mut octal_end = i + 1;
                            let mut octal_count = 0;
                            while octal_end < len
                                && octal_count < max_digits
                                && chars[octal_end] >= '0'
                                && chars[octal_end] <= '7'
                            {
                                octal_end += 1;
                                octal_count += 1;
                            }
                            let octal_str: String = chars[i + 1..octal_end].iter().collect();
                            if let Ok(val) = u32::from_str_radix(&octal_str, 8)
                                && let Some(ch) = char::from_u32(val)
                            {
                                if non_unicode_icase(icase)
                                    && !in_char_class
                                    && needs_case_fold_guard(ch)
                                {
                                    push_case_fold_guarded(&mut result, ch, false);
                                } else {
                                    push_literal_char(&mut result, ch, in_char_class);
                                }
                                i = octal_end;
                            } else {
                                push_literal_char(&mut result, next, in_char_class);
                                i += 2;
                            }
                        }
                    } else {
                        // Unicode mode: pass through (will error in regex engine)
                        result.push('\\');
                        for &ch in &chars[ref_start..ref_end] {
                            result.push(ch);
                        }
                        i = ref_end;
                    }
                }
                // Unicode property escapes: \p{...} / \P{...}
                'p' | 'P'
                    if (flags.contains('u') || flags.contains('v'))
                        && i + 2 < len
                        && chars[i + 2] == '{' =>
                {
                    let start = i + 3;
                    if let Some(end) = chars[start..].iter().position(|&c| c == '}') {
                        let content: String = chars[start..start + end].iter().collect();
                        validate_unicode_property_escape(&content)?;
                        let negated = next == 'P';
                        // Check for property-of-strings (v-flag only, outside char class)
                        if flags.contains('v')
                            && !in_char_class
                            && let Some((singles, multi_strs)) =
                                crate::emoji_strings::lookup_string_property(&content)
                        {
                            if negated {
                                return Err(format!(
                                    "Invalid property escape: \\P{{{}}} cannot negate a property of strings",
                                    content
                                ));
                            }
                            let mut vset = VClassSet::new();
                            for &cp in singles {
                                vset.add_codepoint(cp);
                            }
                            for s in multi_strs {
                                vset.add_string(s.to_string());
                            }
                            result.push_str(&vset.to_regex_pattern());
                            i = start + end + 1;
                            continue;
                        }
                        if let Some(ranges) = crate::unicode_tables::lookup_property(&content) {
                            if is_cs_property(&content) {
                                needs_bytes_mode = true;
                                if negated {
                                    // \P{Cs}: all valid Unicode (WTF-8 surrogates naturally excluded)
                                    expand_property_to_char_class(
                                        &mut result,
                                        ranges,
                                        true,
                                        in_char_class,
                                    );
                                } else if !in_char_class {
                                    // \p{Cs}: match WTF-8 surrogates
                                    result.push_str("(?-u:\\xED[\\xA0-\\xBF][\\x80-\\xBF])");
                                } else {
                                    expand_property_to_char_class(
                                        &mut result,
                                        ranges,
                                        false,
                                        in_char_class,
                                    );
                                }
                            } else if is_co_property(&content) {
                                needs_bytes_mode = true;
                                if negated && !in_char_class {
                                    // \P{Co}: complement + WTF-8 surrogates
                                    let comp = complement_ranges(ranges);
                                    result.push_str("(?:[");
                                    for &(lo, hi) in &comp {
                                        append_unicode_range(&mut result, lo, hi);
                                    }
                                    result.push_str("]|(?-u:\\xED[\\xA0-\\xBF][\\x80-\\xBF]))");
                                } else {
                                    // \p{Co}: normal ranges (WTF-8 surrogates won't match)
                                    expand_property_to_char_class(
                                        &mut result,
                                        ranges,
                                        negated,
                                        in_char_class,
                                    );
                                }
                            } else {
                                expand_property_to_char_class(
                                    &mut result,
                                    ranges,
                                    negated,
                                    in_char_class,
                                );
                            }
                        } else {
                            // Fallback to fancy_regex for unrecognized properties
                            result.push('\\');
                            result.push(next);
                            result.push('{');
                            result.push_str(&content);
                            result.push('}');
                        }
                        i = start + end + 1;
                    } else {
                        return Err("Invalid property escape: unterminated".to_string());
                    }
                }
                // Identity escapes and escaped syntax chars
                _ => {
                    if is_syntax_character(next) || next == '/' {
                        // Escaped syntax char: keep the backslash
                        result.push('\\');
                        result.push(next);
                    } else if non_unicode_icase(icase)
                        && !in_char_class
                        && needs_case_fold_guard(next)
                    {
                        push_case_fold_guarded(&mut result, next, false);
                    } else if in_char_class && next == '-' {
                        // \- in char class: must escape dash to prevent range interpretation
                        result.push_str("\\-");
                    } else {
                        // Identity escape: push the literal character
                        // (fancy_regex may interpret \< \> \A \Z etc. specially)
                        push_literal_char(&mut result, next, in_char_class);
                    }
                    i += 2;
                }
            }
            continue;
        }

        // Named group: (?<name>...) → (?P<name>...)
        if c == '(' && !in_char_class && i + 2 < len && chars[i + 1] == '?' && chars[i + 2] == '<' {
            // Check it's not (?<=...) or (?<!...)
            if i + 3 < len && (chars[i + 3] == '=' || chars[i + 3] == '!') {
                // Lookbehind - pass through
                lookbehind_depth += 1;
                dotall_stack.push(None);
                icase_stack.push(None);
                group_is_capturing.push(false);
                is_lookbehind_group.push(true);
                is_lookahead_group.push(false);
                group_result_start.push(None);
                open_group_names.push(None);
                result.push_str("(?<");
                result.push(chars[i + 3]);
                i += 4;
            } else {
                // Named group (capturing)
                groups_seen += 1;
                open_groups.push(groups_seen);
                group_is_capturing.push(true);
                is_lookbehind_group.push(false);
                is_lookahead_group.push(false);
                group_result_start.push(if lookbehind_depth > 0 {
                    Some(result.len())
                } else {
                    None
                });
                dotall_stack.push(None);
                icase_stack.push(None);
                // Extract the group name to check if it's duplicated
                let name_start = i + 3;
                let mut k = name_start;
                while k < len && chars[k] != '>' {
                    k += 1;
                }
                let name = decode_group_name_raw(&chars[name_start..k]);
                open_group_names.push(Some(name.clone()));
                group_num_to_name.insert(groups_seen, name.clone());
                if group_name_seen.insert(name.clone()) && !name.starts_with("__jsse_qi") {
                    group_name_order.push(name.clone());
                }
                if duplicated_names.contains(&name) {
                    let seq = dup_seen_count.entry(name.clone()).or_insert(0);
                    *seq += 1;
                    let internal_name = format!("{}__{}", name, seq);
                    dup_group_map
                        .entry(name)
                        .or_default()
                        .push((internal_name.clone(), groups_seen));
                    result.push_str(&format!("(?P<{}>", sanitize_group_name(&internal_name)));
                    i = k + 1; // skip past name and '>'
                } else {
                    result.push_str(&format!("(?P<{}>", sanitize_group_name(&name)));
                    i = k + 1; // skip past name and '>'
                }
            }
            continue;
        }

        // Modifier group: (?[ims]*(-[ims]*)?:...)
        if c == '('
            && !in_char_class
            && i + 1 < len
            && chars[i + 1] == '?'
            && i + 2 < len
            && (chars[i + 2] == 'i'
                || chars[i + 2] == 'm'
                || chars[i + 2] == 's'
                || chars[i + 2] == '-')
        {
            let mut j = i + 2;
            let mut add_i = false;
            let mut add_m = false;
            let mut add_s = false;
            let mut remove_i = false;
            let mut remove_m = false;
            let mut remove_s = false;

            // Parse add flags
            while j < len && chars[j] != '-' && chars[j] != ':' {
                match chars[j] {
                    'i' => add_i = true,
                    'm' => add_m = true,
                    's' => add_s = true,
                    _ => break,
                }
                j += 1;
            }

            if j < len && chars[j] == '-' {
                j += 1;
                while j < len && chars[j] != ':' {
                    match chars[j] {
                        'i' => remove_i = true,
                        'm' => remove_m = true,
                        's' => remove_s = true,
                        _ => break,
                    }
                    j += 1;
                }
            }

            if j < len && chars[j] == ':' {
                // Compute new dotAll for this group
                let prev_dot_all = dot_all;
                if add_s {
                    dot_all = true;
                }
                if remove_s {
                    dot_all = false;
                }
                dotall_stack.push(Some(prev_dot_all));

                let prev_icase = icase;
                if add_i {
                    icase = true;
                }
                if remove_i {
                    icase = false;
                }
                icase_stack.push(Some(prev_icase));
                group_is_capturing.push(false);
                is_lookbehind_group.push(false);
                is_lookahead_group.push(false);
                group_result_start.push(None);
                open_group_names.push(None);

                // Emit the group with s stripped from flags
                result.push_str("(?");
                if add_i {
                    result.push('i');
                }
                if add_m {
                    result.push('m');
                }
                let has_add = add_i || add_m;
                let has_remove = remove_i || remove_m;
                if has_remove {
                    result.push('-');
                    if remove_i {
                        result.push('i');
                    }
                    if remove_m {
                        result.push('m');
                    }
                }
                if !has_add && !has_remove {
                    // All flags were s-only, emit as plain non-capturing group
                    result.push(':');
                } else {
                    result.push(':');
                }
                i = j + 1; // skip past ':'
                continue;
            }
        }

        // Close group: pop dotall/icase state if needed
        if c == ')' && !in_char_class {
            if let Some(Some(prev)) = dotall_stack.pop() {
                dot_all = prev;
            }
            if let Some(Some(prev)) = icase_stack.pop() {
                icase = prev;
            }
            let was_capturing = matches!(group_is_capturing.pop(), Some(true));
            let was_lookbehind = matches!(is_lookbehind_group.pop(), Some(true));
            let was_lookahead = matches!(is_lookahead_group.pop(), Some(true));
            let grp_start = group_result_start.pop().flatten();
            open_group_names.pop();
            if was_capturing {
                open_groups.pop();
            }
            if was_lookbehind {
                lookbehind_depth -= 1;
            }

            // Annex B: quantified lookaheads in non-unicode mode.
            // fancy-regex doesn't handle quantified lookaheads correctly,
            // so we translate them:
            //   min >= 1: keep just the assertion (strip quantifier)
            //   min == 0: remove the assertion entirely
            if was_lookahead && !unicode && i + 1 < len {
                let qc = chars[i + 1];
                let is_quant = qc == '*' || qc == '+' || qc == '?' || qc == '{';
                if is_quant {
                    let (min_is_zero, quant_end) = parse_quantifier_min(&chars, i + 1, len);
                    if min_is_zero {
                        let la_start = find_lookahead_start_in_result(&result);
                        if let Some(start) = la_start {
                            result.truncate(start);
                        } else {
                            result.push(')');
                        }
                    } else {
                        result.push(')');
                    }
                    i = quant_end;
                    continue;
                }
            }

            // RTL capture fix: if this is a capturing group inside a lookbehind
            // followed by {N} (exact repeat), rewrite (content){N} to (content)(?:content){N-1}
            if let Some(start_pos) = grp_start {
                result.push(')');
                // Check if next chars are {N} with exact count
                let mut j = i + 1;
                if j < len && chars[j] == '{' {
                    let _brace_start = j;
                    j += 1;
                    let num_start = j;
                    while j < len && chars[j].is_ascii_digit() {
                        j += 1;
                    }
                    if j > num_start && j < len && chars[j] == '}' {
                        let n: u32 = chars[num_start..j]
                            .iter()
                            .collect::<String>()
                            .parse()
                            .unwrap_or(0);
                        if n > 1 {
                            // Extract the group content from result (between '(' and ')')
                            // For named groups (?P<name>...), skip past the ?P<name> prefix
                            let inner_start = if result[start_pos..].starts_with("(?P<") {
                                // Find the closing '>' of the name
                                if let Some(gt) = result[start_pos..].find('>') {
                                    start_pos + gt + 1
                                } else {
                                    start_pos + 1
                                }
                            } else {
                                start_pos + 1
                            };
                            let group_content = result[inner_start..result.len() - 1].to_string();
                            // Rewrite: append (?:content){N-1}
                            result.push_str(&format!("(?:{}){{{}}}", group_content, n - 1));
                            i = j + 1; // skip past {N}
                            continue;
                        }
                    }
                }
                i += 1;
                continue;
            }

            result.push(')');
            i += 1;
            continue;
        }

        // Handle '(' for other group types (non-capturing (?:), lookahead (?=), (?!), plain)
        if c == '(' && !in_char_class {
            dotall_stack.push(None);
            icase_stack.push(None);
            if i + 1 >= len || chars[i + 1] != '?' {
                groups_seen += 1;
                open_groups.push(groups_seen);
                group_is_capturing.push(true);
                is_lookbehind_group.push(false);
                is_lookahead_group.push(false);
                open_group_names.push(None);
                if lookbehind_depth > 0 {
                    group_result_start.push(Some(result.len()));
                } else {
                    group_result_start.push(None);
                }
                // When pattern has named groups, fancy_regex requires all backrefs
                // to use named syntax. Auto-name this unnamed group so we can
                // generate named backreferences to it if needed.
                if has_named_groups {
                    let auto_name = format!("__jsse_g{}__", groups_seen);
                    group_num_to_name.insert(groups_seen, auto_name.clone());
                    result.push_str(&format!("(?P<{}>", auto_name));
                    i += 1;
                    continue;
                }
            } else {
                let is_la = i + 2 < len && (chars[i + 2] == '=' || chars[i + 2] == '!');
                group_is_capturing.push(false);
                is_lookbehind_group.push(false);
                is_lookahead_group.push(is_la);
                group_result_start.push(None);
                open_group_names.push(None);
            }
        }

        // Dot handling: expand based on dotAll state and unicode mode
        if c == '.' && !in_char_class {
            if unicode {
                if dot_all {
                    result.push_str("(?s:.)");
                } else {
                    result.push_str("[^\\n\\r\\u{2028}\\u{2029}]");
                }
            } else {
                // Non-unicode: . matches one UTF-16 code unit (BMP + PUA-mapped lone surrogates)
                if dot_all {
                    result.push_str("[\\x00-\\u{FFFF}\\u{F0000}-\\u{F07FF}]");
                } else {
                    result.push_str(
                        "[^\\n\\r\\u{2028}\\u{2029}\\u{10000}-\\u{EFFFF}\\u{F0800}-\\u{10FFFF}]",
                    );
                }
            }
            i += 1;
            continue;
        }

        if !unicode && c as u32 >= 0x10000 && pua_to_surrogate(c).is_none() {
            let cp = c as u32;
            let hi = ((cp - 0x10000) >> 10) + 0xD800;
            let lo = ((cp - 0x10000) & 0x3FF) + 0xDC00;
            push_literal_char(&mut result, surrogate_to_pua(hi), in_char_class);
            push_literal_char(&mut result, surrogate_to_pua(lo), in_char_class);
        } else if non_unicode_icase(icase) && !in_char_class && needs_case_fold_guard(c) {
            push_case_fold_guarded(&mut result, c, false);
        } else {
            result.push(c);
        }
        if in_char_class {
            cc_prev_was_class_escape = false;
        }
        i += 1;
    }

    Ok(TranslationResult {
        pattern: result,
        dup_group_map,
        group_name_order,
        needs_bytes_mode,
    })
}

fn append_unicode_range(result: &mut String, lo: u32, hi: u32) {
    if lo == hi {
        result.push_str(&format!("\\u{{{:X}}}", lo));
    } else {
        result.push_str(&format!("\\u{{{:X}}}-\\u{{{:X}}}", lo, hi));
    }
}

fn complement_ranges(ranges: &[(u32, u32)]) -> Vec<(u32, u32)> {
    let mut comp = Vec::new();
    let mut prev = 0u32;
    for &(lo, hi) in ranges {
        if prev < lo {
            comp.push((prev, lo - 1));
        }
        prev = hi + 1;
    }
    if prev <= 0x10FFFF {
        comp.push((prev, 0x10FFFF));
    }
    // Remove surrogate range [0xD800, 0xDFFF]
    let mut result = Vec::new();
    for (lo, hi) in comp {
        if hi < 0xD800 || lo > 0xDFFF {
            result.push((lo, hi));
        } else if lo < 0xD800 && hi > 0xDFFF {
            result.push((lo, 0xD7FF));
            result.push((0xE000, hi));
        } else if lo < 0xD800 {
            result.push((lo, 0xD7FF));
        } else if hi > 0xDFFF {
            result.push((0xE000, hi));
        }
    }
    result
}

fn expand_property_to_char_class(
    result: &mut String,
    ranges: &[(u32, u32)],
    negated: bool,
    in_char_class: bool,
) {
    let effective_ranges: Vec<(u32, u32)>;
    let ranges_to_use = if negated {
        effective_ranges = complement_ranges(ranges);
        &effective_ranges[..]
    } else {
        ranges
    };

    if in_char_class {
        for &(lo, hi) in ranges_to_use {
            append_unicode_range(result, lo, hi);
        }
    } else {
        result.push('[');
        for &(lo, hi) in ranges_to_use {
            append_unicode_range(result, lo, hi);
        }
        result.push(']');
    }
}

fn push_literal_char(result: &mut String, ch: char, _in_char_class: bool) {
    // Escape regex-special chars when inserting literal
    if is_syntax_character(ch) || ch == '/' {
        result.push('\\');
    }
    result.push(ch);
}

fn needs_case_fold_guard(ch: char) -> bool {
    matches!(
        ch,
        's' | 'S'
            | 'k'
            | 'K'
            | '\u{017F}'
            | '\u{212A}'
            | '\u{1E9E}'
            | '\u{212B}'
            | '\u{00DF}'
            | '\u{00E5}'
            | '\u{00C5}'
    )
}

fn push_escaped(result: &mut String, ch: char) {
    if is_syntax_character(ch) || ch == '/' {
        result.push('\\');
    }
    result.push(ch);
}

fn push_case_fold_guarded(result: &mut String, ch: char, in_char_class: bool) {
    if in_char_class {
        push_escaped(result, ch);
        return;
    }
    match ch {
        's' | 'S' => result.push_str("(?-i:[sS])"),
        'k' | 'K' => result.push_str("(?-i:[kK])"),
        '\u{017F}' => result.push_str("(?-i:\u{017F})"),
        '\u{212A}' => result.push_str("(?-i:\u{212A})"),
        '\u{1E9E}' => result.push_str("(?-i:\u{1E9E})"),
        '\u{212B}' => result.push_str("(?-i:\u{212B})"),
        '\u{00DF}' => result.push_str("(?-i:\u{00DF})"),
        '\u{00E5}' | '\u{00C5}' => result.push_str("(?-i:[\u{00E5}\u{00C5}])"),
        _ => push_escaped(result, ch),
    }
}

fn resolve_class_escape(chars: &[char], i: &mut usize) -> Option<u32> {
    if *i >= chars.len() {
        return None;
    }
    let c = chars[*i];
    *i += 1;
    match c {
        '\\' => {
            if *i >= chars.len() {
                return None;
            }
            let next = chars[*i];
            *i += 1;
            match next {
                'n' => Some('\n' as u32),
                'r' => Some('\r' as u32),
                't' => Some('\t' as u32),
                'f' => Some('\x0C' as u32),
                'v' => Some('\x0B' as u32),
                '0' => Some(0),
                'x' => {
                    if *i + 1 < chars.len()
                        && chars[*i].is_ascii_hexdigit()
                        && chars[*i + 1].is_ascii_hexdigit()
                    {
                        let hex: String = chars[*i..*i + 2].iter().collect();
                        *i += 2;
                        u32::from_str_radix(&hex, 16).ok()
                    } else {
                        Some('x' as u32)
                    }
                }
                'u' => {
                    if *i < chars.len() && chars[*i] == '{' {
                        *i += 1;
                        let start = *i;
                        while *i < chars.len() && chars[*i] != '}' {
                            *i += 1;
                        }
                        if *i < chars.len() {
                            let hex: String = chars[start..*i].iter().collect();
                            *i += 1;
                            u32::from_str_radix(&hex, 16).ok()
                        } else {
                            Some('u' as u32)
                        }
                    } else if *i + 3 < chars.len()
                        && chars[*i].is_ascii_hexdigit()
                        && chars[*i + 1].is_ascii_hexdigit()
                        && chars[*i + 2].is_ascii_hexdigit()
                        && chars[*i + 3].is_ascii_hexdigit()
                    {
                        let hex: String = chars[*i..*i + 4].iter().collect();
                        *i += 4;
                        u32::from_str_radix(&hex, 16).ok()
                    } else {
                        Some('u' as u32)
                    }
                }
                'c' => {
                    if *i < chars.len() && chars[*i].is_ascii_alphabetic() {
                        let val = (chars[*i] as u8 % 32) as u32;
                        *i += 1;
                        Some(val)
                    } else {
                        Some('c' as u32)
                    }
                }
                'd' | 'D' | 'w' | 'W' | 's' | 'S' | 'b' | 'B' => None,
                'p' | 'P' => {
                    if *i < chars.len() && chars[*i] == '{' {
                        *i += 1;
                        while *i < chars.len() && chars[*i] != '}' {
                            *i += 1;
                        }
                        if *i < chars.len() {
                            *i += 1; // skip '}'
                        }
                    }
                    None
                }
                '1'..='7' => {
                    // Octal escape (Annex B): \1, \12, \123, etc.
                    let mut val = (next as u32) - ('0' as u32);
                    if *i < chars.len() && ('0'..='7').contains(&chars[*i]) {
                        val = val * 8 + (chars[*i] as u32 - '0' as u32);
                        *i += 1;
                        if *i < chars.len()
                            && ('0'..='7').contains(&chars[*i])
                            && val * 8 + (chars[*i] as u32 - '0' as u32) <= 255
                        {
                            val = val * 8 + (chars[*i] as u32 - '0' as u32);
                            *i += 1;
                        }
                    }
                    Some(val)
                }
                _ => Some(next as u32),
            }
        }
        _ => Some(c as u32),
    }
}

fn validate_modifier_group(chars: &[char], start: usize, source: &str) -> Result<(), String> {
    let len = chars.len();
    let mut j = start;
    let mut add_flags: Vec<char> = Vec::new();
    let mut remove_flags: Vec<char> = Vec::new();

    // Parse add flags
    while j < len && chars[j] != '-' && chars[j] != ':' && chars[j] != ')' {
        let c = chars[j];
        if c == 'i' || c == 'm' || c == 's' {
            if add_flags.contains(&c) {
                return Err(format!(
                    "Invalid regular expression: /{}/ : Invalid flags in modifier group",
                    source
                ));
            }
            add_flags.push(c);
        } else {
            return Err(format!(
                "Invalid regular expression: /{}/ : Invalid flags in modifier group",
                source
            ));
        }
        j += 1;
    }

    if j < len && chars[j] == '-' {
        j += 1;
        // Parse remove flags
        while j < len && chars[j] != ':' && chars[j] != ')' {
            let c = chars[j];
            if c == 'i' || c == 'm' || c == 's' {
                if remove_flags.contains(&c) {
                    return Err(format!(
                        "Invalid regular expression: /{}/ : Invalid flags in modifier group",
                        source
                    ));
                }
                remove_flags.push(c);
            } else {
                return Err(format!(
                    "Invalid regular expression: /{}/ : Invalid flags in modifier group",
                    source
                ));
            }
            j += 1;
        }
    }

    // Must end with ':'
    if j >= len || chars[j] != ':' {
        return Err(format!(
            "Invalid regular expression: /{}/ : Invalid modifier group",
            source
        ));
    }

    // Both sections can't be empty: (?-:...) is invalid
    if add_flags.is_empty() && remove_flags.is_empty() {
        return Err(format!(
            "Invalid regular expression: /{}/ : Invalid flags in modifier group",
            source
        ));
    }

    // Same flag can't appear in both add and remove
    for f in &add_flags {
        if remove_flags.contains(f) {
            return Err(format!(
                "Invalid regular expression: /{}/ : Invalid flags in modifier group",
                source
            ));
        }
    }

    Ok(())
}

fn is_id_start(c: char) -> bool {
    if c == '$' || c == '_' {
        return true;
    }
    c.is_alphabetic()
}

fn is_id_continue(c: char) -> bool {
    if c == '$' || c == '_' || c == '\u{200C}' || c == '\u{200D}' {
        return true;
    }
    c.is_alphanumeric()
        || matches!(unicode_general_category(c), Some(cat) if
        cat == "Mn" || cat == "Mc" || cat == "Pc")
}

fn unicode_general_category(c: char) -> Option<&'static str> {
    let cp = c as u32;
    if cp == 0x5F {
        return Some("Pc");
    } // underscore
    if c.is_ascii_digit() {
        return Some("Nd");
    }
    if c.is_alphabetic() {
        return Some("L");
    }
    // Check combining marks
    if (0x0300..=0x036F).contains(&cp)
        || (0x0483..=0x0489).contains(&cp)
        || (0x0591..=0x05BD).contains(&cp)
        || (0x064B..=0x065F).contains(&cp)
        || (0x0670..=0x0670).contains(&cp)
        || (0x06D6..=0x06DC).contains(&cp)
        || (0x0730..=0x074A).contains(&cp)
        || (0x0900..=0x0903).contains(&cp)
        || (0x093A..=0x094F).contains(&cp)
        || (0x0951..=0x0957).contains(&cp)
        || (0x0962..=0x0963).contains(&cp)
        || (0xFE00..=0xFE0F).contains(&cp)
        || (0x20D0..=0x20FF).contains(&cp)
    {
        return Some("Mn");
    }
    None
}

fn parse_regexp_group_name(
    chars: &[char],
    start: usize,
    source: &str,
    unicode: bool,
) -> Result<(String, usize), String> {
    let len = chars.len();
    let mut i = start;
    let mut name = String::new();

    if i >= len || chars[i] == '>' {
        return Err(format!(
            "Invalid regular expression: /{}/ : Invalid capture group name",
            source
        ));
    }

    // Parse first char - must be ID_Start or unicode escape
    let first_char;
    if chars[i] == '\\' && i + 1 < len && chars[i + 1] == 'u' {
        let (cp, new_i) = parse_unicode_escape_in_name(chars, i, source)?;
        if let Some(c) = char::from_u32(cp) {
            if !is_id_start(c) {
                return Err(format!(
                    "Invalid regular expression: /{}/ : Invalid capture group name",
                    source
                ));
            }
            first_char = c;
            i = new_i;
        } else {
            return Err(format!(
                "Invalid regular expression: /{}/ : Invalid capture group name",
                source
            ));
        }
    } else {
        first_char = chars[i];
        // Check for lone surrogates
        if (0xD800..=0xDFFF).contains(&(first_char as u32)) {
            return Err(format!(
                "Invalid regular expression: /{}/ : Invalid capture group name",
                source
            ));
        }
        if !is_id_start(first_char) {
            return Err(format!(
                "Invalid regular expression: /{}/ : Invalid capture group name",
                source
            ));
        }
        i += 1;
    }
    name.push(first_char);

    // Parse remaining chars - must be ID_Continue or unicode escape
    while i < len && chars[i] != '>' {
        if chars[i] == '\\' && i + 1 < len && chars[i + 1] == 'u' {
            let (cp, new_i) = parse_unicode_escape_in_name(chars, i, source)?;
            if let Some(c) = char::from_u32(cp) {
                if !is_id_continue(c) {
                    return Err(format!(
                        "Invalid regular expression: /{}/ : Invalid capture group name",
                        source
                    ));
                }
                name.push(c);
                i = new_i;
            } else {
                return Err(format!(
                    "Invalid regular expression: /{}/ : Invalid capture group name",
                    source
                ));
            }
        } else {
            let c = chars[i];
            // Check for lone surrogates
            if (0xD800..=0xDFFF).contains(&(c as u32)) {
                if unicode {
                    return Err(format!(
                        "Invalid regular expression: /{}/ : Invalid capture group name",
                        source
                    ));
                }
                // In non-unicode mode, lone surrogates in the source still need to be rejected
                // if they appear literally (not via escape)
                return Err(format!(
                    "Invalid regular expression: /{}/ : Invalid capture group name",
                    source
                ));
            }
            if !is_id_continue(c) {
                return Err(format!(
                    "Invalid regular expression: /{}/ : Invalid capture group name",
                    source
                ));
            }
            name.push(c);
            i += 1;
        }
    }

    if i >= len || chars[i] != '>' {
        return Err(format!(
            "Invalid regular expression: /{}/ : Invalid capture group name",
            source
        ));
    }
    i += 1; // skip '>'

    Ok((name, i))
}

fn parse_unicode_escape_in_name(
    chars: &[char],
    start: usize,
    source: &str,
) -> Result<(u32, usize), String> {
    let len = chars.len();
    let mut i = start + 2; // skip \u
    if i >= len {
        return Err(format!(
            "Invalid regular expression: /{}/ : Invalid capture group name",
            source
        ));
    }
    if chars[i] == '{' {
        i += 1;
        let hex_start = i;
        while i < len && chars[i] != '}' {
            if !chars[i].is_ascii_hexdigit() {
                return Err(format!(
                    "Invalid regular expression: /{}/ : Invalid capture group name",
                    source
                ));
            }
            i += 1;
        }
        if i >= len {
            return Err(format!(
                "Invalid regular expression: /{}/ : Invalid capture group name",
                source
            ));
        }
        let hex: String = chars[hex_start..i].iter().collect();
        i += 1; // skip }
        let cp = u32::from_str_radix(&hex, 16).map_err(|_| {
            format!(
                "Invalid regular expression: /{}/ : Invalid capture group name",
                source
            )
        })?;
        if cp > 0x10FFFF {
            return Err(format!(
                "Invalid regular expression: /{}/ : Invalid capture group name",
                source
            ));
        }
        Ok((cp, i))
    } else {
        // \uHHHH - exactly 4 hex digits
        if i + 4 > len {
            return Err(format!(
                "Invalid regular expression: /{}/ : Invalid capture group name",
                source
            ));
        }
        for ch in &chars[i..i + 4] {
            if !ch.is_ascii_hexdigit() {
                return Err(format!(
                    "Invalid regular expression: /{}/ : Invalid capture group name",
                    source
                ));
            }
        }
        let hex: String = chars[i..i + 4].iter().collect();
        let cp = u32::from_str_radix(&hex, 16).unwrap();
        i += 4;
        // Check for surrogate pair
        if (0xD800..=0xDBFF).contains(&cp) && i + 2 < len && chars[i] == '\\' && chars[i + 1] == 'u'
        {
            let trail_start = i + 2;
            if trail_start + 4 <= len
                && chars[trail_start..trail_start + 4]
                    .iter()
                    .all(|c| c.is_ascii_hexdigit())
            {
                let trail_hex: String = chars[trail_start..trail_start + 4].iter().collect();
                let trail = u32::from_str_radix(&trail_hex, 16).unwrap();
                if (0xDC00..=0xDFFF).contains(&trail) {
                    let combined = 0x10000 + ((cp - 0xD800) << 10) + (trail - 0xDC00);
                    return Ok((combined, trail_start + 4));
                }
            }
            // Lone lead surrogate
            return Err(format!(
                "Invalid regular expression: /{}/ : Invalid capture group name",
                source
            ));
        }
        if (0xDC00..=0xDFFF).contains(&cp) {
            return Err(format!(
                "Invalid regular expression: /{}/ : Invalid capture group name",
                source
            ));
        }
        Ok((cp, i))
    }
}

fn is_v_flag_reserved_double_punctuator(a: char, b: char) -> bool {
    a == b
        && matches!(
            a,
            '!' | '#'
                | '$'
                | '%'
                | '*'
                | '+'
                | ','
                | '.'
                | ':'
                | ';'
                | '<'
                | '='
                | '>'
                | '?'
                | '@'
                | '`'
                | '~'
                | '^'
                | '&'
        )
}

const MAX_V_FLAG_CLASS_NESTING_DEPTH: usize = 256;

fn validate_v_flag_class_inner(
    chars: &[char],
    i: &mut usize,
    source: &str,
    negated: bool,
    depth: usize,
) -> Result<(), String> {
    if depth > MAX_V_FLAG_CLASS_NESTING_DEPTH {
        return Err(format!(
            "Invalid regular expression: /{}/ : Character class nesting too deep",
            source
        ));
    }

    let len = chars.len();
    let err = |msg: &str| -> Result<(), String> {
        Err(format!(
            "Invalid regular expression: /{}/ : {}",
            source, msg
        ))
    };

    let mut has_operand = false;

    while *i < len {
        let c = chars[*i];

        if c == ']' {
            *i += 1;
            return Ok(());
        }

        // Check for reserved double punctuators
        if *i + 1 < len {
            let next = chars[*i + 1];
            if c == '-' && next == '-' {
                if !has_operand {
                    return err("Invalid set operation");
                }
                *i += 2;
                has_operand = false;
                continue;
            }
            if c == '&' && next == '&' {
                if !has_operand {
                    return err("Invalid set operation");
                }
                *i += 2;
                has_operand = false;
                continue;
            }
            if is_v_flag_reserved_double_punctuator(c, next) {
                return err("Invalid character in character class");
            }
        }

        if matches!(c, '(' | ')' | '{' | '}' | '/' | '|') {
            return err("Invalid character in character class");
        }
        if c == '[' {
            *i += 1;
            let nested_negated = if *i < len && chars[*i] == '^' {
                *i += 1;
                true
            } else {
                false
            };
            validate_v_flag_class_inner(chars, i, source, nested_negated, depth + 1)?;
            has_operand = true;
            continue;
        }
        if c == '-' {
            // In v-flag mode, a single `-` is only valid as a range separator (e.g. a-z).
            // It must appear between two class atoms (has_operand) and be followed by a valid char.
            if has_operand && *i + 1 < len && chars[*i + 1] != ']' && chars[*i + 1] != '-' {
                *i += 1;
                // The next character is the range end - it will be consumed in the next iteration
                continue;
            }
            return err("Invalid character in character class");
        }

        if c == '\\' {
            if *i + 1 >= len {
                return err("Invalid escape at end of pattern");
            }
            let after = chars[*i + 1];
            *i += 2;
            if after == 'u' {
                if *i < len && chars[*i] == '{' {
                    *i += 1;
                    while *i < len && chars[*i] != '}' {
                        *i += 1;
                    }
                    if *i < len {
                        *i += 1;
                    }
                } else {
                    let mut count = 0;
                    while count < 4 && *i < len && chars[*i].is_ascii_hexdigit() {
                        *i += 1;
                        count += 1;
                    }
                }
            } else if after == 'x' {
                let mut count = 0;
                while count < 2 && *i < len && chars[*i].is_ascii_hexdigit() {
                    *i += 1;
                    count += 1;
                }
            } else if (after == 'p' || after == 'P') && *i < len && chars[*i] == '{' {
                let prop_start = *i + 1;
                *i += 1;
                while *i < len && chars[*i] != '}' {
                    *i += 1;
                }
                if *i < len {
                    let prop_content: String = chars[prop_start..*i].iter().collect();
                    *i += 1;
                    let prop_name = if let Some(eq_pos) = prop_content.find('=') {
                        &prop_content[eq_pos + 1..]
                    } else {
                        &prop_content
                    };
                    let is_string_prop =
                        crate::emoji_strings::lookup_string_property(prop_name).is_some();
                    if is_string_prop && negated && after == 'p' {
                        return err("Invalid property name");
                    }
                }
            } else if after == 'q' && *i < len && chars[*i] == '{' {
                *i += 1;
                while *i < len && chars[*i] != '}' {
                    *i += 1;
                }
                if *i < len {
                    *i += 1;
                }
            } else if after == 'c' && *i < len && chars[*i].is_ascii_alphabetic() {
                *i += 1;
            }
            has_operand = true;
            continue;
        }

        if c == '^' && *i + 1 < len && chars[*i + 1] == '^' {
            return err("Invalid character in character class");
        }
        *i += 1;
        has_operand = true;
    }

    err("Unterminated character class")
}

pub(crate) fn validate_js_pattern(source: &str, flags: &str) -> Result<(), String> {
    let unicode = flags.contains('u') || flags.contains('v');
    let v_flag = flags.contains('v');
    let chars: Vec<char> = source.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut has_atom = false;

    // Pre-count capturing groups for backreference validation in unicode mode
    let total_capturing_groups = if unicode {
        let mut count = 0u32;
        let mut j = 0;
        let mut in_class = false;
        while j < len {
            if chars[j] == '\\' {
                j += 2;
                continue;
            }
            if chars[j] == '[' {
                in_class = true;
                j += 1;
                continue;
            }
            if chars[j] == ']' {
                in_class = false;
                j += 1;
                continue;
            }
            if !in_class && chars[j] == '(' && j + 1 < len && chars[j + 1] != '?' {
                count += 1;
            }
            if !in_class
                && chars[j] == '('
                && j + 2 < len
                && chars[j + 1] == '?'
                && chars[j + 2] == '<'
                && j + 3 < len
                && chars[j + 3] != '='
                && chars[j + 3] != '!'
            {
                count += 1;
            }
            j += 1;
        }
        count
    } else {
        0
    };

    // Track named groups: (name, group_depth, alternative_id at that depth)
    let mut named_groups: Vec<(String, usize, usize)> = Vec::new();
    // Track \k<name> backreferences for later validation
    let mut backref_names: Vec<String> = Vec::new();
    // Track group depth and alternative IDs per depth
    let mut group_depth: usize = 0;
    // alt_ids[depth] = current alternative counter at that depth
    let mut alt_ids: Vec<usize> = vec![0];
    let mut has_any_named_group = false;
    let mut has_bare_k_escape = false;
    let mut has_incomplete_backref = false;
    // Track assertion groups: 0 = non-assertion, 1 = lookahead, 2 = lookbehind
    let mut group_kind_stack: Vec<u8> = Vec::new();

    while i < len {
        let c = chars[i];

        if c == '\\' {
            if i + 1 >= len {
                return Err(format!(
                    "Invalid regular expression: /{}/: \\ at end of pattern",
                    source
                ));
            }
            let after_escape = chars[i + 1];
            i += 2;
            has_atom = true;

            if after_escape == 'k' {
                if i < len && chars[i] == '<' {
                    let save_pos = i;
                    i += 1; // skip '<'
                    if i < len && chars[i] != '>' {
                        match parse_regexp_group_name(&chars, i, source, unicode) {
                            Ok((name, new_i)) => {
                                backref_names.push(name);
                                i = new_i;
                            }
                            Err(_) => {
                                // Invalid/incomplete \k<...> — rewind to after \k
                                // so the main loop can still parse named groups that follow.
                                has_incomplete_backref = true;
                                i = save_pos + 1; // position after '<'
                            }
                        }
                    } else {
                        // \k<> (empty name)
                        has_incomplete_backref = true;
                        if i < len && chars[i] == '>' {
                            i += 1;
                        }
                    }
                } else {
                    has_bare_k_escape = true;
                }
                continue;
            }

            if after_escape == 'x' {
                if i < len && chars[i].is_ascii_hexdigit() {
                    i += 1;
                    if i < len && chars[i].is_ascii_hexdigit() {
                        i += 1;
                    } else if unicode {
                        return Err(format!(
                            "Invalid regular expression: /{}/ : Invalid escape",
                            source
                        ));
                    }
                } else if unicode {
                    return Err(format!(
                        "Invalid regular expression: /{}/ : Invalid escape",
                        source
                    ));
                }
            } else if after_escape == 'u' {
                if unicode && i < len && chars[i] == '{' {
                    i += 1;
                    let hex_start = i;
                    while i < len && chars[i] != '}' {
                        if unicode && !chars[i].is_ascii_hexdigit() {
                            return Err(format!(
                                "Invalid regular expression: /{}/ : Invalid Unicode escape",
                                source
                            ));
                        }
                        i += 1;
                    }
                    if i >= len {
                        if unicode {
                            return Err(format!(
                                "Invalid regular expression: /{}/ : Invalid Unicode escape",
                                source
                            ));
                        }
                    } else {
                        if unicode {
                            let hex_str: String = chars[hex_start..i].iter().collect();
                            if hex_str.is_empty() {
                                return Err(format!(
                                    "Invalid regular expression: /{}/ : Invalid Unicode escape",
                                    source
                                ));
                            }
                            if let Ok(cp) = u64::from_str_radix(&hex_str, 16) {
                                if cp > 0x10FFFF {
                                    return Err(format!(
                                        "Invalid regular expression: /{}/ : Invalid Unicode escape",
                                        source
                                    ));
                                }
                            } else {
                                return Err(format!(
                                    "Invalid regular expression: /{}/ : Invalid Unicode escape",
                                    source
                                ));
                            }
                        }
                        i += 1;
                    }
                } else {
                    let mut count = 0;
                    while count < 4 && i < len && chars[i].is_ascii_hexdigit() {
                        i += 1;
                        count += 1;
                    }
                    if unicode && count < 4 {
                        return Err(format!(
                            "Invalid regular expression: /{}/ : Invalid Unicode escape",
                            source
                        ));
                    }
                }
            } else if after_escape == 'c' {
                if i < len && chars[i].is_ascii_alphabetic() {
                    i += 1;
                } else if unicode {
                    return Err(format!(
                        "Invalid regular expression: /{}/ : Invalid escape",
                        source
                    ));
                }
            } else if (after_escape == 'p' || after_escape == 'P') && i < len && chars[i] == '{' {
                let start = i + 1;
                let mut end = start;
                while end < len && chars[end] != '}' {
                    end += 1;
                }
                if end < len {
                    if unicode {
                        let content: String = chars[start..end].iter().collect();
                        validate_unicode_property_escape(&content).map_err(|_| {
                            format!(
                                "Invalid regular expression: /{}/ : Invalid property name",
                                source
                            )
                        })?;
                        // Check if this is a property-of-strings
                        let prop_name = if let Some(eq_pos) = content.find('=') {
                            &content[eq_pos + 1..]
                        } else {
                            &content
                        };
                        let is_string_prop =
                            crate::emoji_strings::lookup_string_property(prop_name).is_some();
                        if is_string_prop {
                            if !v_flag {
                                // \p{StringProp}/u — only valid with v flag
                                return Err(format!(
                                    "Invalid regular expression: /{}/ : Invalid property name",
                                    source
                                ));
                            }
                            if after_escape == 'P' {
                                // \P{StringProp}/v — negation of string property forbidden
                                return Err(format!(
                                    "Invalid regular expression: /{}/ : Invalid property name",
                                    source
                                ));
                            }
                        }
                    }
                    i = end + 1;
                } else {
                    i = end;
                }
            } else if unicode {
                // In unicode mode, only specific escape sequences are valid.
                // Backreferences \1-\9 are only valid if the referenced group exists.
                if ('1'..='9').contains(&after_escape) {
                    // Parse the full decimal escape number
                    let mut num = (after_escape as u32) - ('0' as u32);
                    while i < len && chars[i].is_ascii_digit() {
                        num = num * 10 + (chars[i] as u32 - '0' as u32);
                        i += 1;
                    }
                    if num > total_capturing_groups {
                        return Err(format!(
                            "Invalid regular expression: /{}/ : Invalid escape",
                            source
                        ));
                    }
                } else {
                    let valid = matches!(
                        after_escape,
                        'd' | 'D'
                            | 'w'
                            | 'W'
                            | 's'
                            | 'S'
                            | 'b'
                            | 'B'
                            | 'n'
                            | 'r'
                            | 't'
                            | 'f'
                            | 'v'
                            | '0'
                            | 'k'
                            | '^'
                            | '$'
                            | '\\'
                            | '.'
                            | '*'
                            | '+'
                            | '?'
                            | '('
                            | ')'
                            | '['
                            | ']'
                            | '{'
                            | '}'
                            | '|'
                            | '/'
                    );
                    if !valid {
                        return Err(format!(
                            "Invalid regular expression: /{}/ : Invalid escape",
                            source
                        ));
                    }
                    // In unicode mode, \0 must not be followed by another digit (no octal)
                    if after_escape == '0' && i < len && chars[i].is_ascii_digit() {
                        return Err(format!(
                            "Invalid regular expression: /{}/ : Invalid escape",
                            source
                        ));
                    }
                }
            }
            continue;
        }

        if c == '[' {
            i += 1;
            let class_negated = if i < len && chars[i] == '^' {
                i += 1;
                true
            } else {
                false
            };

            if v_flag {
                validate_v_flag_class_inner(&chars, &mut i, source, class_negated, 0)?;
            } else {
                let mut prev_value: Option<u32> = None;
                let mut prev_is_class_escape = false;
                let mut expecting_range_end = false;

                while i < len && chars[i] != ']' {
                    if unicode && chars[i] == '\\' && i + 1 < len {
                        let esc_char = chars[i + 1];
                        if esc_char == 'x' {
                            if !(i + 3 < len
                                && chars[i + 2].is_ascii_hexdigit()
                                && chars[i + 3].is_ascii_hexdigit())
                            {
                                return Err(format!(
                                    "Invalid regular expression: /{}/ : Invalid escape",
                                    source
                                ));
                            }
                        } else if esc_char == 'u' {
                            if i + 2 < len && chars[i + 2] == '{' {
                                let hex_start = i + 3;
                                let mut j = hex_start;
                                while j < len && chars[j] != '}' {
                                    if !chars[j].is_ascii_hexdigit() {
                                        return Err(format!(
                                            "Invalid regular expression: /{}/ : Invalid Unicode escape",
                                            source
                                        ));
                                    }
                                    j += 1;
                                }
                                if j >= len {
                                    return Err(format!(
                                        "Invalid regular expression: /{}/ : Invalid Unicode escape",
                                        source
                                    ));
                                }
                                let hex_str: String = chars[hex_start..j].iter().collect();
                                if hex_str.is_empty() {
                                    return Err(format!(
                                        "Invalid regular expression: /{}/ : Invalid Unicode escape",
                                        source
                                    ));
                                }
                                if let Ok(cp) = u64::from_str_radix(&hex_str, 16)
                                    && cp > 0x10FFFF
                                {
                                    return Err(format!(
                                        "Invalid regular expression: /{}/ : Invalid Unicode escape",
                                        source
                                    ));
                                }
                            } else {
                                let mut count = 0;
                                let mut j = i + 2;
                                while count < 4 && j < len && chars[j].is_ascii_hexdigit() {
                                    j += 1;
                                    count += 1;
                                }
                                if count < 4 {
                                    return Err(format!(
                                        "Invalid regular expression: /{}/ : Invalid Unicode escape",
                                        source
                                    ));
                                }
                            }
                        } else if esc_char == 'c' {
                            if !(i + 2 < len && chars[i + 2].is_ascii_alphabetic()) {
                                return Err(format!(
                                    "Invalid regular expression: /{}/ : Invalid escape",
                                    source
                                ));
                            }
                        } else if esc_char == '0' {
                            if i + 2 < len && chars[i + 2].is_ascii_digit() {
                                return Err(format!(
                                    "Invalid regular expression: /{}/ : Invalid escape",
                                    source
                                ));
                            }
                        } else {
                            let is_p_without_brace = (esc_char == 'p' || esc_char == 'P')
                                && !(i + 2 < len && chars[i + 2] == '{');
                            let is_invalid_identity = !matches!(
                                esc_char,
                                'd' | 'D'
                                    | 'w'
                                    | 'W'
                                    | 's'
                                    | 'S'
                                    | 'b'
                                    | 'n'
                                    | 'r'
                                    | 't'
                                    | 'f'
                                    | 'v'
                                    | '0'
                                    | 'c'
                                    | 'x'
                                    | 'u'
                                    | 'p'
                                    | 'P'
                                    | '^'
                                    | '$'
                                    | '\\'
                                    | '.'
                                    | '*'
                                    | '+'
                                    | '?'
                                    | '('
                                    | ')'
                                    | '['
                                    | ']'
                                    | '{'
                                    | '}'
                                    | '|'
                                    | '/'
                                    | '-'
                            );
                            if is_p_without_brace || is_invalid_identity {
                                return Err(format!(
                                    "Invalid regular expression: /{}/ : Invalid escape",
                                    source
                                ));
                            }
                        }
                    }

                    if chars[i] == '-' && !expecting_range_end {
                        if (prev_value.is_some() || prev_is_class_escape)
                            && i + 1 < len
                            && chars[i + 1] != ']'
                        {
                            if unicode && prev_is_class_escape {
                                return Err(format!(
                                    "Invalid regular expression: /{}/ : Invalid class escape in range",
                                    source
                                ));
                            }
                            expecting_range_end = true;
                            i += 1;
                            continue;
                        }
                        prev_value = Some('-' as u32);
                        prev_is_class_escape = false;
                        i += 1;
                        continue;
                    }

                    let save_i = i;
                    let val = resolve_class_escape(&chars, &mut i);
                    let is_class_esc = val.is_none()
                        && save_i < len
                        && chars[save_i] == '\\'
                        && save_i + 1 < len
                        && (matches!(chars[save_i + 1], 'd' | 'D' | 'w' | 'W' | 's' | 'S')
                            || (matches!(chars[save_i + 1], 'p' | 'P')
                                && save_i + 2 < len
                                && chars[save_i + 2] == '{'));

                    if expecting_range_end {
                        expecting_range_end = false;
                        if unicode && is_class_esc {
                            return Err(format!(
                                "Invalid regular expression: /{}/ : Invalid class escape in range",
                                source
                            ));
                        }
                        if let (Some(start_val), Some(end_val)) = (prev_value, val) {
                            if start_val > end_val {
                                return Err(format!(
                                    "Invalid regular expression: /{}/ : Range out of order in character class",
                                    source
                                ));
                            }
                            // Valid range: both endpoints are consumed, neither
                            // is available as the start of a new range.
                            prev_value = None;
                            prev_is_class_escape = false;
                        } else {
                            // Degenerate range (class escape endpoint in non-unicode Annex B):
                            // the end atom remains a standalone literal.
                            prev_value = val;
                            prev_is_class_escape = is_class_esc;
                        }
                        continue;
                    }

                    prev_value = val;
                    prev_is_class_escape = is_class_esc;
                    if i == save_i {
                        i += 1;
                    }
                }

                if i < len {
                    i += 1; // skip ']'
                } else if unicode {
                    return Err(format!(
                        "Invalid regular expression: /{}/ : Unterminated character class",
                        source
                    ));
                }
            }
            has_atom = true;
            continue;
        }

        if c == '(' {
            i += 1;
            let mut kind: u8 = 0; // 0=non-assertion, 1=lookahead, 2=lookbehind
            if i < len && chars[i] == '?' {
                i += 1;
                if i < len {
                    match chars[i] {
                        ':' => {
                            i += 1;
                        }
                        '=' | '!' => {
                            kind = 1; // lookahead
                            i += 1;
                        }
                        '<' if i + 1 < len && (chars[i + 1] == '=' || chars[i + 1] == '!') => {
                            kind = 2; // lookbehind
                            i += 2;
                        }
                        '<' if i + 1 < len && chars[i + 1] != '=' && chars[i + 1] != '!' => {
                            i += 1; // skip '<'
                            let (name, new_i) =
                                parse_regexp_group_name(&chars, i, source, unicode)?;
                            has_any_named_group = true;
                            let current_alt = if group_depth < alt_ids.len() {
                                alt_ids[group_depth]
                            } else {
                                0
                            };
                            named_groups.push((name, group_depth, current_alt));
                            i = new_i;
                        }
                        '<' => {
                            // (?< at end of pattern
                            return Err(format!(
                                "Invalid regular expression: /{}/ : Invalid capture group name",
                                source
                            ));
                        }
                        _ => {
                            validate_modifier_group(&chars, i, source)?;
                        }
                    }
                }
            }
            group_kind_stack.push(kind);
            group_depth += 1;
            while alt_ids.len() <= group_depth {
                alt_ids.push(0);
            }
            has_atom = false;
            continue;
        }

        if c == ')' {
            if group_kind_stack.is_empty() && unicode {
                return Err(format!(
                    "Invalid regular expression: /{}/ : Unmatched ')'",
                    source
                ));
            }
            i += 1;
            group_depth = group_depth.saturating_sub(1);
            let kind = group_kind_stack.pop().unwrap_or(0);
            if kind > 0 {
                // Assertion group: check if next is a quantifier
                // Lookbehinds (kind=2): always reject quantifiers
                // Lookaheads (kind=1): reject in unicode mode only (Annex B allows in non-unicode)
                let reject = kind == 2 || unicode;
                if reject {
                    if i < len {
                        let qc = chars[i];
                        if qc == '*' || qc == '+' || qc == '?' {
                            return Err(format!(
                                "Invalid regular expression: /{}/ : Nothing to repeat",
                                source
                            ));
                        }
                        if qc == '{' {
                            let mut j = i + 1;
                            let mut is_quant = false;
                            while j < len {
                                if chars[j] == '}' {
                                    is_quant = true;
                                    break;
                                }
                                if !chars[j].is_ascii_digit() && chars[j] != ',' {
                                    break;
                                }
                                j += 1;
                            }
                            if is_quant {
                                return Err(format!(
                                    "Invalid regular expression: /{}/ : Nothing to repeat",
                                    source
                                ));
                            }
                        }
                    }
                    has_atom = false;
                } else {
                    // Non-unicode lookahead: allowed as Annex B QuantifiableAssertion
                    has_atom = true;
                }
            } else {
                has_atom = true;
            }
            continue;
        }

        if c == '|' {
            i += 1;
            if group_depth < alt_ids.len() {
                alt_ids[group_depth] += 1;
            }
            has_atom = false;
            continue;
        }

        // Quantifier validation: detect quantifier without preceding atom, and double quantifiers
        if c == '*' || c == '+' || c == '?' || c == '{' {
            if !has_atom {
                // Check if '{' is really a quantifier
                if c == '{' {
                    let mut j = i + 1;
                    let mut is_quant = false;
                    while j < len {
                        if chars[j] == '}' {
                            is_quant = true;
                            break;
                        }
                        if !chars[j].is_ascii_digit() && chars[j] != ',' {
                            break;
                        }
                        j += 1;
                    }
                    if !is_quant {
                        if unicode {
                            return Err(format!(
                                "Invalid regular expression: /{}/ : Lone quantifier brackets",
                                source
                            ));
                        }
                        has_atom = true;
                        i += 1;
                        continue;
                    }
                }
                return Err(format!(
                    "Invalid regular expression: /{}/ : Nothing to repeat",
                    source
                ));
            }
            let mut quant_end = i + 1;
            if c == '{' {
                // Find matching }
                let brace_start = i;
                let mut j = i + 1;
                let mut found_close = false;
                while j < len {
                    if chars[j] == '}' {
                        found_close = true;
                        quant_end = j + 1;
                        break;
                    }
                    if !chars[j].is_ascii_digit() && chars[j] != ',' {
                        break;
                    }
                    j += 1;
                }
                if !found_close {
                    if unicode {
                        return Err(format!(
                            "Invalid regular expression: /{}/ : Lone quantifier brackets",
                            source
                        ));
                    }
                    i += 1;
                    continue;
                }
                // Validate the quantifier content
                let inner: String = chars[brace_start + 1..j].iter().collect();
                let parts: Vec<&str> = inner.split(',').collect();
                let valid = match parts.len() {
                    1 => parts[0].parse::<u64>().is_ok(),
                    2 => {
                        let a_ok = parts[0].parse::<u64>().is_ok();
                        let b_ok = parts[1].is_empty() || parts[1].parse::<u64>().is_ok();
                        if a_ok && b_ok && !parts[1].is_empty() {
                            // Check min <= max
                            if let (Ok(min_val), Ok(max_val)) =
                                (parts[0].parse::<u64>(), parts[1].parse::<u64>())
                                && max_val < min_val
                            {
                                return Err(format!(
                                    "Invalid regular expression: /{}/ : numbers out of order in {{}} quantifier",
                                    source
                                ));
                            }
                        }
                        a_ok && b_ok
                    }
                    _ => false,
                };
                if !valid {
                    if unicode {
                        return Err(format!(
                            "Invalid regular expression: /{}/ : Lone quantifier brackets",
                            source
                        ));
                    }
                    i += 1;
                    continue;
                }
            }

            // After quantifier, check if '?' follows (lazy modifier) — that's OK
            if quant_end < len && chars[quant_end] == '?' {
                quant_end += 1;
            }
            // Now check if another quantifier follows (double quantifier = error)
            if quant_end < len {
                let next_c = chars[quant_end];
                if next_c == '*' || next_c == '+' || next_c == '?' {
                    return Err(format!(
                        "Invalid regular expression: /{}/ : Nothing to repeat",
                        source
                    ));
                }
                if next_c == '{' {
                    // Check if it's a valid quantifier
                    let mut k = quant_end + 1;
                    let mut has_close = false;
                    while k < len {
                        if chars[k] == '}' {
                            has_close = true;
                            break;
                        }
                        if !chars[k].is_ascii_digit() && chars[k] != ',' {
                            break;
                        }
                        k += 1;
                    }
                    if has_close {
                        let inner: String = chars[quant_end + 1..k].iter().collect();
                        let parts: Vec<&str> = inner.split(',').collect();
                        let valid = match parts.len() {
                            1 => parts[0].parse::<u64>().is_ok(),
                            2 => {
                                let a_ok = parts[0].parse::<u64>().is_ok();
                                let b_ok = parts[1].is_empty() || parts[1].parse::<u64>().is_ok();
                                a_ok && b_ok
                            }
                            _ => false,
                        };
                        if valid {
                            return Err(format!(
                                "Invalid regular expression: /{}/ : Nothing to repeat",
                                source
                            ));
                        }
                    }
                }
            }
            i = quant_end;
            has_atom = false;
            continue;
        }

        if unicode && (c == '}' || c == ']') {
            return Err(format!(
                "Invalid regular expression: /{}/ : Lone quantifier brackets",
                source
            ));
        }

        has_atom = true;
        i += 1;
    }

    // Check for duplicate named groups
    // Two groups with the same name are only allowed if they cannot both participate
    // (i.e., they must be in different alternatives at the same depth level)
    for i_idx in 0..named_groups.len() {
        for j_idx in (i_idx + 1)..named_groups.len() {
            let (ref name_a, depth_a, alt_a) = named_groups[i_idx];
            let (ref name_b, depth_b, alt_b) = named_groups[j_idx];
            if name_a == name_b {
                // Duplicate names are only OK if they are in different alternatives
                // at the SAME group depth level (different branches of the same |)
                if depth_a != depth_b || alt_a == alt_b {
                    return Err(format!(
                        "Invalid regular expression: /{}/ : Duplicate capture group name",
                        source
                    ));
                }
            }
        }
    }

    // \k without < is an error if the pattern has named groups or is in unicode mode
    if has_bare_k_escape && (unicode || has_any_named_group) {
        return Err(format!(
            "Invalid regular expression: /{}/ : Invalid escape",
            source
        ));
    }

    // \k<name without closing > is an error if pattern has named groups or in unicode mode
    if has_incomplete_backref && (unicode || has_any_named_group) {
        return Err(format!(
            "Invalid regular expression: /{}/ : Invalid named reference",
            source
        ));
    }

    // Check for dangling \k<name> backreferences
    if !backref_names.is_empty() {
        let defined_names: HashSet<&str> =
            named_groups.iter().map(|(n, _, _)| n.as_str()).collect();
        for bref in &backref_names {
            if !defined_names.contains(bref.as_str()) && (unicode || has_any_named_group) {
                return Err(format!(
                    "Invalid regular expression: /{}/ : Invalid named reference",
                    source
                ));
            }
        }
    }

    // Check for unclosed groups
    if !group_kind_stack.is_empty() {
        return Err(format!(
            "Invalid regular expression: /{}/ : Unterminated group",
            source
        ));
    }

    // Validate property escapes by running translate_js_pattern
    if unicode {
        translate_js_pattern(source, flags)?;
    }

    Ok(())
}

enum CompiledRegex {
    Fancy(fancy_regex::Regex),
    Standard(regex::Regex),
    Bytes(regex::bytes::Regex),
    FancyWithCustomLookbehind {
        outer_regex: fancy_regex::Regex,
        lookbehinds: Vec<super::regexp_lookbehind::LookbehindInfo>,
        flags: String,
        total_groups: usize,
        /// When lookbehind captures are referenced by external backrefs,
        /// store the remaining pattern source and backref-to-lb-capture mapping
        /// so match_with_lookbehind can do position-iteration matching.
        external_lb_backrefs: Vec<(u32, u32)>,
        remaining_source: Option<String>,
    },
}

struct RegexMatch {
    start: usize,
    end: usize,
    text: String,
}

struct RegexCaptures {
    groups: Vec<Option<RegexMatch>>,
    names: Vec<Option<String>>,
}

impl RegexCaptures {
    fn get(&self, i: usize) -> Option<&RegexMatch> {
        self.groups.get(i)?.as_ref()
    }
    fn len(&self) -> usize {
        self.groups.len()
    }
}

type DupGroupMap = HashMap<String, Vec<(String, u32)>>;

fn clear_stale_dup_captures(caps: &mut RegexCaptures, dup_map: &DupGroupMap) {
    if dup_map.is_empty() {
        return;
    }
    for variants in dup_map.values() {
        let mut matched_indices: Vec<(usize, usize)> = Vec::new();
        for (internal_name, _) in variants {
            let sanitized = sanitize_group_name(internal_name);
            for (i, name_opt) in caps.names.iter().enumerate() {
                if let Some(n) = name_opt
                    && *n == sanitized
                    && let Some(ref m) = caps.groups[i]
                {
                    matched_indices.push((i, m.start));
                }
            }
        }
        if matched_indices.len() > 1 {
            let best_idx = matched_indices
                .iter()
                .max_by_key(|(_, start)| *start)
                .unwrap()
                .0;
            for (idx, _) in &matched_indices {
                if *idx != best_idx {
                    caps.groups[*idx] = None;
                }
            }
        }
    }
}

/// Remove capture groups whose names start with `__jsse_qi` (renamed groups from
/// quantifier expansion with backrefs). These are internal and shouldn't appear
/// in the match result array or groups object.
fn strip_renamed_qi_captures(caps: &mut RegexCaptures) {
    let qi_indices: Vec<usize> = caps
        .names
        .iter()
        .enumerate()
        .filter(|(_, name_opt)| {
            name_opt
                .as_ref()
                .is_some_and(|n| n.starts_with("__jsse_qi"))
        })
        .map(|(i, _)| i)
        .collect();
    if qi_indices.is_empty() {
        return;
    }
    // Remove in reverse order to maintain valid indices
    for &idx in qi_indices.iter().rev() {
        caps.groups.remove(idx);
        caps.names.remove(idx);
    }
}

/// Reset captures from groups inside quantifiers per ES spec §22.2.2.5.1 RepeatMatcher steps 3-4.
/// When a quantified group iterates multiple times, captures from inner groups that don't
/// participate in the final iteration must be reset to undefined.
fn reset_quantifier_inner_captures(caps: &mut RegexCaptures, source: &str) {
    let (parent_map, zero_width_clears, self_quantified_min_zero, nc_quantified_branches) =
        build_quantified_parent_map(source);
    // Handle standard case: child capture outside parent capture bounds
    for (child_group, parent_group) in &parent_map {
        let child_idx = *child_group;
        let parent_idx = *parent_group;
        if child_idx >= caps.groups.len() || parent_idx >= caps.groups.len() {
            continue;
        }
        if let (Some(child_match), Some(parent_match)) =
            (&caps.groups[child_idx], &caps.groups[parent_idx])
            && (child_match.start < parent_match.start || child_match.end > parent_match.end)
        {
            caps.groups[child_idx] = None;
        }
    }
    // Handle non-capturing quantified groups with alternation.
    // Per spec §22.2.2.5.1 RepeatMatcher: captures are cleared at the start of each iteration.
    // The Rust regex engine doesn't do this, so we detect which alternation branch matched
    // in the last iteration and clear captures from all other branches.
    for branches in &nc_quantified_branches {
        // Find which branch has the highest max_end → that branch matched in the last iteration
        let mut best_branch_idx = None;
        let mut best_max_end: usize = 0;
        for (bi, branch) in branches.iter().enumerate() {
            for &ci in branch {
                if ci < caps.groups.len()
                    && let Some(ref m) = caps.groups[ci]
                    && m.end > best_max_end
                {
                    best_max_end = m.end;
                    best_branch_idx = Some(bi);
                }
            }
        }
        if let Some(best_bi) = best_branch_idx {
            for (bi, branch) in branches.iter().enumerate() {
                if bi != best_bi {
                    for &ci in branch {
                        if ci < caps.groups.len() {
                            caps.groups[ci] = None;
                        }
                    }
                }
            }
        }
    }
    // Handle zero-width quantified non-capturing groups with min=0:
    // Per spec §22.2.2.5.1 RepeatMatcher step 2.b, when min=0 and the iteration
    // matches empty string, it returns failure. For groups containing only
    // zero-width assertions, every iteration matches empty, so inner captures
    // must always be cleared.
    for child_idx in &zero_width_clears {
        if *child_idx < caps.groups.len() {
            caps.groups[*child_idx] = None;
        }
    }
    // Handle self-quantified capturing groups with min=0 that matched empty.
    // Per spec §22.2.2.5.1 RepeatMatcher step 2.b: when min=0 and the atom
    // matched empty, the quantifier succeeds without consuming and the capture
    // should be undefined. The regex crate reports these as Some("") matches
    // but ES spec requires undefined.
    for cap_idx in &self_quantified_min_zero {
        if *cap_idx < caps.groups.len()
            && let Some(ref m) = caps.groups[*cap_idx]
            && m.start == m.end
        {
            caps.groups[*cap_idx] = None;
        }
    }
}

/// Parse regex source to build a map: child_group_number -> nearest_quantified_ancestor_group_number.
/// A "quantified group" is a capturing or non-capturing group that is followed by *, +, ?, or {n,m}.
/// We track the nesting of groups and which groups are quantified to build this mapping.
/// Returns (parent_map, zero_width_clears):
/// - parent_map: Vec<(child_capture, parent_capture)> for bounds-based clearing
/// - zero_width_clears: Vec<child_capture> for captures inside zero-width
///   quantified groups with min=0 that must always be cleared
fn build_quantified_parent_map(
    source: &str,
) -> (
    Vec<(usize, usize)>,
    Vec<usize>,
    Vec<usize>,
    Vec<Vec<Vec<usize>>>,
) {
    let chars: Vec<char> = source.chars().collect();
    let len = chars.len();
    let mut result = Vec::new();
    let mut zero_width_clears: Vec<usize> = Vec::new();
    let mut self_quantified_min_zero: Vec<usize> = Vec::new();
    let mut nc_quantified_branches: Vec<Vec<Vec<usize>>> = Vec::new();

    // First pass: identify all groups (capturing and non-capturing), their nesting, and which are quantified
    #[derive(Clone)]
    struct GroupInfo {
        capture_index: Option<usize>, // None for non-capturing groups
        children: Vec<usize>,         // indices into groups_info
        start_pos: usize,             // position of '(' in source
        end_pos: usize,               // position of ')' in source
        is_quantified: bool,
        quantifier_min_zero: bool, // true if quantifier has min=0 (*, ?, {0,...})
    }

    let mut groups_info: Vec<GroupInfo> = Vec::new();
    let mut group_stack: Vec<usize> = Vec::new(); // indices into groups_info
    let mut capture_count: usize = 0;
    let mut i = 0;
    let mut in_char_class = false;

    while i < len {
        if in_char_class {
            if chars[i] == '\\' && i + 1 < len {
                i += 2;
            } else if chars[i] == ']' {
                in_char_class = false;
                i += 1;
            } else {
                i += 1;
            }
            continue;
        }

        if chars[i] == '[' {
            in_char_class = true;
            i += 1;
            continue;
        }

        if chars[i] == '\\' && i + 1 < len {
            i += 2;
            continue;
        }

        if chars[i] == '(' {
            let is_capturing = !(i + 1 < len && chars[i + 1] == '?');
            let cap_idx = if is_capturing {
                capture_count += 1;
                Some(capture_count)
            } else {
                None
            };
            let group_idx = groups_info.len();
            groups_info.push(GroupInfo {
                capture_index: cap_idx,
                children: Vec::new(),
                start_pos: i,
                end_pos: 0,
                is_quantified: false,
                quantifier_min_zero: false,
            });
            if let Some(&parent_idx) = group_stack.last() {
                groups_info[parent_idx].children.push(group_idx);
            }
            group_stack.push(group_idx);
            i += 1;
            continue;
        }

        if chars[i] == ')' {
            if let Some(group_idx) = group_stack.pop() {
                groups_info[group_idx].end_pos = i;
                // Check if followed by a quantifier
                let next = i + 1;
                if next < len
                    && (chars[next] == '*'
                        || chars[next] == '+'
                        || chars[next] == '?'
                        || chars[next] == '{')
                {
                    groups_info[group_idx].is_quantified = true;
                    let min_zero = match chars[next] {
                        '*' | '?' => true,
                        '{' => {
                            // Parse {N,...} to check if N == 0
                            let mut k = next + 1;
                            let ns = k;
                            while k < len && chars[k].is_ascii_digit() {
                                k += 1;
                            }
                            if k > ns {
                                let n: u32 =
                                    chars[ns..k].iter().collect::<String>().parse().unwrap_or(1);
                                n == 0
                            } else {
                                false
                            }
                        }
                        _ => false, // '+'
                    };
                    groups_info[group_idx].quantifier_min_zero = min_zero;
                }
            }
            i += 1;
            continue;
        }

        i += 1;
    }

    // Check if a group's content (between start_pos and end_pos) is zero-width-only
    fn is_group_zero_width_only(chars: &[char], group: &GroupInfo) -> bool {
        // Content starts after '(' plus any group prefix like '?:', '?=', '?!', '?<=' etc
        let start = group.start_pos + 1;
        let end = group.end_pos;
        let mut i = start;
        // Skip group prefix
        if i < end && chars[i] == '?' {
            i += 1;
            if i < end {
                match chars[i] {
                    ':' => {
                        i += 1;
                    }
                    '=' | '!' => return true, // lookahead is zero-width
                    '<' if i + 1 < end && (chars[i + 1] == '=' || chars[i + 1] == '!') => {
                        return true; // lookbehind is zero-width
                    }
                    '<' => {
                        // Named group (?<name>...) — skip name
                        while i < end && chars[i] != '>' {
                            i += 1;
                        }
                        if i < end {
                            i += 1;
                        }
                    }
                    _ => {}
                }
            }
        }
        // Check remaining content — must be only assertions/zero-width elements
        while i < end {
            match chars[i] {
                '\\' if i + 1 < end => {
                    let c2 = chars[i + 1];
                    if c2 == 'b' || c2 == 'B' {
                        i += 2; // zero-width anchors
                    } else {
                        return false; // consuming escape
                    }
                }
                '[' => {
                    return false; // char class is consuming
                }
                '^' | '$' => {
                    i += 1;
                }
                '(' if i + 2 < end && chars[i + 1] == '?' => {
                    let c2 = chars[i + 2];
                    if c2 == '=' || c2 == '!' {
                        // Lookahead — skip to matching close
                        let mut depth = 1;
                        i += 1;
                        while i < end && depth > 0 {
                            if chars[i] == '\\' && i + 1 < end {
                                i += 2;
                                continue;
                            }
                            if chars[i] == '(' {
                                depth += 1;
                            }
                            if chars[i] == ')' {
                                depth -= 1;
                            }
                            i += 1;
                        }
                    } else if c2 == '<'
                        && i + 3 < end
                        && (chars[i + 3] == '=' || chars[i + 3] == '!')
                    {
                        // Lookbehind — skip to matching close
                        let mut depth = 1;
                        i += 1;
                        while i < end && depth > 0 {
                            if chars[i] == '\\' && i + 1 < end {
                                i += 2;
                                continue;
                            }
                            if chars[i] == '(' {
                                depth += 1;
                            }
                            if chars[i] == ')' {
                                depth -= 1;
                            }
                            i += 1;
                        }
                    } else if c2 == ':' {
                        // Non-capturing group — need to check content recursively
                        // For simplicity, find matching close and check if it's all zero-width
                        return false; // Conservative: treat as consuming
                    } else {
                        return false;
                    }
                }
                _ => return false, // Any other char is consuming
            }
        }
        true
    }

    fn collect_pairs(
        group_idx: usize,
        chars: &[char],
        groups_info: &[GroupInfo],
        ancestor_stack: &mut Vec<usize>,
        result: &mut Vec<(usize, usize)>,
        zero_width_clears: &mut Vec<usize>,
    ) {
        let children = groups_info[group_idx].children.clone();
        for &child_idx in &children {
            if let Some(cap_idx) = groups_info[child_idx].capture_index {
                for &anc in ancestor_stack.iter().rev() {
                    if groups_info[anc].is_quantified {
                        if let Some(anc_cap) = groups_info[anc].capture_index {
                            result.push((cap_idx, anc_cap));
                            break;
                        }
                        if groups_info[anc].quantifier_min_zero
                            && is_group_zero_width_only(chars, &groups_info[anc])
                        {
                            zero_width_clears.push(cap_idx);
                            break;
                        }
                    }
                }
            }
            ancestor_stack.push(child_idx);
            collect_pairs(
                child_idx,
                chars,
                groups_info,
                ancestor_stack,
                result,
                zero_width_clears,
            );
            ancestor_stack.pop();
        }
    }

    // Process all top-level groups
    let top_level: Vec<usize> = (0..groups_info.len())
        .filter(|&idx| {
            // A group is top-level if it's not a child of any other group
            !groups_info.iter().any(|g| g.children.contains(&idx))
        })
        .collect();

    for &top_idx in &top_level {
        let mut stack = vec![top_idx];
        collect_pairs(
            top_idx,
            &chars,
            &groups_info,
            &mut stack,
            &mut result,
            &mut zero_width_clears,
        );
    }

    // Collect self-quantified capturing groups with min=0.
    // e.g., (x)? or (x)* — the capturing group itself is quantified.
    for g in &groups_info {
        if let Some(cap_idx) = g.capture_index
            && g.is_quantified
            && g.quantifier_min_zero
        {
            self_quantified_min_zero.push(cap_idx);
        }
    }

    // Collect alternation branches of non-capturing quantified groups.
    // Per spec, captures are cleared at the start of each quantifier iteration.
    // We detect alternation branches and clear captures from non-last-matching branches.
    fn collect_all_capturing_children(idx: usize, groups_info: &[GroupInfo], out: &mut Vec<usize>) {
        if let Some(ci) = groups_info[idx].capture_index {
            out.push(ci);
        }
        for &child in &groups_info[idx].children {
            collect_all_capturing_children(child, groups_info, out);
        }
    }
    fn find_top_level_pipes(chars: &[char], group_start: usize, group_end: usize) -> Vec<usize> {
        let mut i = group_start + 1; // skip '('
        // Skip group prefix (?:, (?=, etc.)
        if i < group_end && chars[i] == '?' {
            i += 1;
            if i < group_end {
                match chars[i] {
                    ':' | '=' | '!' => i += 1,
                    '<' if i + 1 < group_end && (chars[i + 1] == '=' || chars[i + 1] == '!') => {
                        i += 2
                    }
                    '<' => {
                        while i < group_end && chars[i] != '>' {
                            i += 1;
                        }
                        if i < group_end {
                            i += 1;
                        }
                    }
                    _ => {}
                }
            }
        }
        let mut pipes = Vec::new();
        let mut depth = 0;
        let mut in_cc = false;
        while i < group_end {
            if in_cc {
                if chars[i] == '\\' && i + 1 < group_end {
                    i += 2;
                    continue;
                }
                if chars[i] == ']' {
                    in_cc = false;
                }
                i += 1;
                continue;
            }
            if chars[i] == '\\' && i + 1 < group_end {
                i += 2;
                continue;
            }
            if chars[i] == '[' {
                in_cc = true;
                i += 1;
                continue;
            }
            if chars[i] == '(' {
                depth += 1;
                i += 1;
                continue;
            }
            if chars[i] == ')' {
                depth -= 1;
                i += 1;
                continue;
            }
            if chars[i] == '|' && depth == 0 {
                pipes.push(i);
            }
            i += 1;
        }
        pipes
    }
    for g in &groups_info {
        if g.capture_index.is_none() && g.is_quantified {
            let pipes = find_top_level_pipes(&chars, g.start_pos, g.end_pos);
            if pipes.is_empty() {
                continue; // No alternation — regex engine overwrites correctly
            }
            // Build branch boundaries
            let content_start = {
                let mut s = g.start_pos + 1;
                if s < g.end_pos && chars[s] == '?' {
                    s += 1;
                    if s < g.end_pos {
                        match chars[s] {
                            ':' | '=' | '!' => s += 1,
                            '<' if s + 1 < g.end_pos
                                && (chars[s + 1] == '=' || chars[s + 1] == '!') =>
                            {
                                s += 2
                            }
                            '<' => {
                                while s < g.end_pos && chars[s] != '>' {
                                    s += 1;
                                }
                                if s < g.end_pos {
                                    s += 1;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                s
            };
            let mut branch_starts = vec![content_start];
            for &p in &pipes {
                branch_starts.push(p + 1);
            }
            let mut branch_ends: Vec<usize> = pipes.clone();
            branch_ends.push(g.end_pos);

            let mut branches: Vec<Vec<usize>> = Vec::new();
            for bi in 0..branch_starts.len() {
                let bs = branch_starts[bi];
                let be = branch_ends[bi];
                let mut branch_caps = Vec::new();
                for &child_idx in &g.children {
                    if groups_info[child_idx].start_pos >= bs
                        && groups_info[child_idx].end_pos <= be
                    {
                        collect_all_capturing_children(child_idx, &groups_info, &mut branch_caps);
                    }
                }
                branches.push(branch_caps);
            }
            nc_quantified_branches.push(branches);
        }
    }

    (
        result,
        zero_width_clears,
        self_quantified_min_zero,
        nc_quantified_branches,
    )
}

fn build_regex(source: &str, flags: &str) -> Result<CompiledRegex, String> {
    build_regex_ex(source, flags).map(|(re, _, _)| re)
}

fn lookbehind_needs_custom_rtl(source: &str) -> bool {
    let chars: Vec<char> = source.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut in_cc = false;

    while i < len {
        if chars[i] == '[' && !in_cc {
            in_cc = true;
            i += 1;
            continue;
        }
        if chars[i] == ']' && in_cc {
            in_cc = false;
            i += 1;
            continue;
        }
        if chars[i] == '\\' && i + 1 < len {
            i += 2;
            continue;
        }

        // Detect lookbehind
        if !in_cc
            && chars[i] == '('
            && i + 3 < len
            && chars[i + 1] == '?'
            && chars[i + 2] == '<'
            && (chars[i + 3] == '=' || chars[i + 3] == '!')
        {
            let content_start = i + 4;
            let mut depth = 1;
            let mut j = content_start;
            let mut has_backref = false;
            let mut has_quantified_capture = false;
            let mut in_cc2 = false;
            let mut capturing_stack: Vec<bool> = Vec::new();
            while j < len && depth > 0 {
                if chars[j] == '[' && !in_cc2 {
                    in_cc2 = true;
                } else if chars[j] == ']' && in_cc2 {
                    in_cc2 = false;
                } else if chars[j] == '\\' && j + 1 < len {
                    j += 1;
                    if chars[j].is_ascii_digit() && chars[j] != '0' {
                        has_backref = true;
                    }
                } else if chars[j] == '(' && !in_cc2 {
                    depth += 1;
                    let is_cap = if j + 1 < len && chars[j + 1] == '?' {
                        j + 2 < len
                            && chars[j + 2] == '<'
                            && j + 3 < len
                            && chars[j + 3] != '='
                            && chars[j + 3] != '!'
                    } else {
                        true
                    };
                    capturing_stack.push(is_cap);
                } else if chars[j] == ')' && !in_cc2 {
                    depth -= 1;
                    if depth > 0 {
                        let was_cap = capturing_stack.pop().unwrap_or(false);
                        if was_cap && j + 1 < len {
                            // Only flag variable-width quantifiers (+ and *).
                            // Fixed-count {n} and optional ? are handled correctly by fancy-regex.
                            if matches!(chars[j + 1], '+' | '*') {
                                has_quantified_capture = true;
                            }
                        }
                    }
                }
                if depth > 0 {
                    j += 1;
                }
            }

            if has_backref || has_quantified_capture {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Detect patterns where lookbehind captures are referenced by backrefs outside
/// the lookbehind. Returns verify info if so, None otherwise.
fn lookbehind_captures_with_external_backrefs(
    source: &str,
    flags: &str,
) -> Option<Vec<super::regexp_lookbehind::LookbehindVerifyInfo>> {
    let chars: Vec<char> = source.chars().collect();
    let len = chars.len();
    let mut in_cc = false;
    let mut group_count: u32 = 0;
    let mut lb_capture_groups: Vec<(u32, u32, String, bool)> = Vec::new(); // (offset, num_caps, content, positive)
    let mut external_backrefs: Vec<u32> = Vec::new();
    let mut i = 0;

    while i < len {
        if chars[i] == '[' && !in_cc {
            in_cc = true;
            i += 1;
            continue;
        }
        if chars[i] == ']' && in_cc {
            in_cc = false;
            i += 1;
            continue;
        }
        if chars[i] == '\\' && i + 1 < len {
            if !in_cc && chars[i + 1].is_ascii_digit() && chars[i + 1] != '0' {
                let mut num_str = String::new();
                let mut k = i + 1;
                while k < len && chars[k].is_ascii_digit() {
                    num_str.push(chars[k]);
                    k += 1;
                }
                if let Ok(n) = num_str.parse::<u32>() {
                    external_backrefs.push(n);
                }
            }
            i += 2;
            continue;
        }

        if !in_cc
            && chars[i] == '('
            && i + 3 < len
            && chars[i + 1] == '?'
            && chars[i + 2] == '<'
            && (chars[i + 3] == '=' || chars[i + 3] == '!')
        {
            let positive = chars[i + 3] == '=';
            let capture_offset = group_count;
            let content_start = i + 4;
            let mut depth = 1;
            let mut j = content_start;
            let mut in_cc2 = false;
            let mut num_caps: u32 = 0;
            while j < len && depth > 0 {
                if chars[j] == '[' && !in_cc2 {
                    in_cc2 = true;
                } else if chars[j] == ']' && in_cc2 {
                    in_cc2 = false;
                } else if chars[j] == '\\' && j + 1 < len {
                    j += 1;
                } else if chars[j] == '(' && !in_cc2 {
                    depth += 1;
                    if j + 1 < len && chars[j + 1] == '?' {
                        if j + 2 < len
                            && chars[j + 2] == '<'
                            && j + 3 < len
                            && chars[j + 3] != '='
                            && chars[j + 3] != '!'
                        {
                            num_caps += 1;
                            group_count += 1;
                        }
                    } else {
                        num_caps += 1;
                        group_count += 1;
                    }
                } else if chars[j] == ')' && !in_cc2 {
                    depth -= 1;
                }
                if depth > 0 {
                    j += 1;
                }
            }
            let content: String = chars[content_start..j].iter().collect();
            if num_caps > 0 {
                lb_capture_groups.push((capture_offset, num_caps, content, positive));
            }
            i = j + 1;
            continue;
        }

        if !in_cc && chars[i] == '(' {
            if i + 1 < len && chars[i + 1] == '?' {
                if i + 2 < len
                    && chars[i + 2] == '<'
                    && i + 3 < len
                    && chars[i + 3] != '='
                    && chars[i + 3] != '!'
                {
                    group_count += 1;
                }
            } else {
                group_count += 1;
            }
        }

        i += 1;
    }

    // Check if any external backref references a lookbehind capture group
    let mut verify_infos = Vec::new();
    for (offset, num_caps, content, positive) in &lb_capture_groups {
        let mut referenced_groups = Vec::new();
        for group_num in (*offset + 1)..=(*offset + num_caps) {
            if external_backrefs.contains(&group_num) {
                referenced_groups.push(group_num);
            }
        }
        if !referenced_groups.is_empty() {
            verify_infos.push(super::regexp_lookbehind::LookbehindVerifyInfo {
                positive: *positive,
                content: content.clone(),
                capture_groups: referenced_groups,
                capture_offset: *offset,
                flags: super::regexp_lookbehind::LbFlags {
                    ignore_case: flags.contains('i'),
                    multiline: flags.contains('m'),
                    dot_all: flags.contains('s'),
                },
            });
        }
    }

    if verify_infos.is_empty() {
        None
    } else {
        Some(verify_infos)
    }
}

fn try_build_custom_lookbehind(
    source: &str,
    flags: &str,
    dup_map: DupGroupMap,
    name_order: Vec<String>,
) -> Option<(CompiledRegex, DupGroupMap, Vec<String>)> {
    let (lookbehinds, stripped) = super::regexp_lookbehind::extract_lookbehinds(source);
    if lookbehinds.is_empty() {
        return None;
    }

    // Detect if lookbehind captures are referenced by external backrefs
    let mut external_lb_backrefs: Vec<(u32, u32)> = Vec::new();
    let mut remaining_source: Option<String> = None;

    if let Some(verify_infos) = lookbehind_captures_with_external_backrefs(source, flags) {
        for vi in &verify_infos {
            for &g in &vi.capture_groups {
                external_lb_backrefs.push((g, g));
            }
        }
        // Build remaining pattern: source with lookbehinds removed (no markers)
        let (_, rem) = super::regexp_lookbehind::extract_lookbehinds_remaining(source);
        remaining_source = Some(rem);
    }

    let stripped_tr = translate_js_pattern_ex(&stripped, flags).ok()?;
    let outer = fancy_regex::Regex::new(&stripped_tr.pattern).ok()?;
    let total = count_capture_groups(source);
    Some((
        CompiledRegex::FancyWithCustomLookbehind {
            outer_regex: outer,
            lookbehinds,
            flags: flags.to_string(),
            total_groups: total,
            external_lb_backrefs,
            remaining_source,
        },
        dup_map,
        name_order,
    ))
}

/// Fix patterns that fancy-regex rejects with TargetNotRepeatable.
/// When a non-capturing group `(?:...)` contains only zero-width assertions
/// (lookaheads/lookbehinds) and is followed by a quantifier, fancy-regex
/// considers it non-repeatable. We fix this by inserting an empty-matching
/// element `(?:)` before the closing `)` to make the group "consuming".
fn fix_assertion_only_quantified_groups(pattern: &str) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let len = chars.len();
    if len == 0 {
        return pattern.to_string();
    }

    // Find all non-capturing groups and check if they contain only assertions
    let mut result = String::with_capacity(pattern.len() + 16);

    // First, find all group open positions and their matching close positions
    // Then check which non-capturing groups are assertion-only
    struct GroupInfo {
        open: usize,
        close: usize,
        is_non_capturing: bool,
    }

    let mut groups: Vec<GroupInfo> = Vec::new();
    let mut stack: Vec<(usize, bool)> = Vec::new(); // (open_pos, is_non_capturing)
    let mut j = 0;
    let mut in_cc = false;
    while j < len {
        match chars[j] {
            '\\' if j + 1 < len => {
                j += 2;
                continue;
            }
            '[' if !in_cc => {
                in_cc = true;
            }
            ']' if in_cc => {
                in_cc = false;
            }
            '(' if !in_cc => {
                let is_nc = j + 2 < len && chars[j + 1] == '?' && chars[j + 2] == ':';
                stack.push((j, is_nc));
            }
            ')' if !in_cc => {
                if let Some((open, is_nc)) = stack.pop() {
                    groups.push(GroupInfo {
                        open,
                        close: j,
                        is_non_capturing: is_nc,
                    });
                }
            }
            _ => {}
        }
        j += 1;
    }

    // For each non-capturing group followed by a quantifier, check if it's assertion-only
    let mut insert_positions: HashSet<usize> = HashSet::default();
    for g in &groups {
        if !g.is_non_capturing {
            continue;
        }
        // Check if followed by a quantifier
        let after = g.close + 1;
        if after >= len {
            continue;
        }
        let qc = chars[after];
        if qc != '?' && qc != '*' && qc != '+' && qc != '{' {
            continue;
        }
        // Check if group content is assertion-only (only lookaheads/lookbehinds/anchors)
        if is_assertion_only_content(&chars, g.open + 3, g.close) {
            insert_positions.insert(g.close);
        }
    }

    if insert_positions.is_empty() {
        return pattern.to_string();
    }

    for (idx, &c) in chars.iter().enumerate() {
        if insert_positions.contains(&idx) {
            // Insert zero-width padding that fancy-regex considers repeatable
            result.push_str("a{0}");
        }
        result.push(c);
    }
    result
}

/// Check if the content between `start` and `end` (exclusive) in chars
/// consists only of zero-width assertions (lookaheads, lookbehinds, anchors).
fn is_assertion_only_content(chars: &[char], start: usize, end: usize) -> bool {
    let mut i = start;
    let len = end;
    while i < len {
        match chars[i] {
            // Lookahead/lookbehind groups are zero-width
            '(' if i + 2 < len && chars[i + 1] == '?' => {
                let c2 = chars[i + 2];
                if c2 == '=' || c2 == '!' {
                    // Lookahead (?= or (?! — find matching close
                    if let Some(close) = find_matching_close_paren(&chars[..len], i) {
                        i = close + 1;
                        continue;
                    }
                    return false;
                }
                if c2 == '<' && i + 3 < len && (chars[i + 3] == '=' || chars[i + 3] == '!') {
                    // Lookbehind (?<= or (?<! — find matching close
                    if let Some(close) = find_matching_close_paren(&chars[..len], i) {
                        i = close + 1;
                        continue;
                    }
                    return false;
                }
                // Non-capturing group (?:...) — check recursively
                if c2 == ':'
                    && let Some(close) = find_matching_close_paren(&chars[..len], i)
                {
                    if !is_assertion_only_content(chars, i + 3, close) {
                        return false;
                    }
                    i = close + 1;
                    continue;
                }
                return false;
            }
            // Anchors are zero-width
            '^' | '$' => {
                i += 1;
            }
            '\\' if i + 1 < len => {
                let next = chars[i + 1];
                // \b, \B are zero-width anchors
                if next == 'b' || next == 'B' {
                    i += 2;
                } else {
                    return false; // \d, \w, etc. are consuming
                }
            }
            // Whitespace between assertions is ok
            ' ' | '\t' | '\n' | '\r' => {
                i += 1;
            }
            _ => return false,
        }
    }
    true
}

/// Per spec §22.2.2.6.1 step 2.b: when a nullable group body (one that can
/// match the empty string) is quantified with `*` or `+`, an iteration that
/// matches empty must fail.  fancy-regex stops the loop instead of
/// backtracking, so we convert lazy min-0 quantifiers (`??`, `*?`) inside
/// nullable bodies to greedy, forcing them to consume characters.
fn fix_nullable_quantifiers(source: &str) -> String {
    if !source.contains("??") && !source.contains("*?") {
        return source.to_string();
    }

    let chars: Vec<char> = source.chars().collect();
    let len = chars.len();

    // Build matching-paren map
    let mut close_of = vec![0usize; len];
    let mut stack: Vec<usize> = Vec::new();
    let mut i = 0;
    let mut in_cc = false;
    while i < len {
        match chars[i] {
            '\\' if !in_cc && i + 1 < len => {
                i += 2;
                continue;
            }
            '[' if !in_cc => in_cc = true,
            ']' if in_cc => in_cc = false,
            '(' if !in_cc => stack.push(i),
            ')' if !in_cc => {
                if let Some(open) = stack.pop() {
                    close_of[open] = i;
                }
            }
            _ => {}
        }
        i += 1;
    }

    let mut remove = vec![false; len];

    for open_pos in 0..len {
        if chars[open_pos] != '(' || close_of[open_pos] == 0 {
            continue;
        }
        let close = close_of[open_pos];
        let after = close + 1;
        if after >= len || !matches!(chars[after], '*' | '+') {
            continue;
        }
        // Find body start (skip group type prefix)
        let body_start = nq_body_start(&chars, open_pos);
        if body_start >= close {
            continue;
        }
        if nq_is_nullable(&chars, body_start, close, &close_of) {
            nq_mark_lazy(&chars, body_start, close, &close_of, &mut remove);
        }
    }

    if !remove.iter().any(|&r| r) {
        return source.to_string();
    }
    let mut result = String::with_capacity(len);
    for i in 0..len {
        if !remove[i] {
            result.push(chars[i]);
        }
    }
    result
}

fn nq_body_start(chars: &[char], open: usize) -> usize {
    let len = chars.len();
    if open + 1 < len && chars[open + 1] == '?' {
        if open + 2 < len && chars[open + 2] == ':' {
            return open + 3;
        }
        if open + 2 < len
            && chars[open + 2] == '<'
            && open + 3 < len
            && chars[open + 3] != '='
            && chars[open + 3] != '!'
        {
            for (j, &ch) in chars.iter().enumerate().take(len).skip(open + 3) {
                if ch == '>' {
                    return j + 1;
                }
            }
        }
        // Other group types (assertions, modifiers) — don't rewrite
        return len;
    }
    open + 1
}

/// Check whether the regex segment chars[start..end] can match the empty string.
fn nq_is_nullable(chars: &[char], start: usize, end: usize, close_of: &[usize]) -> bool {
    let mut i = start;
    while i < end {
        match chars[i] {
            '|' => {
                // Left alternative was all nullable (we got here without returning false).
                // If ANY alternative is nullable, the whole alternation is nullable.
                // Skip right side — it's nullable if left side was.
                return true;
            }
            '\\' if i + 1 < end => {
                let atom_end = i + 2;
                if !nq_has_min0(chars, atom_end, end) {
                    return false;
                }
                i = nq_skip_quant(chars, atom_end, end);
            }
            '[' => {
                let mut j = i + 1;
                while j < end && chars[j] != ']' {
                    if chars[j] == '\\' && j + 1 < end {
                        j += 1;
                    }
                    j += 1;
                }
                let atom_end = if j < end { j + 1 } else { j };
                if !nq_has_min0(chars, atom_end, end) {
                    return false;
                }
                i = nq_skip_quant(chars, atom_end, end);
            }
            '(' => {
                let close = close_of[i];
                if close == 0 || close >= end {
                    return false;
                }
                let atom_end = close + 1;
                if !nq_has_min0(chars, atom_end, end) {
                    return false;
                }
                i = nq_skip_quant(chars, atom_end, end);
            }
            '^' | '$' => {
                i += 1;
            }
            _ => {
                let atom_end = i + 1;
                if !nq_has_min0(chars, atom_end, end) {
                    return false;
                }
                i = nq_skip_quant(chars, atom_end, end);
            }
        }
    }
    true
}

fn nq_has_min0(chars: &[char], pos: usize, end: usize) -> bool {
    if pos >= end {
        return false;
    }
    match chars[pos] {
        '?' | '*' => true,
        '{' => {
            // {0,...}
            pos + 1 < end && chars[pos + 1] == '0'
        }
        _ => false,
    }
}

fn nq_skip_quant(chars: &[char], pos: usize, end: usize) -> usize {
    if pos >= end {
        return pos;
    }
    let mut p = pos;
    match chars[p] {
        '?' | '*' | '+' => {
            p += 1;
            if p < end && chars[p] == '?' {
                p += 1;
            }
        }
        '{' => {
            while p < end && chars[p] != '}' {
                p += 1;
            }
            if p < end {
                p += 1;
            }
            if p < end && chars[p] == '?' {
                p += 1;
            }
        }
        _ => {}
    }
    p
}

/// Mark lazy modifiers (`?` after `?` or `*`) for removal in nullable bodies.
fn nq_mark_lazy(chars: &[char], start: usize, end: usize, close_of: &[usize], remove: &mut [bool]) {
    let mut i = start;
    while i < end {
        let atom_end = match chars[i] {
            '\\' if i + 1 < end => i + 2,
            '[' => {
                let mut j = i + 1;
                while j < end && chars[j] != ']' {
                    if chars[j] == '\\' && j + 1 < end {
                        j += 1;
                    }
                    j += 1;
                }
                if j < end { j + 1 } else { j }
            }
            '(' => {
                let close = close_of[i];
                if close > 0 && close < end {
                    close + 1
                } else {
                    i += 1;
                    continue;
                }
            }
            '^' | '$' | '|' => {
                i += 1;
                continue;
            }
            _ => i + 1,
        };
        // Check for lazy min-0 quantifier after this atom
        if atom_end < end && matches!(chars[atom_end], '?' | '*') {
            let lazy_pos = atom_end + 1;
            if lazy_pos < end && chars[lazy_pos] == '?' {
                remove[lazy_pos] = true;
            }
        }
        i = nq_skip_quant(chars, atom_end, end);
    }
}

fn build_regex_ex(
    source: &str,
    flags: &str,
) -> Result<(CompiledRegex, DupGroupMap, Vec<String>), String> {
    let source = fix_nullable_quantifiers(source);
    let tr = translate_js_pattern_ex(&source, flags)?;
    let dup_map = tr.dup_group_map;
    let name_order = tr.group_name_order;

    if tr.needs_bytes_mode {
        return regex::bytes::Regex::new(&tr.pattern)
            .map(|r| (CompiledRegex::Bytes(r), dup_map, name_order))
            .map_err(|e| e.to_string());
    }

    // Check if lookbehind needs custom RTL handling (has backrefs inside lookbehind)
    if lookbehind_needs_custom_rtl(&source)
        && let Some(result) =
            try_build_custom_lookbehind(&source, flags, dup_map.clone(), name_order.clone())
    {
        return Ok(result);
    }

    match fancy_regex::Regex::new(&tr.pattern) {
        Ok(r) => Ok((CompiledRegex::Fancy(r), dup_map, name_order)),
        Err(e) => {
            // If the error is TargetNotRepeatable, try fixing assertion-only groups
            let err_str = e.to_string();
            if err_str.contains("Target of repeat operator") {
                let fixed = fix_assertion_only_quantified_groups(&tr.pattern);
                if fixed != tr.pattern
                    && let Ok(r) = fancy_regex::Regex::new(&fixed)
                {
                    return Ok((CompiledRegex::Fancy(r), dup_map, name_order));
                }
            }

            // If fancy-regex fails and the pattern has lookbehinds, try custom path
            if (source.contains("(?<=") || source.contains("(?<!"))
                && let Some(result) =
                    try_build_custom_lookbehind(&source, flags, dup_map.clone(), name_order.clone())
            {
                return Ok(result);
            }
            regex::Regex::new(&tr.pattern)
                .map(|r| (CompiledRegex::Standard(r), dup_map, name_order))
                .map_err(|e| e.to_string())
        }
    }
}

fn count_capture_groups(source: &str) -> usize {
    let chars: Vec<char> = source.chars().collect();
    let mut count = 0;
    let mut i = 0;
    let mut in_cc = false;
    while i < chars.len() {
        match chars[i] {
            '[' if !in_cc => in_cc = true,
            ']' if in_cc => in_cc = false,
            '\\' if i + 1 < chars.len() => {
                i += 1;
            }
            '(' if !in_cc => {
                if i + 1 < chars.len() && chars[i + 1] == '?' {
                    if i + 2 < chars.len()
                        && chars[i + 2] == '<'
                        && i + 3 < chars.len()
                        && chars[i + 3] != '='
                        && chars[i + 3] != '!'
                    {
                        count += 1; // named group
                    }
                } else {
                    count += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    count
}

fn regex_captures(re: &CompiledRegex, text: &str) -> Option<RegexCaptures> {
    regex_captures_at(re, text, 0)
}

/// Extract named groups from a lookbehind content string.
/// Returns (1-based capture index within the lookbehind, name).
fn extract_named_groups_from_content(content: &str) -> Vec<(usize, String)> {
    let chars: Vec<char> = content.chars().collect();
    let len = chars.len();
    let mut result = Vec::new();
    let mut capture_count = 0usize;
    let mut i = 0;
    let mut in_cc = false;
    while i < len {
        match chars[i] {
            '\\' if i + 1 < len => {
                i += 2;
                continue;
            }
            '[' if !in_cc => in_cc = true,
            ']' if in_cc => in_cc = false,
            '(' if !in_cc => {
                if i + 1 < len && chars[i + 1] == '?' {
                    if i + 2 < len
                        && chars[i + 2] == '<'
                        && i + 3 < len
                        && chars[i + 3] != '='
                        && chars[i + 3] != '!'
                    {
                        capture_count += 1;
                        let name_start = i + 3;
                        let mut name_end = name_start;
                        while name_end < len && chars[name_end] != '>' {
                            name_end += 1;
                        }
                        let name: String = chars[name_start..name_end].iter().collect();
                        result.push((capture_count, name));
                    }
                } else {
                    capture_count += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    result
}

macro_rules! build_captures {
    ($caps:expr, $names_iter:expr) => {{
        let names: Vec<Option<String>> = $names_iter.map(|n| n.map(|s| s.to_string())).collect();
        let mut groups = Vec::new();
        for i in 0..$caps.len() {
            groups.push($caps.get(i).map(|m| RegexMatch {
                start: m.start(),
                end: m.end(),
                text: m.as_str().to_string(),
            }));
        }
        Some(RegexCaptures { groups, names })
    }};
}

fn regex_captures_at(re: &CompiledRegex, text: &str, pos: usize) -> Option<RegexCaptures> {
    match re {
        CompiledRegex::Fancy(r) => {
            let caps = r.captures_from_pos(text, pos).ok()??;
            build_captures!(caps, r.capture_names())
        }
        CompiledRegex::Standard(r) => {
            let caps = if pos == 0 {
                r.captures(text)?
            } else {
                r.captures_at(text, pos)?
            };
            build_captures!(caps, r.capture_names())
        }
        CompiledRegex::FancyWithCustomLookbehind {
            outer_regex,
            lookbehinds,
            flags,
            total_groups,
            external_lb_backrefs,
            remaining_source,
        } => {
            let result = if !external_lb_backrefs.is_empty() {
                if let Some(rem) = remaining_source {
                    super::regexp_lookbehind::match_with_lookbehind_no_backtrack(
                        lookbehinds,
                        rem,
                        flags,
                        text,
                        pos,
                        *total_groups,
                        external_lb_backrefs,
                    )?
                } else {
                    super::regexp_lookbehind::match_with_lookbehind(
                        outer_regex,
                        lookbehinds,
                        text,
                        flags,
                        pos,
                        *total_groups,
                    )?
                }
            } else {
                super::regexp_lookbehind::match_with_lookbehind(
                    outer_regex,
                    lookbehinds,
                    text,
                    flags,
                    pos,
                    *total_groups,
                )?
            };

            let mut names: Vec<Option<String>> = outer_regex
                .capture_names()
                .map(|n| n.map(|s| s.to_string()))
                .collect();

            // Add named groups from lookbehind captures
            for lb in lookbehinds {
                let lb_names = extract_named_groups_from_content(&lb.content);
                for (offset, name) in lb_names {
                    let global_idx = lb.capture_offset as usize + offset;
                    while names.len() <= global_idx {
                        names.push(None);
                    }
                    names[global_idx] = Some(sanitize_group_name(&name));
                }
            }

            let mut groups: Vec<Option<RegexMatch>> = Vec::new();
            for cap in &result {
                groups.push(cap.map(|(start, end)| RegexMatch {
                    start,
                    end,
                    text: text[start..end].to_string(),
                }));
            }

            // Ensure we have at least total_groups + 1 entries
            while groups.len() <= *total_groups {
                groups.push(None);
            }

            while names.len() < groups.len() {
                names.push(None);
            }

            Some(RegexCaptures { groups, names })
        }
        CompiledRegex::Bytes(_) => {
            unreachable!("Bytes regex should use bytes_regex_captures_at")
        }
    }
}

fn bytes_regex_captures_at(
    re: &regex::bytes::Regex,
    input: &[u8],
    pos: usize,
) -> Option<RegexCaptures> {
    let caps = if pos == 0 {
        re.captures(input)?
    } else {
        re.captures_at(input, pos)?
    };
    let names: Vec<Option<String>> = re
        .capture_names()
        .map(|n| n.map(|s| s.to_string()))
        .collect();
    let mut groups = Vec::new();
    for i in 0..caps.len() {
        groups.push(caps.get(i).map(|m| RegexMatch {
            start: m.start(),
            end: m.end(),
            text: wtf8_slice_to_pua_string(&input[m.start()..m.end()]),
        }));
    }
    Some(RegexCaptures { groups, names })
}

fn extract_source_flags(interp: &Interpreter, this_val: &JsValue) -> Option<(String, String, u64)> {
    if let JsValue::Object(o) = this_val
        && let Some(obj) = interp.get_object(o.id)
    {
        let b = obj.borrow();
        let source = if let Some(ref s) = b.regexp_original_source {
            js_string_to_regex_input(&s.code_units)
        } else {
            return None;
        };
        let flags = if let Some(ref s) = b.regexp_original_flags {
            s.to_rust_string()
        } else {
            String::new()
        };
        drop(b);
        Some((source, flags, o.id))
    } else {
        None
    }
}

/// Spec-compliant Set(O, P, V, Throw) — invokes setters and Proxy traps
fn spec_set(
    interp: &mut Interpreter,
    obj_id: u64,
    key: &str,
    value: JsValue,
    throw: bool,
) -> Result<(), JsValue> {
    let obj_val = JsValue::Object(crate::types::JsObject { id: obj_id });
    if let Some(obj) = interp.get_object(obj_id) {
        // Proxy set trap
        if obj.borrow().is_proxy() || obj.borrow().proxy_revoked {
            let receiver = obj_val.clone();
            match interp.proxy_set(obj_id, key, value, &receiver) {
                Ok(success) => {
                    if !success && throw {
                        return Err(
                            interp.create_type_error(&format!("Cannot set property '{key}'"))
                        );
                    }
                    return Ok(());
                }
                Err(e) => return Err(e),
            }
        }
        // Check for setter accessor
        let desc = interp.get_property_descriptor_on_id(obj_id, key);
        if let Some(ref d) = desc
            && let Some(ref setter) = d.set
            && !matches!(setter, JsValue::Undefined)
        {
            let setter = setter.clone();
            return match interp.call_function(&setter, &obj_val, &[value]) {
                Completion::Normal(_) => Ok(()),
                Completion::Throw(e) => Err(e),
                _ => Ok(()),
            };
        }
        if desc
            .as_ref()
            .map(|d| d.is_accessor_descriptor())
            .unwrap_or(false)
        {
            if throw {
                return Err(interp.create_type_error(&format!(
                    "Cannot set property '{key}' which has only a getter"
                )));
            }
            return Ok(());
        }
        let success = obj.borrow_mut().set_property_value(key, value);
        if !success && throw {
            return Err(interp.create_type_error(&format!(
                "Cannot set property '{key}' which is not writable"
            )));
        }
    }
    Ok(())
}

fn set_last_index_strict(interp: &mut Interpreter, obj_id: u64, val: f64) -> Result<(), JsValue> {
    spec_set(interp, obj_id, "lastIndex", JsValue::Number(val), true)
}

fn regexp_exec_abstract(
    interp: &mut Interpreter,
    rx_id: u64,
    s: &str,
    code_units: &[u16],
) -> Completion {
    let rx_val = JsValue::Object(crate::types::JsObject { id: rx_id });
    let exec_val = match interp.get_object_property(rx_id, "exec", &rx_val) {
        Completion::Normal(v) => v,
        other => return other,
    };
    if interp.is_callable(&exec_val) {
        let result = interp.call_function(
            &exec_val,
            &rx_val,
            &[JsValue::String(JsString {
                code_units: code_units.to_vec(),
            })],
        );
        match result {
            Completion::Normal(ref v) => {
                if matches!(v, JsValue::Object(_)) || matches!(v, JsValue::Null) {
                    result
                } else {
                    Completion::Throw(interp.create_type_error(
                        "RegExp exec method returned something other than an Object or null",
                    ))
                }
            }
            other => other,
        }
    } else {
        // If R does not have a [[RegExpMatcher]] internal slot, throw TypeError
        let is_regexp = if let Some(obj) = interp.get_object(rx_id) {
            obj.borrow().class_name == "RegExp"
        } else {
            false
        };
        if !is_regexp {
            return Completion::Throw(interp.create_type_error(
                "RegExp.prototype.exec requires that 'this' be a RegExp object",
            ));
        }
        let (source, flags) = if let Some(obj) = interp.get_object(rx_id) {
            let b = obj.borrow();
            let src = if let Some(ref s) = b.regexp_original_source {
                js_string_to_regex_input(&s.code_units)
            } else {
                String::new()
            };
            let fl = if let Some(ref s) = b.regexp_original_flags {
                s.to_rust_string()
            } else {
                String::new()
            };
            drop(b);
            (src, fl)
        } else {
            return Completion::Normal(JsValue::Null);
        };
        regexp_exec_raw(interp, rx_id, &source, &flags, s, code_units)
    }
}

/// Inner implementation of RegExp @@replace result collection and processing.
/// Extracted so the caller can bracket it with gc_temp_roots save/restore.
/// AdvanceStringIndex per spec. `index` is in UTF-16 code units.
fn advance_string_index(s: &str, index: usize, unicode: bool) -> usize {
    if !unicode {
        return index + 1;
    }
    let utf16_len = pua_aware_utf16_len(s);
    if index + 1 >= utf16_len {
        return index + 1;
    }
    let byte_offset = utf16_to_byte_offset(s, index);
    if byte_offset >= s.len() {
        return index + 1;
    }
    let c = s[byte_offset..].chars().next().unwrap_or('\0');
    if pua_to_surrogate(c).is_some() {
        index + 1
    } else if c.len_utf16() == 2 {
        index + 2
    } else {
        index + 1
    }
}

fn get_substitution(
    interp: &mut Interpreter,
    matched: &str,
    s: &str,
    position: usize,
    tail_pos: usize,
    captures: &[JsValue],
    named_captures: &JsValue,
    replacement: &str,
) -> Result<String, JsValue> {
    let tail_pos = tail_pos.min(s.len());
    let mut result = String::new();
    let rchars: Vec<char> = replacement.chars().collect();
    let len = rchars.len();
    let mut i = 0;
    let m = captures.len();
    while i < len {
        if rchars[i] == '$' && i + 1 < len {
            match rchars[i + 1] {
                '$' => {
                    result.push('$');
                    i += 2;
                }
                '&' => {
                    result.push_str(matched);
                    i += 2;
                }
                '`' => {
                    if let Some(prefix) = s.get(..position) {
                        result.push_str(prefix);
                    }
                    i += 2;
                }
                '\'' => {
                    if let Some(suffix) = s.get(tail_pos..) {
                        result.push_str(suffix);
                    }
                    i += 2;
                }
                c if c.is_ascii_digit() => {
                    let d1 = (c as u32 - '0' as u32) as usize;
                    if i + 2 < len && rchars[i + 2].is_ascii_digit() {
                        let d2 = (rchars[i + 2] as u32 - '0' as u32) as usize;
                        let nn = d1 * 10 + d2;
                        if nn >= 1 && nn <= m {
                            let cap = &captures[nn - 1];
                            if !cap.is_undefined() {
                                let cap_s = interp.to_string_value(cap)?;
                                result.push_str(&cap_s);
                            }
                            i += 3;
                            continue;
                        }
                    }
                    if d1 >= 1 && d1 <= m {
                        let cap = &captures[d1 - 1];
                        if !cap.is_undefined() {
                            let cap_s = interp.to_string_value(cap)?;
                            result.push_str(&cap_s);
                        }
                    } else {
                        result.push('$');
                        result.push(c);
                    }
                    i += 2;
                }
                '<' => {
                    if matches!(named_captures, JsValue::Undefined) {
                        result.push('$');
                        result.push('<');
                        i += 2;
                    } else {
                        let start = i + 2;
                        let rest: String = rchars[start..].iter().collect();
                        if let Some(end_pos) = rest.find('>') {
                            let group_name: String = rchars
                                [start..start + rest[..end_pos].chars().count()]
                                .iter()
                                .collect();
                            let nc_obj = match named_captures {
                                JsValue::Object(o) => o.id,
                                _ => {
                                    result.push('$');
                                    result.push('<');
                                    i += 2;
                                    continue;
                                }
                            };
                            let capture = match interp.get_object_property(
                                nc_obj,
                                &group_name,
                                named_captures,
                            ) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Err(e),
                                _ => JsValue::Undefined,
                            };
                            if !capture.is_undefined() {
                                let cap_str = interp.to_string_value(&capture)?;
                                result.push_str(&cap_str);
                            }
                            i = start + rest[..end_pos].chars().count() + 1;
                        } else {
                            result.push('$');
                            result.push('<');
                            i += 2;
                        }
                    }
                }
                _ => {
                    result.push('$');
                    i += 1;
                }
            }
        } else {
            result.push(rchars[i]);
            i += 1;
        }
    }
    Ok(result)
}

fn split_surrogates_for_non_unicode(input: &str) -> String {
    let mut result = String::new();
    for c in input.chars() {
        if c as u32 >= 0x10000 && pua_to_surrogate(c).is_none() {
            let cp = c as u32;
            let hi = ((cp - 0x10000) >> 10) + 0xD800;
            let lo = ((cp - 0x10000) & 0x3FF) + 0xDC00;
            result.push(surrogate_to_pua(hi));
            result.push(surrogate_to_pua(lo));
        } else {
            result.push(c);
        }
    }
    result
}

fn resolve_named_group_matches<'a>(
    caps: &'a RegexCaptures,
    dup_map: &DupGroupMap,
    name_order: &[String],
) -> Vec<(String, Option<&'a RegexMatch>)> {
    name_order
        .iter()
        .map(|orig_name| {
            let found = if let Some(variants) = dup_map.get(orig_name) {
                variants.iter().find_map(|(internal_name, _)| {
                    let sanitized = sanitize_group_name(internal_name);
                    caps.names.iter().enumerate().find_map(|(i, name_opt)| {
                        if name_opt.as_deref() == Some(&sanitized) {
                            caps.get(i)
                        } else {
                            None
                        }
                    })
                })
            } else {
                let sanitized = sanitize_group_name(orig_name);
                caps.names
                    .iter()
                    .enumerate()
                    .find_map(|(i, name_opt)| {
                        if name_opt.as_deref() == Some(&sanitized) {
                            Some(caps.get(i))
                        } else {
                            None
                        }
                    })
                    .flatten()
            };
            (orig_name.clone(), found)
        })
        .collect()
}

fn regexp_exec_raw(
    interp: &mut Interpreter,
    this_id: u64,
    source: &str,
    flags: &str,
    input: &str,
    input_code_units: &[u16],
) -> Completion {
    // Spec: Let lastIndex be ? ToLength(? Get(R, "lastIndex")).
    // ToLength may trigger valueOf side effects that recompile the regexp,
    // so we re-read source/flags from internal slots afterward.
    let this_val = JsValue::Object(crate::types::JsObject { id: this_id });
    let li_val = match interp.get_object_property(this_id, "lastIndex", &this_val) {
        Completion::Normal(v) => v,
        other => return other,
    };
    let li_num = match interp.to_number_value(&li_val) {
        Ok(n) => n,
        Err(e) => return Completion::Throw(e),
    };
    let li_length = if li_num.is_nan() || li_num <= 0.0 {
        0.0
    } else {
        li_num.min(9007199254740991.0).floor()
    };

    // Re-read source/flags from internal slots (may have changed via compile() side effect)
    let (source_owned, flags_owned) = {
        if let Some(obj) = interp.get_object(this_id) {
            let b = obj.borrow();
            let src = b
                .regexp_original_source
                .as_ref()
                .map(|s| js_string_to_regex_input(&s.code_units))
                .unwrap_or_else(|| source.to_string());
            let fl = b
                .regexp_original_flags
                .as_ref()
                .map(|s| s.to_rust_string())
                .unwrap_or_else(|| flags.to_string());
            (src, fl)
        } else {
            (source.to_string(), flags.to_string())
        }
    };
    let source = &source_owned;
    let flags = &flags_owned;

    let global = flags.contains('g');
    let sticky = flags.contains('y');
    let has_indices = flags.contains('d');
    let unicode = flags.contains('u') || flags.contains('v');

    let non_unicode_input;
    let input = if !unicode {
        non_unicode_input = split_surrogates_for_non_unicode(input);
        &non_unicode_input
    } else {
        input
    };

    // lastIndex is in UTF-16 code units; convert to byte offset for string slicing
    let input_utf16_len = pua_aware_utf16_len(input);
    let last_index_utf16 = if global || sticky {
        let li_int = li_length as i64;
        if li_int < 0 || li_int as usize > input_utf16_len {
            if let Err(e) = set_last_index_strict(interp, this_id, 0.0) {
                return Completion::Throw(e);
            }
            return Completion::Normal(JsValue::Null);
        }
        li_int as usize
    } else {
        0
    };
    let (re, dup_map, name_order) = match build_regex_ex(source, flags) {
        Ok(r) => r,
        Err(_) => return Completion::Normal(JsValue::Null),
    };

    let is_bytes_mode = matches!(re, CompiledRegex::Bytes(_));
    let wtf8_bytes = if is_bytes_mode {
        js_code_units_to_wtf8(input_code_units)
    } else {
        Vec::new()
    };
    let last_index_byte = if is_bytes_mode {
        utf16_to_wtf8_byte_offset(&wtf8_bytes, last_index_utf16)
    } else {
        utf16_to_byte_offset(input, last_index_utf16)
    };

    let mut caps = if is_bytes_mode {
        if let CompiledRegex::Bytes(ref bytes_re) = re {
            match bytes_regex_captures_at(bytes_re, &wtf8_bytes, last_index_byte) {
                Some(c) => c,
                None => {
                    if (global || sticky)
                        && let Err(e) = set_last_index_strict(interp, this_id, 0.0)
                    {
                        return Completion::Throw(e);
                    }
                    return Completion::Normal(JsValue::Null);
                }
            }
        } else {
            unreachable!()
        }
    } else {
        match regex_captures_at(&re, input, last_index_byte) {
            Some(c) => c,
            None => {
                if (global || sticky)
                    && let Err(e) = set_last_index_strict(interp, this_id, 0.0)
                {
                    return Completion::Throw(e);
                }
                return Completion::Normal(JsValue::Null);
            }
        }
    };

    clear_stale_dup_captures(&mut caps, &dup_map);
    strip_renamed_qi_captures(&mut caps);
    reset_quantifier_inner_captures(&mut caps, source);

    let full_match = caps.get(0).unwrap();
    // Convert absolute byte offsets to UTF-16 code unit offsets
    let match_start_utf16 = if is_bytes_mode {
        wtf8_byte_offset_to_utf16(&wtf8_bytes, full_match.start)
    } else {
        byte_offset_to_utf16(input, full_match.start)
    };
    let match_end_utf16 = if is_bytes_mode {
        wtf8_byte_offset_to_utf16(&wtf8_bytes, full_match.end)
    } else {
        byte_offset_to_utf16(input, full_match.end)
    };

    if sticky && full_match.start != last_index_byte {
        if let Err(e) = set_last_index_strict(interp, this_id, 0.0) {
            return Completion::Throw(e);
        }
        return Completion::Normal(JsValue::Null);
    }

    if (global || sticky)
        && let Err(e) = set_last_index_strict(interp, this_id, match_end_utf16 as f64)
    {
        return Completion::Throw(e);
    }

    // Update Annex B legacy static properties (B.2.4)
    {
        interp.regexp_legacy_input = input.to_string();
        interp.regexp_legacy_last_match = full_match.text.clone();
        interp.regexp_legacy_left_context = if is_bytes_mode {
            wtf8_slice_to_pua_string(&wtf8_bytes[..full_match.start])
        } else {
            input[..full_match.start].to_string()
        };
        interp.regexp_legacy_right_context = if is_bytes_mode {
            wtf8_slice_to_pua_string(&wtf8_bytes[full_match.end..])
        } else {
            input[full_match.end..].to_string()
        };
        let mut last_paren = String::new();
        for idx in (1..caps.len()).rev() {
            if let Some(m) = caps.get(idx) {
                last_paren = m.text.clone();
                break;
            }
        }
        interp.regexp_legacy_last_paren = last_paren;
        for p in 0..9 {
            interp.regexp_legacy_parens[p] =
                caps.get(p + 1).map(|m| m.text.clone()).unwrap_or_default();
        }
    }

    let mut elements: Vec<JsValue> = Vec::new();
    elements.push(JsValue::String(regex_output_to_js_string(&full_match.text)));
    for i in 1..caps.len() {
        match caps.get(i) {
            Some(m) => elements.push(JsValue::String(regex_output_to_js_string(&m.text))),
            None => elements.push(JsValue::Undefined),
        }
    }

    let has_named = caps.names.iter().any(|n| n.is_some());
    let resolved_named = if has_named {
        Some(resolve_named_group_matches(&caps, &dup_map, &name_order))
    } else {
        None
    };
    let groups_val = if let Some(ref resolved) = resolved_named {
        let groups_obj = interp.create_object();
        groups_obj.borrow_mut().prototype_id = None;
        for (name, m) in resolved {
            let val = match m {
                Some(m) => JsValue::String(regex_output_to_js_string(&m.text)),
                None => JsValue::Undefined,
            };
            groups_obj.borrow_mut().insert_value(name.clone(), val);
        }
        let id = groups_obj.borrow().id.unwrap();
        JsValue::Object(crate::types::JsObject { id })
    } else {
        JsValue::Undefined
    };

    let result = interp.create_array(elements);
    if let JsValue::Object(ref ro) = result
        && let Some(robj) = interp.get_object(ro.id)
    {
        robj.borrow_mut().insert_value(
            "index".to_string(),
            JsValue::Number(match_start_utf16 as f64),
        );
        robj.borrow_mut().insert_value(
            "input".to_string(),
            JsValue::String(JsString::from_str(input)),
        );
        robj.borrow_mut()
            .insert_value("groups".to_string(), groups_val.clone());

        if has_indices {
            let cap_byte_to_utf16 = |offset: usize| -> usize {
                if is_bytes_mode {
                    wtf8_byte_offset_to_utf16(&wtf8_bytes, offset)
                } else {
                    byte_offset_to_utf16(input, offset)
                }
            };
            let mut index_pairs: Vec<JsValue> = Vec::new();
            for i in 0..caps.len() {
                match caps.get(i) {
                    Some(m) => {
                        let cap_start = cap_byte_to_utf16(m.start);
                        let cap_end = cap_byte_to_utf16(m.end);
                        let pair = interp.create_array(vec![
                            JsValue::Number(cap_start as f64),
                            JsValue::Number(cap_end as f64),
                        ]);
                        index_pairs.push(pair);
                    }
                    None => index_pairs.push(JsValue::Undefined),
                }
            }
            let indices_arr = interp.create_array(index_pairs);
            if let Some(ref resolved) = resolved_named {
                let idx_groups = interp.create_object();
                idx_groups.borrow_mut().prototype_id = None;
                for (name, m) in resolved {
                    let val = match m {
                        Some(m) => {
                            let cap_start = cap_byte_to_utf16(m.start);
                            let cap_end = cap_byte_to_utf16(m.end);
                            interp.create_array(vec![
                                JsValue::Number(cap_start as f64),
                                JsValue::Number(cap_end as f64),
                            ])
                        }
                        None => JsValue::Undefined,
                    };
                    idx_groups.borrow_mut().insert_value(name.clone(), val);
                }
                let idx_groups_id = idx_groups.borrow().id.unwrap();
                if let JsValue::Object(ref io) = indices_arr
                    && let Some(iobj) = interp.get_object(io.id)
                {
                    iobj.borrow_mut().insert_value(
                        "groups".to_string(),
                        JsValue::Object(crate::types::JsObject { id: idx_groups_id }),
                    );
                }
            } else if let JsValue::Object(ref io) = indices_arr
                && let Some(iobj) = interp.get_object(io.id)
            {
                iobj.borrow_mut()
                    .insert_value("groups".to_string(), JsValue::Undefined);
            }
            robj.borrow_mut()
                .insert_value("indices".to_string(), indices_arr);
        }
    }
    Completion::Normal(result)
}

fn get_symbol_key(interp: &Interpreter, name: &str) -> Option<String> {
    interp
        .realm()
        .global_env
        .borrow()
        .get("Symbol")
        .and_then(|sv| {
            if let JsValue::Object(so) = sv {
                Some(to_js_string(&interp.get_property_on_id(so.id, name)))
            } else {
                None
            }
        })
}

impl Interpreter {
    pub(crate) fn setup_regexp(&mut self) {
        let regexp_proto = self.create_object();

        // RegExp.prototype.exec
        let exec_fn = self.create_function(JsFunction::native(
            "exec".to_string(),
            1,
            |interp, this_val, args| {
                let obj_id = match this_val {
                    JsValue::Object(o) => o.id,
                    _ => {
                        return Completion::Throw(interp.create_type_error(
                            "RegExp.prototype.exec requires that 'this' be an Object",
                        ));
                    }
                };
                if let Some(obj) = interp.get_object(obj_id)
                    && obj.borrow().class_name != "RegExp"
                {
                    return Completion::Throw(interp.create_type_error(
                        "RegExp.prototype.exec requires that 'this' be a RegExp object",
                    ));
                }
                let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let (input, code_units) = match to_regex_input_with_units(interp, &arg) {
                    Ok(r) => r,
                    Err(e) => return Completion::Throw(e),
                };
                if let Some((source, flags, obj_id)) = extract_source_flags(interp, this_val) {
                    return regexp_exec_raw(interp, obj_id, &source, &flags, &input, &code_units);
                }
                Completion::Normal(JsValue::Null)
            },
        ));
        regexp_proto
            .borrow_mut()
            .insert_builtin("exec".to_string(), exec_fn);

        // RegExp.prototype.test
        let test_fn = self.create_function(JsFunction::native(
            "test".to_string(),
            1,
            |interp, this_val, args| {
                let obj_id = match this_val {
                    JsValue::Object(o) => o.id,
                    _ => {
                        return Completion::Throw(interp.create_type_error(
                            "RegExp.prototype.test requires that 'this' be an Object",
                        ));
                    }
                };
                let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let (input, input_cu) = match to_regex_input_with_units(interp, &arg) {
                    Ok(r) => r,
                    Err(e) => return Completion::Throw(e),
                };
                let result = regexp_exec_abstract(interp, obj_id, &input, &input_cu);
                match result {
                    Completion::Normal(v) => {
                        Completion::Normal(JsValue::Boolean(!matches!(v, JsValue::Null)))
                    }
                    other => other,
                }
            },
        ));
        regexp_proto
            .borrow_mut()
            .insert_builtin("test".to_string(), test_fn);

        // RegExp.prototype.toString (§22.2.5.14)
        let tostring_fn = self.create_function(JsFunction::native(
            "toString".to_string(),
            0,
            |interp, this_val, _args| {
                let obj_id = match this_val {
                    JsValue::Object(o) => o.id,
                    _ => {
                        return Completion::Throw(interp.create_type_error(
                            "RegExp.prototype.toString requires that 'this' be an Object",
                        ));
                    }
                };
                // 3. Let pattern be ? ToString(? Get(R, "source")).
                let source_val = match interp.get_object_property(obj_id, "source", this_val) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let source = match interp.to_string_value(&source_val) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                // 4. Let flags be ? ToString(? Get(R, "flags")).
                let flags_val = match interp.get_object_property(obj_id, "flags", this_val) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let flags = match interp.to_string_value(&flags_val) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                Completion::Normal(JsValue::String(JsString::from_str(&format!(
                    "/{}/{}",
                    source, flags
                ))))
            },
        ));
        regexp_proto
            .borrow_mut()
            .insert_builtin("toString".to_string(), tostring_fn);

        // RegExp.prototype.compile (Annex B §B.2.5.1)
        let compile_proto_id = regexp_proto.borrow().id.unwrap();
        let compile_fn = self.create_function(JsFunction::native(
            "compile".to_string(),
            2,
            move |interp, this_val, args| {
                // 1. Let O be the this value.
                let obj_id = match this_val {
                    JsValue::Object(o) => o.id,
                    _ => {
                        return Completion::Throw(interp.create_type_error(
                            "RegExp.prototype.compile requires that 'this' be an Object",
                        ));
                    }
                };
                // 2. Perform ? RequireInternalSlot(O, [[RegExpMatcher]]).
                if let Some(obj) = interp.get_object(obj_id) {
                    if obj.borrow().class_name != "RegExp" {
                        return Completion::Throw(interp.create_type_error(
                            "RegExp.prototype.compile requires that 'this' be a RegExp object",
                        ));
                    }
                    // B.2.5.1 step 3: throw TypeError for subclass instances
                    let proto_matches = obj.borrow().prototype_id == Some(compile_proto_id);
                    if !proto_matches {
                        return Completion::Throw(interp.create_type_error(
                            "RegExp.prototype.compile cannot be used on RegExp subclass instances",
                        ));
                    }
                } else {
                    return Completion::Throw(interp.create_type_error(
                        "RegExp.prototype.compile requires that 'this' be a RegExp object",
                    ));
                }

                let pattern_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let flags_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);

                let (pattern_str, flags_str);
                let pattern_js: Option<JsString>;

                // 3. If pattern is a RegExp object
                let pattern_is_regexp = if let JsValue::Object(po) = &pattern_arg {
                    interp
                        .get_object(po.id)
                        .map(|o| o.borrow().class_name == "RegExp")
                        .unwrap_or(false)
                } else {
                    false
                };

                if pattern_is_regexp {
                    // a. If flags is not undefined, throw a TypeError exception.
                    if !matches!(flags_arg, JsValue::Undefined) {
                        return Completion::Throw(interp.create_type_error(
                            "Cannot supply flags when constructing one RegExp from another",
                        ));
                    }
                    // b. Let P be pattern.[[OriginalSource]].
                    // c. Let F be pattern.[[OriginalFlags]].
                    let po_id = if let JsValue::Object(po) = &pattern_arg {
                        po.id
                    } else {
                        unreachable!()
                    };
                    if let Some(pobj) = interp.get_object(po_id) {
                        let b = pobj.borrow();
                        pattern_str = if let Some(ref s) = b.regexp_original_source {
                            js_string_to_regex_input(&s.code_units)
                        } else {
                            "(?:)".to_string()
                        };
                        pattern_js = b.regexp_original_source.clone();
                        flags_str = if let Some(ref s) = b.regexp_original_flags {
                            s.to_rust_string()
                        } else {
                            String::new()
                        };
                    } else {
                        pattern_str = "(?:)".to_string();
                        pattern_js = None;
                        flags_str = String::new();
                    }
                } else {
                    // 4. Let P be pattern, let F be flags.
                    let p_js = if matches!(pattern_arg, JsValue::Undefined) {
                        JsString::from_str("")
                    } else {
                        match interp.to_js_string(&pattern_arg) {
                            Ok(s) => s,
                            Err(e) => return Completion::Throw(e),
                        }
                    };
                    pattern_str = js_string_to_regex_input(&p_js.code_units);
                    pattern_js = Some(p_js);
                    flags_str = if matches!(flags_arg, JsValue::Undefined) {
                        String::new()
                    } else {
                        match interp.to_string_value(&flags_arg) {
                            Ok(s) => s,
                            Err(e) => return Completion::Throw(e),
                        }
                    };
                }

                // Validate flags: no invalid chars and no duplicates
                {
                    let mut seen = HashSet::default();
                    for c in flags_str.chars() {
                        if !matches!(c, 'g' | 'i' | 'm' | 's' | 'u' | 'v' | 'y' | 'd') {
                            return Completion::Throw(interp.create_error(
                                "SyntaxError",
                                &format!("Invalid regular expression flags: {}", flags_str),
                            ));
                        }
                        if !seen.insert(c) {
                            return Completion::Throw(interp.create_error(
                                "SyntaxError",
                                &format!("Invalid regular expression flags: {}", flags_str),
                            ));
                        }
                    }
                }

                // Validate pattern
                if let Err(msg) = validate_js_pattern(&pattern_str, &flags_str) {
                    return Completion::Throw(interp.create_error("SyntaxError", &msg));
                }

                // Build the regex to validate it compiles
                if let Err(msg) = build_regex_ex(&pattern_str, &flags_str) {
                    return Completion::Throw(interp.create_error("SyntaxError", &msg));
                }

                let source_js = match pattern_js {
                    Some(js) if !js.code_units.is_empty() => js,
                    _ => JsString::from_str("(?:)"),
                };

                // 5. Return ? RegExpInitialize(O, P, F).
                if let Some(obj) = interp.get_object(obj_id) {
                    let mut b = obj.borrow_mut();
                    b.regexp_original_source = Some(source_js);
                    b.regexp_original_flags = Some(JsString::from_str(&flags_str));
                }
                // Set lastIndex to 0 with strict mode (throws TypeError if non-writable)
                if let Err(e) = set_last_index_strict(interp, obj_id, 0.0) {
                    return Completion::Throw(e);
                }

                Completion::Normal(this_val.clone())
            },
        ));
        regexp_proto
            .borrow_mut()
            .insert_builtin("compile".to_string(), compile_fn);

        // [@@match] (§22.2.5.6)
        let match_fn = self.create_function(JsFunction::native(
            "[Symbol.match]".to_string(),
            1,
            |interp, this_val, args| {
                let rx_id = match this_val {
                    JsValue::Object(o) => o.id,
                    _ => {
                        return Completion::Throw(interp.create_type_error(
                            "RegExp.prototype[@@match] requires that 'this' be an Object",
                        ));
                    }
                };
                let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let (s, s_cu) = match to_regex_input_with_units(interp, &arg) {
                    Ok(r) => r,
                    Err(e) => return Completion::Throw(e),
                };

                // 4. Let flags be ? ToString(? Get(rx, "flags")).
                let rx_val = JsValue::Object(crate::types::JsObject { id: rx_id });
                let flags_val = match interp.get_object_property(rx_id, "flags", &rx_val) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let flags_str = match interp.to_string_value(&flags_val) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };

                // 5. If flags does not contain "g", then
                if !flags_str.contains('g') {
                    // a. Return ? RegExpExec(rx, S).
                    return regexp_exec_abstract(interp, rx_id, &s, &s_cu);
                }

                // 6. Let fullUnicode be flags contains "u" or "v".
                let full_unicode = flags_str.contains('u') || flags_str.contains('v');

                // 6b. Perform ? Set(rx, "lastIndex", +0𝔽, true).
                if let Err(e) = set_last_index_strict(interp, rx_id, 0.0) {
                    return Completion::Throw(e);
                }

                let mut results: Vec<JsValue> = Vec::new();
                loop {
                    let result = regexp_exec_abstract(interp, rx_id, &s, &s_cu);
                    match result {
                        Completion::Normal(JsValue::Null) => break,
                        Completion::Normal(ref result_val)
                            if matches!(result_val, JsValue::Object(_)) =>
                        {
                            let result_id = if let JsValue::Object(o) = result_val {
                                o.id
                            } else {
                                unreachable!()
                            };
                            let matched_val =
                                match interp.get_object_property(result_id, "0", result_val) {
                                    Completion::Normal(v) => v,
                                    other => return other,
                                };
                            // Use to_js_string to preserve lone surrogates (not lossy)
                            let match_js_str = match interp.to_js_string(&matched_val) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            };
                            let is_empty = match_js_str.code_units.is_empty();
                            results.push(JsValue::String(match_js_str));
                            if is_empty {
                                let rx_val2 = JsValue::Object(crate::types::JsObject { id: rx_id });
                                let li_val = match interp.get_object_property(
                                    rx_id,
                                    "lastIndex",
                                    &rx_val2,
                                ) {
                                    Completion::Normal(v) => v,
                                    other => return other,
                                };
                                let li_num = match interp.to_number_value(&li_val) {
                                    Ok(n) => n,
                                    Err(e) => return Completion::Throw(e),
                                };
                                let this_index = if li_num.is_nan() || li_num <= 0.0 {
                                    0
                                } else {
                                    li_num.min(9007199254740991.0).floor() as usize
                                };
                                let next_index = advance_string_index(&s, this_index, full_unicode);
                                if let Err(e) =
                                    set_last_index_strict(interp, rx_id, next_index as f64)
                                {
                                    return Completion::Throw(e);
                                }
                            }
                        }
                        Completion::Normal(_) => break,
                        other => return other,
                    }
                }
                if results.is_empty() {
                    Completion::Normal(JsValue::Null)
                } else {
                    Completion::Normal(interp.create_array(results))
                }
            },
        ));
        if let Some(key) = get_symbol_key(self, "match") {
            regexp_proto
                .borrow_mut()
                .insert_property(key, PropertyDescriptor::data(match_fn, true, false, true));
        }

        // [@@search] (§22.2.5.9)
        let search_fn = self.create_function(JsFunction::native(
            "[Symbol.search]".to_string(),
            1,
            |interp, this_val, args| {
                let rx_id = match this_val {
                    JsValue::Object(o) => o.id,
                    _ => {
                        return Completion::Throw(interp.create_type_error(
                            "RegExp.prototype[@@search] requires that 'this' be an Object",
                        ));
                    }
                };
                let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let (s, s_cu) = match to_regex_input_with_units(interp, &arg) {
                    Ok(r) => r,
                    Err(e) => return Completion::Throw(e),
                };
                let rx_val = JsValue::Object(crate::types::JsObject { id: rx_id });

                // 4. Let previousLastIndex be ? Get(rx, "lastIndex").
                let previous_last_index =
                    match interp.get_object_property(rx_id, "lastIndex", &rx_val) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };

                // 5. If SameValue(previousLastIndex, +0𝔽) is false, then
                //    a. Perform ? Set(rx, "lastIndex", +0𝔽, true).
                if !same_value(&previous_last_index, &JsValue::Number(0.0))
                    && let Err(e) = spec_set(interp, rx_id, "lastIndex", JsValue::Number(0.0), true)
                {
                    return Completion::Throw(e);
                }

                // 6. Let result be ? RegExpExec(rx, S).
                let result = regexp_exec_abstract(interp, rx_id, &s, &s_cu);
                let result_val = match result {
                    Completion::Normal(v) => v,
                    other => return other,
                };

                // 7. Let currentLastIndex be ? Get(rx, "lastIndex").
                let current_last_index =
                    match interp.get_object_property(rx_id, "lastIndex", &rx_val) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };

                // 8. If SameValue(currentLastIndex, previousLastIndex) is false, then
                //    a. Perform ? Set(rx, "lastIndex", previousLastIndex, true).
                if !same_value(&current_last_index, &previous_last_index)
                    && let Err(e) = spec_set(interp, rx_id, "lastIndex", previous_last_index, true)
                {
                    return Completion::Throw(e);
                }

                // 9. If result is null, return -1𝔽.
                if matches!(result_val, JsValue::Null) {
                    return Completion::Normal(JsValue::Number(-1.0));
                }

                // 10. Return ? Get(result, "index").
                if let JsValue::Object(ref o) = result_val {
                    match interp.get_object_property(o.id, "index", &result_val) {
                        Completion::Normal(v) => Completion::Normal(v),
                        other => other,
                    }
                } else {
                    Completion::Normal(JsValue::Number(-1.0))
                }
            },
        ));
        if let Some(key) = get_symbol_key(self, "search") {
            regexp_proto
                .borrow_mut()
                .insert_property(key, PropertyDescriptor::data(search_fn, true, false, true));
        }

        // [@@replace] (§22.2.5.8)
        let replace_fn = self.create_function(JsFunction::native(
            "[Symbol.replace]".to_string(),
            2,
            |interp, this_val, args| {
                // 1. Let rx be the this value.
                // 2. If Type(rx) is not Object, throw a TypeError.
                let rx_id = match this_val {
                    JsValue::Object(o) => o.id,
                    _ => {
                        let err = interp.create_type_error(
                            "RegExp.prototype[@@replace] requires that 'this' be an Object",
                        );
                        return Completion::Throw(err);
                    }
                };

                // 3. Let S be ? ToString(string).
                let string_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let (s, s_cu) = match to_regex_input_with_units(interp, &string_arg) {
                    Ok(r) => r,
                    Err(e) => return Completion::Throw(e),
                };
                // Non-unicode version for string slicing (surrogates kept as
                // individual PUA chars so UTF-16 indexing works correctly)
                let s_slice = match &string_arg {
                    JsValue::String(js) => js_string_to_regex_input_non_unicode(&js.code_units),
                    _ => s.clone(),
                };
                let length_s = s_slice.len();
                let s_utf16_len = pua_aware_utf16_len(&s_slice);

                // 5. Let functionalReplace be IsCallable(replaceValue).
                let replace_value = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let functional_replace = interp.is_callable(&replace_value);

                // 6. If functionalReplace is false, set replaceValue to ? ToString(replaceValue).
                let replace_str = if !functional_replace {
                    match interp.to_string_value(&replace_value) {
                        Ok(s) => Some(s),
                        Err(e) => return Completion::Throw(e),
                    }
                } else {
                    None
                };

                // 7. Let flags be ? ToString(? Get(rx, "flags")).
                let rx_val = JsValue::Object(crate::types::JsObject { id: rx_id });
                let flags_val = match interp.get_object_property(rx_id, "flags", &rx_val) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let flags = match interp.to_string_value(&flags_val) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };

                // 8. Let global be (flags contains "g").
                let global = flags.contains('g');
                let mut full_unicode = false;

                // 9. If global is true, then
                if global {
                    // a. Let fullUnicode be (flags contains "u" or "v").
                    full_unicode = flags.contains('u') || flags.contains('v');
                    // b. Perform ? Set(rx, "lastIndex", +0𝔽, true).
                    match set_last_index_strict(interp, rx_id, 0.0) {
                        Ok(()) => {}
                        Err(e) => return Completion::Throw(e),
                    }
                }

                // 10-11. Collect results
                let mut results: Vec<JsValue> = Vec::new();
                let gc_root_start = interp.gc_temp_roots.len();
                loop {
                    // 11a. Let result be ? RegExpExec(rx, S).
                    let result = regexp_exec_abstract(interp, rx_id, &s, &s_cu);
                    match result {
                        Completion::Normal(JsValue::Null) => break,
                        Completion::Normal(ref result_val)
                            if matches!(result_val, JsValue::Object(_)) =>
                        {
                            let result_obj = result_val.clone();
                            if let JsValue::Object(ref o) = result_obj {
                                interp.gc_temp_roots.push(o.id);
                            }
                            results.push(result_obj.clone());

                            if !global {
                                break;
                            }

                            // For global: check if match is empty and advance
                            let result_id = if let JsValue::Object(ref o) = result_obj {
                                o.id
                            } else {
                                unreachable!()
                            };
                            let matched_val =
                                match interp.get_object_property(result_id, "0", &result_obj) {
                                    Completion::Normal(v) => v,
                                    other => {
                                        interp.gc_temp_roots.truncate(gc_root_start);
                                        return other;
                                    }
                                };
                            let match_str = match interp.to_string_value(&matched_val) {
                                Ok(s) => s,
                                Err(e) => {
                                    interp.gc_temp_roots.truncate(gc_root_start);
                                    return Completion::Throw(e);
                                }
                            };
                            if match_str.is_empty() {
                                // a. Let thisIndex be ? ToLength(? Get(rx, "lastIndex")).
                                let rx_val = JsValue::Object(crate::types::JsObject { id: rx_id });
                                let li_val =
                                    match interp.get_object_property(rx_id, "lastIndex", &rx_val) {
                                        Completion::Normal(v) => v,
                                        other => {
                                            interp.gc_temp_roots.truncate(gc_root_start);
                                            return other;
                                        }
                                    };
                                let li_num = match interp.to_number_value(&li_val) {
                                    Ok(n) => n,
                                    Err(e) => {
                                        interp.gc_temp_roots.truncate(gc_root_start);
                                        return Completion::Throw(e);
                                    }
                                };
                                let this_index = {
                                    let n = if li_num.is_nan() || li_num <= 0.0 {
                                        0.0
                                    } else {
                                        li_num.min(9007199254740991.0).floor()
                                    };
                                    n as usize
                                };
                                let next_index = advance_string_index(&s, this_index, full_unicode);
                                match set_last_index_strict(interp, rx_id, next_index as f64) {
                                    Ok(()) => {}
                                    Err(e) => {
                                        interp.gc_temp_roots.truncate(gc_root_start);
                                        return Completion::Throw(e);
                                    }
                                }
                            }
                        }
                        Completion::Normal(_) => break,
                        other => {
                            interp.gc_temp_roots.truncate(gc_root_start);
                            return other;
                        }
                    }
                }

                // 14. For each element result of results, do
                let mut accumulated_result = String::new();
                let mut next_source_position: usize = 0;

                for result_val in &results {
                    let result_id = if let JsValue::Object(o) = result_val {
                        o.id
                    } else {
                        continue;
                    };

                    // a. Let nCaptures be ? ToLength(? Get(result, "length")).
                    let len_val = match interp.get_object_property(result_id, "length", result_val)
                    {
                        Completion::Normal(v) => v,
                        other => {
                            interp.gc_temp_roots.truncate(gc_root_start);
                            return other;
                        }
                    };
                    let n_captures = {
                        let n = match interp.to_number_value(&len_val) {
                            Ok(n) => n,
                            Err(e) => {
                                interp.gc_temp_roots.truncate(gc_root_start);
                                return Completion::Throw(e);
                            }
                        };
                        let len = if n.is_nan() || n <= 0.0 {
                            0.0
                        } else {
                            n.min(9007199254740991.0).floor()
                        };
                        (len as usize).max(1) // at least 1
                    };
                    // nCaptures = max(nCaptures - 1, 0) -- number of capture groups
                    let n_cap = if n_captures > 0 { n_captures - 1 } else { 0 };

                    // d. Let matched be ? ToString(? Get(result, "0")).
                    let matched_val = match interp.get_object_property(result_id, "0", result_val) {
                        Completion::Normal(v) => v,
                        other => {
                            interp.gc_temp_roots.truncate(gc_root_start);
                            return other;
                        }
                    };
                    let matched = match interp.to_string_value(&matched_val) {
                        Ok(s) => s,
                        Err(e) => {
                            interp.gc_temp_roots.truncate(gc_root_start);
                            return Completion::Throw(e);
                        }
                    };
                    // Compute matchLength in UTF-16 code units for tail_pos calculation
                    let match_length_utf16 = match &matched_val {
                        JsValue::String(js) => js.code_units.len(),
                        _ => matched.encode_utf16().count(),
                    };

                    // e. Let position be ? ToIntegerOrInfinity(? Get(result, "index")).
                    let index_val = match interp.get_object_property(result_id, "index", result_val)
                    {
                        Completion::Normal(v) => v,
                        other => {
                            interp.gc_temp_roots.truncate(gc_root_start);
                            return other;
                        }
                    };
                    // Keep UTF-16 position for passing to replacement function;
                    // convert to byte offset for string slicing in PUA-mapped string.
                    let (position_utf16, position) = {
                        let n = match interp.to_number_value(&index_val) {
                            Ok(n) => n,
                            Err(e) => {
                                interp.gc_temp_roots.truncate(gc_root_start);
                                return Completion::Throw(e);
                            }
                        };
                        let int = to_integer_or_infinity(n);
                        let utf16_pos = int.max(0.0) as usize;
                        (
                            utf16_pos,
                            utf16_to_byte_offset(&s_slice, utf16_pos).min(length_s),
                        )
                    };

                    // g-i. Get captures
                    let mut captures: Vec<JsValue> = Vec::new();
                    for n in 1..=n_cap {
                        let cap_n =
                            match interp.get_object_property(result_id, &n.to_string(), result_val)
                            {
                                Completion::Normal(v) => v,
                                other => {
                                    interp.gc_temp_roots.truncate(gc_root_start);
                                    return other;
                                }
                            };
                        if !cap_n.is_undefined() {
                            let cap_str = match interp.to_string_value(&cap_n) {
                                Ok(s) => s,
                                Err(e) => {
                                    interp.gc_temp_roots.truncate(gc_root_start);
                                    return Completion::Throw(e);
                                }
                            };
                            captures.push(JsValue::String(JsString::from_str(&cap_str)));
                        } else {
                            captures.push(JsValue::Undefined);
                        }
                    }

                    // j. Let namedCaptures be ? Get(result, "groups").
                    let named_captures =
                        match interp.get_object_property(result_id, "groups", result_val) {
                            Completion::Normal(v) => v,
                            other => {
                                interp.gc_temp_roots.truncate(gc_root_start);
                                return other;
                            }
                        };

                    let replacement = if functional_replace {
                        // k. If functionalReplace is true, then
                        let mut replacer_args: Vec<JsValue> = Vec::new();
                        replacer_args.push(JsValue::String(JsString::from_str(&matched)));
                        for cap in &captures {
                            replacer_args.push(cap.clone());
                        }
                        // Pass UTF-16 position and the primitive string S (non-PUA)
                        replacer_args.push(JsValue::Number(position_utf16 as f64));
                        replacer_args.push(JsValue::String(regex_output_to_js_string(&s_slice)));
                        if !named_captures.is_undefined() {
                            replacer_args.push(named_captures.clone());
                        }
                        let repl_val = interp.call_function(
                            &replace_value,
                            &JsValue::Undefined,
                            &replacer_args,
                        );
                        match repl_val {
                            Completion::Normal(v) => match interp.to_string_value(&v) {
                                Ok(s) => s,
                                Err(e) => {
                                    interp.gc_temp_roots.truncate(gc_root_start);
                                    return Completion::Throw(e);
                                }
                            },
                            other => {
                                interp.gc_temp_roots.truncate(gc_root_start);
                                return other;
                            }
                        }
                    } else {
                        // l. Else (string replace)
                        let template = replace_str.as_ref().unwrap();
                        let named_captures_obj = if !named_captures.is_undefined() {
                            // i. Set namedCaptures to ? ToObject(namedCaptures).
                            match interp.to_object(&named_captures) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => {
                                    interp.gc_temp_roots.truncate(gc_root_start);
                                    return Completion::Throw(e);
                                }
                                _ => JsValue::Undefined,
                            }
                        } else {
                            JsValue::Undefined
                        };
                        let tail_pos = utf16_to_byte_offset(
                            &s_slice,
                            (position_utf16 + match_length_utf16).min(s_utf16_len),
                        );
                        match get_substitution(
                            interp,
                            &matched,
                            &s_slice,
                            position,
                            tail_pos,
                            &captures,
                            &named_captures_obj,
                            template,
                        ) {
                            Ok(s) => s,
                            Err(e) => {
                                interp.gc_temp_roots.truncate(gc_root_start);
                                return Completion::Throw(e);
                            }
                        }
                    };

                    // p. If position >= nextSourcePosition, then
                    let tail_pos_final = utf16_to_byte_offset(
                        &s_slice,
                        (position_utf16 + match_length_utf16).min(s_utf16_len),
                    );
                    if position >= next_source_position {
                        accumulated_result.push_str(&s_slice[next_source_position..position]);
                        accumulated_result.push_str(&replacement);
                        next_source_position = tail_pos_final;
                    }
                }

                interp.gc_temp_roots.truncate(gc_root_start);

                // 15. Return accumulatedResult + remainder of S.
                if next_source_position < length_s {
                    accumulated_result.push_str(&s_slice[next_source_position..]);
                }
                Completion::Normal(JsValue::String(regex_output_to_js_string(
                    &accumulated_result,
                )))
            },
        ));
        if let Some(key) = get_symbol_key(self, "replace") {
            regexp_proto
                .borrow_mut()
                .insert_property(key, PropertyDescriptor::data(replace_fn, true, false, true));
        }

        // [@@split] (§22.2.5.13)
        let split_fn = self.create_function(JsFunction::native(
            "[Symbol.split]".to_string(),
            2,
            |interp, this_val, args| {
                // 1. Let rx be the this value.
                let rx_id = match this_val {
                    JsValue::Object(o) => o.id,
                    _ => {
                        return Completion::Throw(interp.create_type_error(
                            "RegExp.prototype[@@split] requires that 'this' be an Object",
                        ));
                    }
                };
                // 2. Let S be ? ToString(string).
                let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let (s, s_cu) = match to_regex_input_with_units(interp, &arg) {
                    Ok(r) => r,
                    Err(e) => return Completion::Throw(e),
                };

                // 3. Let C be ? SpeciesConstructor(rx, %RegExp%).
                let rx_val = JsValue::Object(crate::types::JsObject { id: rx_id });
                let regexp_ctor = interp
                    .realm()
                    .global_env
                    .borrow()
                    .get("RegExp")
                    .unwrap_or(JsValue::Undefined);
                let c = match interp.species_constructor(&rx_val, &regexp_ctor) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // 4. Let flags be ? ToString(? Get(rx, "flags")).
                let flags_val = match interp.get_object_property(rx_id, "flags", &rx_val) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let flags_str = match interp.to_string_value(&flags_val) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };

                // 5-6. unicodeMatching, newFlags
                let unicode_matching = flags_str.contains('u') || flags_str.contains('v');
                let new_flags = if flags_str.contains('y') {
                    flags_str.clone()
                } else {
                    format!("{flags_str}y")
                };

                // 7. Let splitter be ? Construct(C, « rx, newFlags »).
                let splitter_val = match interp.construct_with_new_target(
                    &c,
                    &[
                        rx_val.clone(),
                        JsValue::String(JsString::from_str(&new_flags)),
                    ],
                    c.clone(),
                ) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Completion::Throw(e),
                    _ => return Completion::Normal(JsValue::Undefined),
                };
                let splitter_id = match &splitter_val {
                    JsValue::Object(o) => o.id,
                    _ => {
                        return Completion::Throw(
                            interp.create_type_error("splitter is not an object"),
                        );
                    }
                };

                // 8. Let A be ! ArrayCreate(0).
                let mut a: Vec<JsValue> = Vec::new();
                // 9. Let lengthA = 0.
                let mut length_a: u32 = 0;

                // 10. Let lim = limit is undefined ? 2^32-1 : ToUint32(limit).
                let limit = args.get(1).cloned().unwrap_or(JsValue::Undefined);
                let lim: u32 = if matches!(limit, JsValue::Undefined) {
                    0xFFFFFFFF
                } else {
                    match interp.to_number_value(&limit) {
                        Ok(n) => to_uint32_f64(n),
                        Err(e) => return Completion::Throw(e),
                    }
                };

                // 11. If lim = 0, return A.
                if lim == 0 {
                    return Completion::Normal(interp.create_array(a));
                }

                let size = pua_aware_utf16_len(&s);

                // 12. If size = 0, then
                if size == 0 {
                    // a. Let z be ? RegExpExec(splitter, S).
                    let z = regexp_exec_abstract(interp, splitter_id, &s, &s_cu);
                    match z {
                        Completion::Normal(ref v) if matches!(v, JsValue::Null) => {
                            a.push(JsValue::String(regex_output_to_js_string(&s)));
                        }
                        Completion::Normal(_) => {}
                        other => return other,
                    }
                    return Completion::Normal(interp.create_array(a));
                }

                // 13. Let p = 0.
                let mut p: usize = 0;
                // 14. Let q = p.
                let mut q: usize = p;

                // 15. Repeat, while q < size,
                while q < size {
                    // a. Perform ? Set(splitter, "lastIndex", 𝔽(q), true).
                    if let Err(e) = spec_set(
                        interp,
                        splitter_id,
                        "lastIndex",
                        JsValue::Number(q as f64),
                        true,
                    ) {
                        return Completion::Throw(e);
                    }

                    // b. Let z be ? RegExpExec(splitter, S).
                    let z = regexp_exec_abstract(interp, splitter_id, &s, &s_cu);
                    let z_val = match z {
                        Completion::Normal(v) => v,
                        other => return other,
                    };

                    // c. If z is null, set q to AdvanceStringIndex(S, q, unicodeMatching).
                    if matches!(z_val, JsValue::Null) {
                        q = advance_string_index(&s, q, unicode_matching);
                        continue;
                    }

                    // d. Else,
                    //   i. Let e be ℝ(? ToLength(? Get(splitter, "lastIndex"))).
                    let splitter_val2 = JsValue::Object(crate::types::JsObject { id: splitter_id });
                    let e_val = match interp.get_object_property(
                        splitter_id,
                        "lastIndex",
                        &splitter_val2,
                    ) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let e_num = match interp.to_number_value(&e_val) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let e_length = if e_num.is_nan() || e_num <= 0.0 {
                        0usize
                    } else {
                        (e_num.min(9007199254740991.0).floor() as usize).min(size)
                    };

                    //   ii. If e = p, set q to AdvanceStringIndex(S, q, unicodeMatching).
                    if e_length == p {
                        q = advance_string_index(&s, q, unicode_matching);
                        continue;
                    }

                    //   iii. Else,
                    // Push substring from p to q (convert UTF-16 positions to byte offsets)
                    let p_byte = utf16_to_byte_offset(&s, p);
                    let q_byte = utf16_to_byte_offset(&s, q);
                    let t = &s[p_byte..q_byte];
                    a.push(JsValue::String(regex_output_to_js_string(t)));
                    length_a += 1;
                    if length_a == lim {
                        return Completion::Normal(interp.create_array(a));
                    }

                    // Set p = e
                    p = e_length;

                    // Get captures from z
                    let z_id = match &z_val {
                        JsValue::Object(o) => o.id,
                        _ => {
                            q = advance_string_index(&s, q, unicode_matching);
                            continue;
                        }
                    };
                    // numberOfCaptures
                    let z_val_ref = z_val.clone();
                    let len_val = match interp.get_object_property(z_id, "length", &z_val_ref) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let len_num = match interp.to_number_value(&len_val) {
                        Ok(n) => n,
                        Err(e) => return Completion::Throw(e),
                    };
                    let number_of_captures = if len_num.is_nan() || len_num <= 0.0 {
                        0usize
                    } else {
                        (len_num.floor() as usize).max(1) - 1
                    };

                    let mut i = 1usize;
                    while i <= number_of_captures {
                        let cap = match interp.get_object_property(z_id, &i.to_string(), &z_val_ref)
                        {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        a.push(cap);
                        length_a += 1;
                        if length_a == lim {
                            return Completion::Normal(interp.create_array(a));
                        }
                        i += 1;
                    }

                    // Set q = p
                    q = p;
                }

                // 16. Push remaining substring
                let p_byte = utf16_to_byte_offset(&s, p);
                let t = &s[p_byte..];
                a.push(JsValue::String(regex_output_to_js_string(t)));
                Completion::Normal(interp.create_array(a))
            },
        ));
        if let Some(key) = get_symbol_key(self, "split") {
            regexp_proto
                .borrow_mut()
                .insert_property(key, PropertyDescriptor::data(split_fn, true, false, true));
        }

        // [@@matchAll] (§22.2.5.7)
        let match_all_fn = self.create_function(JsFunction::native(
            "[Symbol.matchAll]".to_string(),
            1,
            |interp, this_val, args| {
                // 1. Let R be the this value.
                let rx_id = match this_val {
                    JsValue::Object(o) => o.id,
                    _ => {
                        return Completion::Throw(interp.create_type_error(
                            "RegExp.prototype[@@matchAll] requires that 'this' be an Object",
                        ));
                    }
                };
                // 2. Let S be ? ToString(string).
                let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let (s, _s_cu) = match to_regex_input_with_units(interp, &arg) {
                    Ok(r) => r,
                    Err(e) => return Completion::Throw(e),
                };

                // 3. Let C be ? SpeciesConstructor(R, %RegExp%).
                let rx_val = JsValue::Object(crate::types::JsObject { id: rx_id });
                let regexp_ctor = interp
                    .realm()
                    .global_env
                    .borrow()
                    .get("RegExp")
                    .unwrap_or(JsValue::Undefined);
                let c = match interp.species_constructor(&rx_val, &regexp_ctor) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };

                // 4. Let flags be ? ToString(? Get(R, "flags")).
                let flags_val = match interp.get_object_property(rx_id, "flags", &rx_val) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let flags = match interp.to_string_value(&flags_val) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };

                // 5. Let matcher be ? Construct(C, « R, flags »).
                let matcher_val = match interp.construct_with_new_target(
                    &c,
                    &[rx_val.clone(), JsValue::String(JsString::from_str(&flags))],
                    c.clone(),
                ) {
                    Completion::Normal(v) => v,
                    Completion::Throw(e) => return Completion::Throw(e),
                    _ => return Completion::Normal(JsValue::Undefined),
                };
                let matcher_id = match &matcher_val {
                    JsValue::Object(o) => o.id,
                    _ => {
                        return Completion::Throw(
                            interp.create_type_error("matcher is not an object"),
                        );
                    }
                };

                // 6. Let lastIndex be ? ToLength(? Get(R, "lastIndex")).
                let li_val = match interp.get_object_property(rx_id, "lastIndex", &rx_val) {
                    Completion::Normal(v) => v,
                    other => return other,
                };
                let li_num = match interp.to_number_value(&li_val) {
                    Ok(n) => n,
                    Err(e) => return Completion::Throw(e),
                };
                let last_index = if li_num.is_nan() || li_num <= 0.0 {
                    0.0
                } else {
                    li_num.min(9007199254740991.0).floor()
                };

                // 7. Perform ? Set(matcher, "lastIndex", lastIndex, true).
                if let Err(e) = spec_set(
                    interp,
                    matcher_id,
                    "lastIndex",
                    JsValue::Number(last_index),
                    true,
                ) {
                    return Completion::Throw(e);
                }

                // 8-10. global, fullUnicode flags
                let global = flags.contains('g');
                let full_unicode = flags.contains('u') || flags.contains('v');

                // Extract source/flags from the matcher for the iterator state
                let (m_source, m_flags, _) = match extract_source_flags(interp, &matcher_val) {
                    Some(v) => v,
                    None => {
                        // Use empty pattern if matcher has no source/flags
                        (String::new(), String::new(), matcher_id)
                    }
                };

                // Create iterator with %RegExpStringIteratorPrototype%
                let iter_obj = interp.create_object();
                iter_obj.borrow_mut().class_name = "RegExp String Iterator".to_string();
                if let Some(rsi_proto_id) = interp.realm().regexp_string_iterator_prototype {
                    iter_obj.borrow_mut().prototype_id =
                        Some(interp.get_object_expect(rsi_proto_id).borrow().id.unwrap());
                }

                // Store matcher ID for spec-compliant RegExpExec
                iter_obj.borrow_mut().insert_value(
                    "__matcher__".to_string(),
                    JsValue::Number(matcher_id as f64),
                );
                iter_obj.borrow_mut().insert_value(
                    "__full_unicode__".to_string(),
                    JsValue::Boolean(full_unicode),
                );

                iter_obj.borrow_mut().iterator_state = Some(IteratorState::RegExpStringIterator {
                    source: m_source,
                    flags: m_flags,
                    string: s,
                    global,
                    last_index: last_index as usize,
                    done: false,
                });

                let id = iter_obj.borrow().id.unwrap();
                Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
            },
        ));
        if let Some(key) = get_symbol_key(self, "matchAll") {
            regexp_proto.borrow_mut().insert_property(
                key,
                PropertyDescriptor::data(match_all_fn, true, false, true),
            );
        }

        // Setup %RegExpStringIteratorPrototype% (§22.2.9.2)
        let rsi_proto = self.create_object();
        rsi_proto.borrow_mut().class_name = "RegExp String Iterator".to_string();
        if let Some(ip_id) = self.realm().iterator_prototype {
            rsi_proto.borrow_mut().prototype_id =
                Some(self.get_object_expect(ip_id).borrow().id.unwrap());
        }

        // %RegExpStringIteratorPrototype%.next
        let rsi_next_fn = self.create_function(JsFunction::native(
            "next".to_string(),
            0,
            |interp, this_val, _args| {
                let o = match this_val {
                    JsValue::Object(o) => o,
                    _ => {
                        return Completion::Throw(interp.create_type_error(
                            "%RegExpStringIteratorPrototype%.next requires that 'this' be an Object",
                        ));
                    }
                };
                let obj = match interp.get_object(o.id) {
                    Some(obj) => obj,
                    None => {
                        return Completion::Throw(interp.create_type_error(
                            "%RegExpStringIteratorPrototype%.next called on invalid object",
                        ));
                    }
                };
                let state = obj.borrow().iterator_state.clone();
                let matcher_id_val = interp.get_property_on_id(o.id, "__matcher__");
                let full_unicode_val = interp.get_property_on_id(o.id, "__full_unicode__");
                let full_unicode = matches!(full_unicode_val, JsValue::Boolean(true));

                let (source, flags, string, global, last_index, done) =
                    if let Some(IteratorState::RegExpStringIterator {
                        ref source,
                        ref flags,
                        ref string,
                        global,
                        last_index,
                        done,
                    }) = state
                    {
                        (
                            source.clone(),
                            flags.clone(),
                            string.clone(),
                            global,
                            last_index,
                            done,
                        )
                    } else {
                        return Completion::Throw(interp.create_type_error(
                            "%RegExpStringIteratorPrototype%.next requires that 'this' be a RegExp String Iterator",
                        ));
                    };

                if done {
                    return Completion::Normal(
                        interp.create_iter_result_object(JsValue::Undefined, true),
                    );
                }

                // If we have a matcher object, use RegExpExec
                if let JsValue::Number(mid) = matcher_id_val {
                    let mid = mid as u64;
                    let string_cu = regex_output_to_js_string(&string).code_units;
                    let result = regexp_exec_abstract(interp, mid, &string, &string_cu);
                    let result_val = match result {
                        Completion::Normal(v) => v,
                        other => return other,
                    };

                    if matches!(result_val, JsValue::Null) {
                        if let Some(obj2) = interp.get_object(o.id) {
                            obj2.borrow_mut().iterator_state =
                                Some(IteratorState::RegExpStringIterator {
                                    source, flags, string, global,
                                    last_index, done: true,
                                });
                        }
                        return Completion::Normal(
                            interp.create_iter_result_object(JsValue::Undefined, true),
                        );
                    }

                    if !global {
                        if let Some(obj2) = interp.get_object(o.id) {
                            obj2.borrow_mut().iterator_state =
                                Some(IteratorState::RegExpStringIterator {
                                    source, flags, string, global,
                                    last_index, done: true,
                                });
                        }
                        return Completion::Normal(
                            interp.create_iter_result_object(result_val, false),
                        );
                    }

                    // Global: check for empty match, advance if needed
                    let result_id = if let JsValue::Object(ro) = &result_val {
                        ro.id
                    } else {
                        if let Some(obj2) = interp.get_object(o.id) {
                            obj2.borrow_mut().iterator_state =
                                Some(IteratorState::RegExpStringIterator {
                                    source, flags, string, global,
                                    last_index, done: true,
                                });
                        }
                        return Completion::Normal(
                            interp.create_iter_result_object(result_val, false),
                        );
                    };
                    let match_str_val = match interp.get_object_property(
                        result_id, "0", &result_val,
                    ) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let match_str = match interp.to_string_value(&match_str_val) {
                        Ok(s) => s,
                        Err(e) => return Completion::Throw(e),
                    };
                    if match_str.is_empty() {
                        let matcher_val2 =
                            JsValue::Object(crate::types::JsObject { id: mid });
                        let li_val = match interp.get_object_property(
                            mid, "lastIndex", &matcher_val2,
                        ) {
                            Completion::Normal(v) => v,
                            other => return other,
                        };
                        let li_num = match interp.to_number_value(&li_val) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        };
                        let this_index = if li_num.is_nan() || li_num <= 0.0 {
                            0
                        } else {
                            li_num.min(9007199254740991.0).floor() as usize
                        };
                        let next_index =
                            advance_string_index(&string, this_index, full_unicode);
                        if let Err(e) = spec_set(
                            interp, mid, "lastIndex",
                            JsValue::Number(next_index as f64), true,
                        ) {
                            return Completion::Throw(e);
                        }
                    }

                    if let Some(obj2) = interp.get_object(o.id) {
                        obj2.borrow_mut().iterator_state =
                            Some(IteratorState::RegExpStringIterator {
                                source, flags, string, global,
                                last_index, done: false,
                            });
                    }
                    return Completion::Normal(
                        interp.create_iter_result_object(result_val, false),
                    );
                }

                // Fallback: use raw regex (legacy path)
                let re = match build_regex(&source, &flags) {
                    Ok(r) => r,
                    Err(_) => {
                        if let Some(obj2) = interp.get_object(o.id) {
                            obj2.borrow_mut().iterator_state =
                                Some(IteratorState::RegExpStringIterator {
                                    source, flags, string, global,
                                    last_index, done: true,
                                });
                        }
                        return Completion::Normal(
                            interp.create_iter_result_object(JsValue::Undefined, true),
                        );
                    }
                };

                if last_index > string.len() {
                    if let Some(obj2) = interp.get_object(o.id) {
                        obj2.borrow_mut().iterator_state =
                            Some(IteratorState::RegExpStringIterator {
                                source, flags, string, global,
                                last_index, done: true,
                            });
                    }
                    return Completion::Normal(
                        interp.create_iter_result_object(JsValue::Undefined, true),
                    );
                }

                match regex_captures(&re, &string[last_index..]) {
                    None => {
                        if let Some(obj2) = interp.get_object(o.id) {
                            obj2.borrow_mut().iterator_state =
                                Some(IteratorState::RegExpStringIterator {
                                    source, flags, string, global,
                                    last_index, done: true,
                                });
                        }
                        Completion::Normal(
                            interp.create_iter_result_object(JsValue::Undefined, true),
                        )
                    }
                    Some(caps) => {
                        let full = caps.get(0).unwrap();
                        let match_start = last_index + full.start;
                        let match_end = last_index + full.end;

                        let mut elements: Vec<JsValue> = Vec::new();
                        elements.push(JsValue::String(JsString::from_str(&full.text)));
                        for i in 1..caps.len() {
                            match caps.get(i) {
                                Some(m) => elements.push(JsValue::String(
                                    JsString::from_str(&m.text),
                                )),
                                None => elements.push(JsValue::Undefined),
                            }
                        }

                        let result_arr = interp.create_array(elements);
                        if let JsValue::Object(ref ro) = result_arr
                            && let Some(robj) = interp.get_object(ro.id)
                        {
                            robj.borrow_mut().insert_value(
                                "index".to_string(),
                                JsValue::Number(match_start as f64),
                            );
                            robj.borrow_mut().insert_value(
                                "input".to_string(),
                                JsValue::String(JsString::from_str(&string)),
                            );
                            robj.borrow_mut().insert_value(
                                "groups".to_string(),
                                JsValue::Undefined,
                            );
                        }

                        let new_last_index = if global {
                            if full.text.is_empty() { match_end + 1 } else { match_end }
                        } else {
                            last_index
                        };
                        let new_done = !global;

                        if let Some(obj2) = interp.get_object(o.id) {
                            obj2.borrow_mut().iterator_state =
                                Some(IteratorState::RegExpStringIterator {
                                    source, flags, string, global,
                                    last_index: new_last_index,
                                    done: new_done,
                                });
                        }

                        Completion::Normal(
                            interp.create_iter_result_object(result_arr, false),
                        )
                    }
                }
            },
        ));
        rsi_proto.borrow_mut().insert_property(
            "next".to_string(),
            PropertyDescriptor::data(rsi_next_fn, true, false, true),
        );

        // %RegExpStringIteratorPrototype%[@@toStringTag]
        if let Some(key) = get_symbol_key(self, "toStringTag") {
            rsi_proto.borrow_mut().insert_property(
                key,
                PropertyDescriptor::data(
                    JsValue::String(JsString::from_str("RegExp String Iterator")),
                    false,
                    false,
                    true,
                ),
            );
        }

        self.realm_mut().regexp_string_iterator_prototype = Some(rsi_proto.borrow().id.unwrap());

        // RegExp.prototype.flags getter (§22.2.5.3)
        let flags_getter = self.create_function(JsFunction::native(
            "get flags".to_string(),
            0,
            |interp, this_val, _args| {
                let obj_id = match this_val {
                    JsValue::Object(o) => o.id,
                    _ => {
                        let err = interp.create_type_error(
                            "RegExp.prototype.flags requires that 'this' be an Object",
                        );
                        return Completion::Throw(err);
                    }
                };
                if interp.get_object(obj_id).is_none() {
                    let err = interp.create_type_error(
                        "RegExp.prototype.flags requires that 'this' be an Object",
                    );
                    return Completion::Throw(err);
                }
                let mut result = String::new();
                let flags_to_check: &[(&str, char)] = &[
                    ("hasIndices", 'd'),
                    ("global", 'g'),
                    ("ignoreCase", 'i'),
                    ("multiline", 'm'),
                    ("dotAll", 's'),
                    ("unicode", 'u'),
                    ("unicodeSets", 'v'),
                    ("sticky", 'y'),
                ];
                for (prop, ch) in flags_to_check {
                    let val = match interp.get_object_property(obj_id, prop, this_val) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    if interp.to_boolean_val(&val) {
                        result.push(*ch);
                    }
                }
                Completion::Normal(JsValue::String(JsString::from_str(&result)))
            },
        ));
        regexp_proto.borrow_mut().insert_property(
            "flags".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(flags_getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );

        // Flag property getters on prototype (§22.2.5.x)
        let flag_props: &[(&str, char)] = &[
            ("global", 'g'),
            ("ignoreCase", 'i'),
            ("multiline", 'm'),
            ("dotAll", 's'),
            ("unicode", 'u'),
            ("unicodeSets", 'v'),
            ("sticky", 'y'),
            ("hasIndices", 'd'),
        ];
        let regexp_proto_id = regexp_proto.borrow().id.unwrap();
        let my_realm_id = self.current_realm_id;
        for &(prop_name, flag_char) in flag_props {
            let name = prop_name.to_string();
            let getter = self.create_function(JsFunction::native(
                format!("get {}", name),
                0,
                move |interp, this_val, _args| {
                    let obj_ref = match this_val {
                        JsValue::Object(o) => o,
                        _ => {
                            return Completion::Throw(interp.create_error_in_realm(
                                my_realm_id,
                                "TypeError",
                                &format!(
                                    "RegExp.prototype.{} requires that 'this' be an Object",
                                    name
                                ),
                            ));
                        }
                    };
                    let obj = match interp.get_object(obj_ref.id) {
                        Some(o) => o,
                        None => return Completion::Normal(JsValue::Undefined),
                    };
                    if obj.borrow().class_name != "RegExp" {
                        if obj_ref.id == regexp_proto_id {
                            return Completion::Normal(JsValue::Undefined);
                        }
                        return Completion::Throw(interp.create_error_in_realm(
                            my_realm_id,
                            "TypeError",
                            &format!(
                                "RegExp.prototype.{} requires that 'this' be a RegExp object",
                                name
                            ),
                        ));
                    }
                    let flags_opt = obj.borrow().regexp_original_flags.clone();
                    if let Some(s) = flags_opt {
                        Completion::Normal(JsValue::Boolean(s.to_rust_string().contains(flag_char)))
                    } else {
                        Completion::Normal(JsValue::Undefined)
                    }
                },
            ));
            regexp_proto.borrow_mut().insert_property(
                prop_name.to_string(),
                PropertyDescriptor {
                    value: None,
                    writable: None,
                    get: Some(getter),
                    set: None,
                    enumerable: Some(false),
                    configurable: Some(true),
                },
            );
        }

        // source getter (§22.2.5.10)
        let source_getter = self.create_function(JsFunction::native(
            "get source".to_string(),
            0,
            move |interp, this_val, _args| {
                let obj_ref = match this_val {
                    JsValue::Object(o) => o,
                    _ => {
                        return Completion::Throw(interp.create_error_in_realm(
                            my_realm_id,
                            "TypeError",
                            "RegExp.prototype.source requires that 'this' be an Object",
                        ));
                    }
                };
                let obj = match interp.get_object(obj_ref.id) {
                    Some(o) => o,
                    None => return Completion::Normal(JsValue::Undefined),
                };
                if obj.borrow().class_name != "RegExp" {
                    if obj_ref.id == regexp_proto_id {
                        return Completion::Normal(JsValue::String(JsString::from_str("(?:)")));
                    }
                    return Completion::Throw(interp.create_error_in_realm(
                        my_realm_id,
                        "TypeError",
                        "RegExp.prototype.source requires that 'this' be a RegExp object",
                    ));
                }
                let source_opt = obj.borrow().regexp_original_source.clone();
                if let Some(ref s) = source_opt {
                    let escaped = escape_regexp_pattern_code_units(&s.code_units);
                    Completion::Normal(JsValue::String(escaped))
                } else {
                    Completion::Normal(JsValue::String(JsString::from_str("(?:)")))
                }
            },
        ));
        regexp_proto.borrow_mut().insert_property(
            "source".to_string(),
            PropertyDescriptor {
                value: None,
                writable: None,
                get: Some(source_getter),
                set: None,
                enumerable: Some(false),
                configurable: Some(true),
            },
        );

        let regexp_proto_rc_id = regexp_proto.borrow().id.unwrap();

        // RegExp constructor
        let regexp_ctor = self.create_function(JsFunction::constructor(
            "RegExp".to_string(),
            2,
            move |interp, _this, args| {
                let pattern_arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let flags_arg = args.get(1).cloned().unwrap_or(JsValue::Undefined);

                // IsRegExp check per §7.2.8
                let is_regexp_obj = if let JsValue::Object(ref o) = pattern_arg {
                    let match_key = get_symbol_key(interp, "match");
                    if let Some(key) = match_key {
                        let matcher = match interp.get_object_property(o.id, &key, &pattern_arg) {
                            Completion::Normal(v) => v,
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => JsValue::Undefined,
                        };
                        if !matches!(matcher, JsValue::Undefined) {
                            interp.to_boolean_val(&matcher)
                        } else {
                            // Symbol.match is undefined, check [[RegExpMatcher]]
                            if let Some(obj) = interp.get_object(o.id) {
                                obj.borrow().class_name == "RegExp"
                            } else {
                                false
                            }
                        }
                    } else if let Some(obj) = interp.get_object(o.id) {
                        obj.borrow().class_name == "RegExp"
                    } else {
                        false
                    }
                } else {
                    false
                };

                // §22.2.3.1 step 2: If NewTarget is undefined (called as function, not new)
                if interp.new_target.is_none()
                    && is_regexp_obj
                    && matches!(flags_arg, JsValue::Undefined)
                    && let JsValue::Object(ref o) = pattern_arg
                {
                    // Get pattern.constructor
                    let ctor = match interp.get_object_property(o.id, "constructor", &pattern_arg) {
                        Completion::Normal(v) => v,
                        Completion::Throw(e) => return Completion::Throw(e),
                        _ => JsValue::Undefined,
                    };
                    // Get the active function object (RegExp constructor)
                    let regexp_fn = interp
                        .realm()
                        .global_env
                        .borrow()
                        .get("RegExp")
                        .unwrap_or(JsValue::Undefined);
                    if same_value(&regexp_fn, &ctor) {
                        return Completion::Normal(pattern_arg.clone());
                    }
                }

                // §22.2.3.1 steps 3-4: Extract P and F from arguments.
                // Source is extracted fully (freezing it before prototype lookup),
                // but flags ToString is deferred until after RegExpAlloc (step 5).
                let has_regexp_matcher = if let JsValue::Object(ref o) = pattern_arg {
                    interp
                        .get_object(o.id)
                        .is_some_and(|obj| obj.borrow().class_name == "RegExp")
                } else {
                    false
                };

                // Phase 1: Extract source and raw flags value
                enum RawFlags {
                    Resolved(String),
                    NeedsToString(JsValue),
                }
                let (pattern_js, pattern_str, raw_flags) =
                    if has_regexp_matcher && let JsValue::Object(ref o) = pattern_arg {
                        let src_js = interp
                            .get_object(o.id)
                            .and_then(|obj| obj.borrow().regexp_original_source.clone())
                            .unwrap_or_else(|| JsString::from_str(""));
                        let src = js_string_to_regex_input(&src_js.code_units);
                        let flg = if matches!(flags_arg, JsValue::Undefined) {
                            RawFlags::Resolved(
                                interp
                                    .get_object(o.id)
                                    .and_then(|obj| {
                                        obj.borrow()
                                            .regexp_original_flags
                                            .as_ref()
                                            .map(|s| s.to_string())
                                    })
                                    .unwrap_or_default(),
                            )
                        } else {
                            RawFlags::NeedsToString(flags_arg.clone())
                        };
                        (src_js, src, flg)
                    } else if is_regexp_obj && let JsValue::Object(ref o) = pattern_arg {
                        let src = match interp.get_object_property(o.id, "source", &pattern_arg) {
                            Completion::Normal(v) => match interp.to_js_string(&v) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            },
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => JsString::from_str(""),
                        };
                        let src_str = js_string_to_regex_input(&src.code_units);
                        let flg = if matches!(flags_arg, JsValue::Undefined) {
                            let f = match interp.get_object_property(o.id, "flags", &pattern_arg) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => JsValue::Undefined,
                            };
                            RawFlags::NeedsToString(f)
                        } else {
                            RawFlags::NeedsToString(flags_arg.clone())
                        };
                        (src, src_str, flg)
                    } else {
                        let p_js = if matches!(pattern_arg, JsValue::Undefined) {
                            JsString::from_str("")
                        } else {
                            match interp.to_js_string(&pattern_arg) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            }
                        };
                        let p_str = js_string_to_regex_input(&p_js.code_units);
                        let flg = if matches!(flags_arg, JsValue::Undefined) {
                            RawFlags::Resolved(String::new())
                        } else {
                            RawFlags::NeedsToString(flags_arg.clone())
                        };
                        (p_js, p_str, flg)
                    };

                // §22.2.3.1 step 5: RegExpAlloc — get prototype from new target
                let proto = match interp
                    .get_prototype_from_new_target_realm(|realm| realm.regexp_prototype)
                {
                    Ok(p) => p,
                    Err(e) => return Completion::Throw(e),
                };

                // Phase 2: Now ToString flags (after prototype lookup)
                let flags_str = match raw_flags {
                    RawFlags::Resolved(s) => s,
                    RawFlags::NeedsToString(v) => match interp.to_string_value(&v) {
                        Ok(s) => s,
                        Err(e) => return Completion::Throw(e),
                    },
                };

                // Validate flags: only dgimsuy allowed, no duplicates
                let valid_flags = "dgimsuyv";
                for c in flags_str.chars() {
                    if !valid_flags.contains(c) {
                        return Completion::Throw(interp.create_error(
                            "SyntaxError",
                            &format!("Invalid regular expression flags '{}'", flags_str),
                        ));
                    }
                }
                let mut seen = HashSet::default();
                for c in flags_str.chars() {
                    if !seen.insert(c) {
                        return Completion::Throw(interp.create_error(
                            "SyntaxError",
                            &format!("Invalid regular expression flags '{}'", flags_str),
                        ));
                    }
                }

                // u and v flags are mutually exclusive
                if flags_str.contains('u') && flags_str.contains('v') {
                    return Completion::Throw(interp.create_error(
                        "SyntaxError",
                        &format!("Invalid regular expression flags '{}'", flags_str),
                    ));
                }

                // Validate the pattern
                if let Err(msg) = validate_js_pattern(&pattern_str, &flags_str) {
                    return Completion::Throw(interp.create_error("SyntaxError", &msg));
                }

                // Empty source → "(?:)" per spec
                let source_js = if pattern_js.code_units.is_empty() {
                    JsString::from_str("(?:)")
                } else {
                    pattern_js
                };

                let mut obj = JsObjectData::new();
                obj.prototype_id = proto.or(Some(regexp_proto_rc_id));
                obj.class_name = "RegExp".to_string();
                // Store internal slots as non-enumerable hidden properties
                obj.regexp_original_source = Some(source_js);
                obj.regexp_original_flags = Some(JsString::from_str(&flags_str));
                obj.insert_property(
                    "lastIndex".to_string(),
                    PropertyDescriptor::data(JsValue::Number(0.0), true, false, false),
                );
                let id = interp.alloc_object(obj);
                Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
            },
        ));
        // RegExp.escape (§22.2.5.1)
        let escape_fn = self.create_function(JsFunction::native(
            "escape".to_string(),
            1,
            |interp, _this, args| {
                let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                let code_units = match &arg {
                    JsValue::String(s) => s.code_units.clone(),
                    _ => {
                        let err =
                            interp.create_type_error("RegExp.escape requires a string argument");
                        return Completion::Throw(err);
                    }
                };
                let mut result_units: Vec<u16> = Vec::new();
                let mut idx = 0;
                let mut is_first = true;
                while idx < code_units.len() {
                    let cu = code_units[idx];
                    let cp = if (0xD800..=0xDBFF).contains(&cu) && idx + 1 < code_units.len() {
                        let lo = code_units[idx + 1];
                        if (0xDC00..=0xDFFF).contains(&lo) {
                            let cp = ((cu as u32 - 0xD800) << 10) + (lo as u32 - 0xDC00) + 0x10000;
                            idx += 2;
                            Some(cp)
                        } else {
                            idx += 1;
                            None
                        }
                    } else {
                        idx += 1;
                        if (0xD800..=0xDFFF).contains(&cu) {
                            None
                        } else {
                            Some(cu as u32)
                        }
                    };
                    if let Some(cp) = cp {
                        if let Some(c) = char::from_u32(cp) {
                            let escaped = if is_first && c.is_ascii_alphanumeric() {
                                format!("\\x{:02x}", cp)
                            } else {
                                encode_for_regexp_escape(c)
                            };
                            for ecu in escaped.encode_utf16() {
                                result_units.push(ecu);
                            }
                        }
                    } else {
                        let escaped = format!("\\u{:04x}", cu);
                        for ecu in escaped.encode_utf16() {
                            result_units.push(ecu);
                        }
                    }
                    is_first = false;
                }
                Completion::Normal(JsValue::String(JsString {
                    code_units: result_units,
                }))
            },
        ));

        if let JsValue::Object(ref o) = regexp_ctor
            && let Some(obj) = self.get_object(o.id)
        {
            obj.borrow_mut().deferred_construct = true;
            obj.borrow_mut().insert_builtin(
                "prototype".to_string(),
                JsValue::Object(crate::types::JsObject {
                    id: regexp_proto.borrow().id.unwrap(),
                }),
            );
            obj.borrow_mut()
                .insert_builtin("escape".to_string(), escape_fn);

            // RegExp[Symbol.species] getter
            let species_getter = self.create_function(JsFunction::native(
                "get [Symbol.species]".to_string(),
                0,
                |_interp, this_val, _args| Completion::Normal(this_val.clone()),
            ));
            obj.borrow_mut().insert_property(
                "Symbol(Symbol.species)".to_string(),
                PropertyDescriptor {
                    value: None,
                    writable: None,
                    get: Some(species_getter),
                    set: None,
                    enumerable: Some(false),
                    configurable: Some(true),
                },
            );

            // Annex B legacy static accessor properties (B.2.4)
            let ctor_id = o.id;
            self.regexp_constructor_id = Some(ctor_id);

            // Helper macro for legacy accessor property getters/setters
            // $1..$9 — get-only
            for idx in 1u8..=9 {
                let prop_name = format!("${}", idx);
                let getter =
                    self.create_function(JsFunction::native(
                        format!("get ${}", idx),
                        0,
                        move |interp, this_val, _args| {
                            match this_val {
                                JsValue::Object(o) if o.id == ctor_id => {}
                                _ => return Completion::Throw(interp.create_type_error(
                                    "RegExp legacy accessor requires RegExp constructor as this",
                                )),
                            }
                            let val = interp.regexp_legacy_parens[(idx - 1) as usize].clone();
                            Completion::Normal(JsValue::String(JsString::from_str(&val)))
                        },
                    ));
                obj.borrow_mut().insert_property(
                    prop_name,
                    PropertyDescriptor {
                        value: None,
                        writable: None,
                        get: Some(getter),
                        set: None,
                        enumerable: Some(false),
                        configurable: Some(true),
                    },
                );
            }

            // input / $_ — get/set
            for prop_name in &["input", "$_"] {
                let pn = prop_name.to_string();
                let getter =
                    self.create_function(JsFunction::native(
                        format!("get {}", pn),
                        0,
                        move |interp, this_val, _args| {
                            match this_val {
                                JsValue::Object(o) if o.id == ctor_id => {}
                                _ => return Completion::Throw(interp.create_type_error(
                                    "RegExp legacy accessor requires RegExp constructor as this",
                                )),
                            }
                            Completion::Normal(JsValue::String(JsString::from_str(
                                &interp.regexp_legacy_input,
                            )))
                        },
                    ));
                let setter =
                    self.create_function(JsFunction::native(
                        format!("set {}", pn),
                        1,
                        move |interp, this_val, args| {
                            match this_val {
                                JsValue::Object(o) if o.id == ctor_id => {}
                                _ => return Completion::Throw(interp.create_type_error(
                                    "RegExp legacy accessor requires RegExp constructor as this",
                                )),
                            }
                            let val = args.first().cloned().unwrap_or(JsValue::Undefined);
                            let s = match interp.to_string_value(&val) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            };
                            interp.regexp_legacy_input = s;
                            Completion::Normal(JsValue::Undefined)
                        },
                    ));
                obj.borrow_mut().insert_property(
                    pn,
                    PropertyDescriptor {
                        value: None,
                        writable: None,
                        get: Some(getter),
                        set: Some(setter),
                        enumerable: Some(false),
                        configurable: Some(true),
                    },
                );
            }

            // lastMatch / $& — get-only
            for prop_name in &["lastMatch", "$&"] {
                let pn = prop_name.to_string();
                let getter =
                    self.create_function(JsFunction::native(
                        format!("get {}", pn),
                        0,
                        move |interp, this_val, _args| {
                            match this_val {
                                JsValue::Object(o) if o.id == ctor_id => {}
                                _ => return Completion::Throw(interp.create_type_error(
                                    "RegExp legacy accessor requires RegExp constructor as this",
                                )),
                            }
                            Completion::Normal(JsValue::String(JsString::from_str(
                                &interp.regexp_legacy_last_match,
                            )))
                        },
                    ));
                obj.borrow_mut().insert_property(
                    pn,
                    PropertyDescriptor {
                        value: None,
                        writable: None,
                        get: Some(getter),
                        set: None,
                        enumerable: Some(false),
                        configurable: Some(true),
                    },
                );
            }

            // lastParen / $+ — get-only
            for prop_name in &["lastParen", "$+"] {
                let pn = prop_name.to_string();
                let getter =
                    self.create_function(JsFunction::native(
                        format!("get {}", pn),
                        0,
                        move |interp, this_val, _args| {
                            match this_val {
                                JsValue::Object(o) if o.id == ctor_id => {}
                                _ => return Completion::Throw(interp.create_type_error(
                                    "RegExp legacy accessor requires RegExp constructor as this",
                                )),
                            }
                            Completion::Normal(JsValue::String(JsString::from_str(
                                &interp.regexp_legacy_last_paren,
                            )))
                        },
                    ));
                obj.borrow_mut().insert_property(
                    pn,
                    PropertyDescriptor {
                        value: None,
                        writable: None,
                        get: Some(getter),
                        set: None,
                        enumerable: Some(false),
                        configurable: Some(true),
                    },
                );
            }

            // leftContext / $` — get-only
            for prop_name in &["leftContext", "$`"] {
                let pn = prop_name.to_string();
                let getter =
                    self.create_function(JsFunction::native(
                        format!("get {}", pn),
                        0,
                        move |interp, this_val, _args| {
                            match this_val {
                                JsValue::Object(o) if o.id == ctor_id => {}
                                _ => return Completion::Throw(interp.create_type_error(
                                    "RegExp legacy accessor requires RegExp constructor as this",
                                )),
                            }
                            Completion::Normal(JsValue::String(JsString::from_str(
                                &interp.regexp_legacy_left_context,
                            )))
                        },
                    ));
                obj.borrow_mut().insert_property(
                    pn,
                    PropertyDescriptor {
                        value: None,
                        writable: None,
                        get: Some(getter),
                        set: None,
                        enumerable: Some(false),
                        configurable: Some(true),
                    },
                );
            }

            // rightContext / $' — get-only
            for prop_name in &["rightContext", "$'"] {
                let pn = prop_name.to_string();
                let getter =
                    self.create_function(JsFunction::native(
                        format!("get {}", pn),
                        0,
                        move |interp, this_val, _args| {
                            match this_val {
                                JsValue::Object(o) if o.id == ctor_id => {}
                                _ => return Completion::Throw(interp.create_type_error(
                                    "RegExp legacy accessor requires RegExp constructor as this",
                                )),
                            }
                            Completion::Normal(JsValue::String(JsString::from_str(
                                &interp.regexp_legacy_right_context,
                            )))
                        },
                    ));
                obj.borrow_mut().insert_property(
                    pn,
                    PropertyDescriptor {
                        value: None,
                        writable: None,
                        get: Some(getter),
                        set: None,
                        enumerable: Some(false),
                        configurable: Some(true),
                    },
                );
            }

            regexp_proto
                .borrow_mut()
                .insert_builtin("constructor".to_string(), regexp_ctor.clone());
        }
        self.realm()
            .global_env
            .borrow_mut()
            .declare("RegExp", BindingKind::Var);
        let _ = self
            .realm()
            .global_env
            .borrow_mut()
            .set("RegExp", regexp_ctor);

        self.realm_mut().regexp_prototype = Some(regexp_proto.borrow().id.unwrap());
    }

    pub(crate) fn get_symbol_key(&self, name: &str) -> Option<String> {
        get_symbol_key(self, name)
    }
}
