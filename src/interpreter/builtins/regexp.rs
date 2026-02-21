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

fn is_surrogate(cp: u32) -> bool {
    (SURROGATE_START..=SURROGATE_END).contains(&cp)
}

fn surrogate_to_pua(cp: u32) -> char {
    char::from_u32(SURROGATE_PUA_BASE + (cp - SURROGATE_START)).unwrap()
}

fn pua_to_surrogate(c: char) -> Option<u16> {
    let cp = c as u32;
    if cp >= SURROGATE_PUA_BASE && cp <= SURROGATE_PUA_BASE + (SURROGATE_END - SURROGATE_START) {
        Some((cp - SURROGATE_PUA_BASE + SURROGATE_START) as u16)
    } else {
        None
    }
}

/// Convert a JsString (UTF-16) to a Rust String for regex matching.
/// Lone surrogates are mapped to PUA characters so they survive the conversion
/// and can be matched by patterns that also use the PUA mapping.
pub(crate) fn js_string_to_regex_input(code_units: &[u16]) -> String {
    let mut result = String::new();
    let mut i = 0;
    while i < code_units.len() {
        let cu = code_units[i];
        if (0xD800..=0xDBFF).contains(&cu) {
            if i + 1 < code_units.len() && (0xDC00..=0xDFFF).contains(&code_units[i + 1]) {
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
            if cp >= SURROGATE_PUA_BASE
                && cp <= SURROGATE_PUA_BASE + (SURROGATE_END - SURROGATE_START)
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

/// Convert a JsValue to a String suitable for regex matching.
/// For JsString values, uses PUA mapping for lone surrogates.
/// For other types, falls back to normal ToString conversion.
fn to_regex_input(interp: &mut Interpreter, val: &JsValue) -> Result<String, JsValue> {
    match val {
        JsValue::String(s) => Ok(js_string_to_regex_input(&s.code_units)),
        _ => interp.to_string_value(val),
    }
}

fn escape_regexp_pattern(source: &str) -> String {
    if source.is_empty() {
        return "(?:)".to_string();
    }
    let mut result = String::with_capacity(source.len());
    for c in source.chars() {
        match c {
            '/' => result.push_str("\\/"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\u{2028}' => result.push_str("\\u2028"),
            '\u{2029}' => result.push_str("\\u2029"),
            _ => result.push(c),
        }
    }
    result
}

fn escape_regexp_pattern_code_units(code_units: &[u16]) -> JsString {
    if code_units.is_empty() {
        return JsString::from_str("(?:)");
    }
    let mut result: Vec<u16> = Vec::with_capacity(code_units.len());
    for &cu in code_units {
        match cu {
            0x002F => result.extend_from_slice(&[0x005C, 0x002F]), // / -> \/
            0x000A => result.extend_from_slice(&[0x005C, 0x006E]), // LF -> \n
            0x000D => result.extend_from_slice(&[0x005C, 0x0072]), // CR -> \r
            0x2028 => result.extend_from_slice(&[0x005C, 0x0075, 0x0032, 0x0030, 0x0032, 0x0038]),
            0x2029 => result.extend_from_slice(&[0x005C, 0x0075, 0x0032, 0x0030, 0x0032, 0x0039]),
            _ => result.push(cu),
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
        let b_strings: std::collections::HashSet<&str> =
            b.strings.iter().map(|s| s.as_str()).collect();
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
        let b_strings: std::collections::HashSet<&str> =
            b.strings.iter().map(|s| s.as_str()).collect();
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
        let mut seen = std::collections::HashSet::new();
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
        s.strings.sort_by(|a, b| b.len().cmp(&a.len()));

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

/// Parse a v-flag character class starting right after the opening `[`.
/// Returns (VClassSet, new_index_after_closing_bracket).
fn parse_v_flag_class(
    chars: &[char],
    start: usize,
    flags: &str,
) -> Result<(VClassSet, usize), String> {
    let len = chars.len();
    let mut i = start;
    let negated = i < len && chars[i] == '^';
    if negated {
        i += 1;
    }

    // Parse the first operand
    let mut result = parse_v_class_operand(chars, &mut i, flags)?;

    // Check for set operations
    while i < len && chars[i] != ']' {
        if i + 1 < len && chars[i] == '-' && chars[i + 1] == '-' {
            // Difference: --
            i += 2;
            let rhs = parse_v_class_operand(chars, &mut i, flags)?;
            result = result.difference(&rhs);
        } else if i + 1 < len && chars[i] == '&' && chars[i + 1] == '&' {
            // Intersection: &&
            i += 2;
            let rhs = parse_v_class_operand(chars, &mut i, flags)?;
            result = result.intersect(&rhs);
        } else if i < len && chars[i] == '[' {
            // Nested class in union position
            i += 1;
            let (set, new_i) = parse_v_flag_class(chars, i, flags)?;
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
fn parse_v_class_operand(chars: &[char], i: &mut usize, flags: &str) -> Result<VClassSet, String> {
    let len = chars.len();
    if *i < len && chars[*i] == '[' {
        // Nested character class
        *i += 1;
        let (set, new_i) = parse_v_flag_class(chars, *i, flags)?;
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
        if *i + 1 < len {
            if (c == '-' && chars[*i + 1] == '-') || (c == '&' && chars[*i + 1] == '&') {
                break;
            }
        }
        if c == '[' {
            // Nested class in union position
            *i += 1;
            let (set, new_i) = parse_v_flag_class(chars, *i, flags)?;
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
                    result.add_codepoint(cp);
                    return check_range(chars, i, result, flags);
                }
                return Ok(result);
            }
        }
    }

    // Regular character
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
            if let Some(cp) = parse_v_class_escape(&chars, &mut i) {
                if let Some(ch) = char::from_u32(cp) {
                    result.push(ch);
                }
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

struct TranslationResult {
    pattern: String,
    dup_group_map: std::collections::HashMap<String, Vec<(String, u32)>>,
}

fn translate_js_pattern(source: &str, flags: &str) -> Result<String, String> {
    translate_js_pattern_ex(source, flags).map(|r| r.pattern)
}

fn translate_js_pattern_ex(source: &str, flags: &str) -> Result<TranslationResult, String> {
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
    let mut groups_seen: u32 = 0;
    let dot_all_base = flags.contains('s');
    // Stack for tracking dotAll state through modifier groups.
    // Each entry is Some(previous_dotall) for modifier groups that change s,
    // or None for regular groups.
    let mut dotall_stack: Vec<Option<bool>> = Vec::new();
    let mut dot_all = dot_all_base;

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
                '(' if !in_cc && j + 2 < len && chars[j + 1] == '?' && chars[j + 2] == '<' => {
                    if j + 3 < len && chars[j + 3] != '=' && chars[j + 3] != '!' {
                        // Named group — extract name
                        let name_start = j + 3;
                        let mut k = name_start;
                        while k < len && chars[k] != '>' {
                            k += 1;
                        }
                        if k < len {
                            let name: String = chars[name_start..k].iter().collect();
                            all_group_names.push(name);
                        }
                    }
                }
                _ => {}
            }
            j += 1;
        }
    }
    let mut name_count: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for name in &all_group_names {
        *name_count.entry(name.clone()).or_insert(0) += 1;
    }
    let duplicated_names: std::collections::HashSet<String> = name_count
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(name, _)| name)
        .collect();
    // Track how many times we've seen each duplicated name during translation
    let mut dup_seen_count: std::collections::HashMap<String, u32> =
        std::collections::HashMap::new();
    let mut dup_group_map: std::collections::HashMap<String, Vec<(String, u32)>> =
        std::collections::HashMap::new();

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
            result.push(c);
            i += 1;
            continue;
        }
        // JS treats '[' as a literal inside a character class (without v-flag),
        // but fancy_regex interprets it as a nested class. Escape it.
        if c == '[' && in_char_class {
            result.push_str("\\[");
            i += 1;
            continue;
        }

        if c == '\\' && i + 1 < len {
            let next = chars[i + 1];
            match next {
                // Named backreference: \k<name> → (?P=name) or alternation for duplicates
                'k' if !in_char_class && i + 2 < len && chars[i + 2] == '<' => {
                    let start = i + 3;
                    if let Some(end) = chars[start..].iter().position(|&c| c == '>') {
                        let name: String = chars[start..start + end].iter().collect();
                        if duplicated_names.contains(&name) {
                            // For duplicate named groups, emit alternation of all variants
                            // plus empty-match fallback (when no variant captured)
                            if let Some(variants) = dup_group_map.get(&name) {
                                let parts: Vec<String> = variants
                                    .iter()
                                    .map(|(iname, _)| format!("(?P={})", iname))
                                    .collect();
                                // (?=) is zero-width always-succeed for when neither captured
                                result.push_str(&format!("(?:{}|(?=))", parts.join("|")));
                            } else {
                                // Haven't seen the groups yet (forward ref) — use deferred
                                result.push_str(&format!("(?P={})", name));
                            }
                        } else {
                            result.push_str(&format!("(?P={})", name));
                        }
                        i = start + end + 1;
                        continue;
                    }
                    result.push_str("\\k");
                    i += 2;
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
                            push_literal_char(&mut result, ch, in_char_class);
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
                // \xHH → hex escape
                'x' if i + 3 < len
                    && chars[i + 2].is_ascii_hexdigit()
                    && chars[i + 3].is_ascii_hexdigit() =>
                {
                    let hex: String = chars[i + 2..i + 4].iter().collect();
                    if let Ok(cp) = u32::from_str_radix(&hex, 16)
                        && let Some(ch) = char::from_u32(cp)
                    {
                        push_literal_char(&mut result, ch, in_char_class);
                    }
                    i += 4;
                }
                // \uHHHH or \u{HHHH+}
                'u' => {
                    if i + 2 < len && chars[i + 2] == '{' {
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
                                    push_literal_char(&mut result, ch, in_char_class);
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
                            if is_surrogate(cp) {
                                push_literal_char(&mut result, surrogate_to_pua(cp), in_char_class);
                            } else if let Some(ch) = char::from_u32(cp) {
                                push_literal_char(&mut result, ch, in_char_class);
                            }
                        }
                        i += 6;
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
                            result.push_str("\\S");
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
                        if in_char_class {
                            result.push_str("A-Za-z0-9_");
                        } else {
                            result.push_str("[A-Za-z0-9_]");
                        }
                    } else if next == 'W' {
                        if in_char_class {
                            result.push_str("\\x{00}-\\x{2F}\\x{3A}-\\x{40}\\x{5B}-\\x{5E}\\x{60}\\x{7B}-\\x{10FFFF}");
                        } else {
                            result.push_str("[^A-Za-z0-9_]");
                        }
                    } else {
                        result.push('\\');
                        result.push(next);
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
                    if ref_num <= total_groups && ref_num > groups_seen {
                        // Forward reference: group exists but not yet captured, matches empty string
                        result.push_str("(?:)");
                        i = ref_end;
                    } else if ref_num <= total_groups {
                        // Normal backreference to already-seen group
                        result.push('\\');
                        for &ch in &chars[ref_start..ref_end] {
                            result.push(ch);
                        }
                        i = ref_end;
                    } else if !flags.contains('u') && !flags.contains('v') {
                        // Annex B: octal escape (non-Unicode mode)
                        // Parse up to 3 octal digits
                        let mut octal_end = i + 1;
                        let mut octal_count = 0;
                        while octal_end < len
                            && octal_count < 3
                            && chars[octal_end] >= '0'
                            && chars[octal_end] <= '7'
                        {
                            octal_end += 1;
                            octal_count += 1;
                        }
                        if octal_count > 0 {
                            let octal_str: String = chars[i + 1..octal_end].iter().collect();
                            if let Ok(val) = u32::from_str_radix(&octal_str, 8) {
                                if val <= 0xFF {
                                    if let Some(ch) = char::from_u32(val) {
                                        push_literal_char(&mut result, ch, in_char_class);
                                        i = octal_end;
                                    } else {
                                        result.push('\\');
                                        result.push(next);
                                        i += 2;
                                    }
                                } else {
                                    // Value too large, just match first digit as octal
                                    let single_val = (next as u32) - ('0' as u32);
                                    if let Some(ch) = char::from_u32(single_val) {
                                        push_literal_char(&mut result, ch, in_char_class);
                                    }
                                    i += 2;
                                }
                            } else {
                                result.push('\\');
                                result.push(next);
                                i += 2;
                            }
                        } else {
                            result.push('\\');
                            result.push(next);
                            i += 2;
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
                        if flags.contains('v') && !in_char_class {
                            if let Some((singles, multi_strs)) =
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
                        }
                        if let Some(ranges) = crate::unicode_tables::lookup_property(&content) {
                            expand_property_to_char_class(
                                &mut result,
                                ranges,
                                negated,
                                in_char_class,
                            );
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
                dotall_stack.push(None);
                result.push_str("(?<");
                result.push(chars[i + 3]);
                i += 4;
            } else {
                // Named group (capturing)
                groups_seen += 1;
                dotall_stack.push(None);
                // Extract the group name to check if it's duplicated
                let name_start = i + 3;
                let mut k = name_start;
                while k < len && chars[k] != '>' {
                    k += 1;
                }
                let name: String = chars[name_start..k].iter().collect();
                if duplicated_names.contains(&name) {
                    let seq = dup_seen_count.entry(name.clone()).or_insert(0);
                    *seq += 1;
                    let internal_name = format!("{}__{}", name, seq);
                    dup_group_map
                        .entry(name)
                        .or_default()
                        .push((internal_name.clone(), groups_seen));
                    result.push_str(&format!("(?P<{}>", internal_name));
                    i = k + 1; // skip past name and '>'
                } else {
                    result.push_str("(?P<");
                    i += 3;
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

        // Close group: pop dotall state if needed
        if c == ')' && !in_char_class {
            if let Some(Some(prev)) = dotall_stack.pop() {
                dot_all = prev;
            }
            result.push(')');
            i += 1;
            continue;
        }

        // Handle '(' for other group types (non-capturing (?:), lookahead (?=), (?!), plain)
        if c == '(' && !in_char_class {
            dotall_stack.push(None);
            if i + 1 >= len || chars[i + 1] != '?' {
                groups_seen += 1;
            }
        }

        // Dot handling: expand based on dotAll state
        if c == '.' && !in_char_class {
            if dot_all {
                result.push_str("(?s:.)");
            } else {
                result.push_str("[^\\n\\r\\u{2028}\\u{2029}]");
            }
            i += 1;
            continue;
        }

        result.push(c);
        i += 1;
    }

    Ok(TranslationResult {
        pattern: result,
        dup_group_map,
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
        // Inside a [...], just insert ranges inline
        for &(lo, hi) in ranges_to_use {
            append_unicode_range(result, lo, hi);
        }
    } else {
        // Outside a char class, wrap in [...]
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
                'd' | 'D' | 'w' | 'W' | 's' | 'S' | 'b' | 'B' | 'p' | 'P' => None,
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
    let mut has_dash = false;
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
        has_dash = true;
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

    // If has dash but both sections can't be empty (already checked above for no dash case)
    if has_dash && add_flags.is_empty() && remove_flags.is_empty() {
        return Err(format!(
            "Invalid regular expression: /{}/ : Invalid flags in modifier group",
            source
        ));
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

pub(crate) fn validate_js_pattern(source: &str, _flags: &str) -> Result<(), String> {
    let _unicode = _flags.contains('u') || _flags.contains('v');
    let v_flag = _flags.contains('v');
    let chars: Vec<char> = source.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut has_atom = false;

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
                        match parse_regexp_group_name(&chars, i, source, _unicode) {
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

            if after_escape == 'x' && i < len && chars[i].is_ascii_hexdigit() {
                i += 1;
                if i < len && chars[i].is_ascii_hexdigit() {
                    i += 1;
                }
            } else if after_escape == 'u' {
                if i < len && chars[i] == '{' {
                    i += 1;
                    while i < len && chars[i] != '}' {
                        i += 1;
                    }
                    if i < len {
                        i += 1;
                    }
                } else {
                    let mut count = 0;
                    while count < 4 && i < len && chars[i].is_ascii_hexdigit() {
                        i += 1;
                        count += 1;
                    }
                }
            } else if after_escape == 'c' && i < len && chars[i].is_ascii_alphabetic() {
                i += 1;
            } else if (after_escape == 'p' || after_escape == 'P') && i < len && chars[i] == '{' {
                let start = i + 1;
                let mut end = start;
                while end < len && chars[end] != '}' {
                    end += 1;
                }
                if end < len {
                    if _unicode {
                        let content: String = chars[start..end].iter().collect();
                        validate_unicode_property_escape(&content).map_err(|_| {
                            format!(
                                "Invalid regular expression: /{}/ : Invalid property name",
                                source
                            )
                        })?;
                    }
                    i = end + 1;
                } else {
                    i = end;
                }
            } else if _unicode {
                // In unicode mode, only specific escape sequences are valid
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
                        | 'c'
                        | 'x'
                        | 'u'
                        | 'p'
                        | 'P'
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
                        | '1'..='9'
                );
                if !valid {
                    return Err(format!(
                        "Invalid regular expression: /{}/ : Invalid escape",
                        source
                    ));
                }
            }
            continue;
        }

        if c == '[' {
            i += 1;
            if i < len && chars[i] == '^' {
                i += 1;
            }

            if v_flag {
                let mut depth = 1;
                while i < len && depth > 0 {
                    match chars[i] {
                        '[' => depth += 1,
                        ']' => depth -= 1,
                        '\\' if i + 1 < len => {
                            i += 1;
                        }
                        _ => {}
                    }
                    i += 1;
                }
            } else {
                let mut prev_value: Option<u32> = None;
                let mut expecting_range_end = false;

                while i < len && chars[i] != ']' {
                    if chars[i] == '-' && !expecting_range_end {
                        if prev_value.is_some() && i + 1 < len && chars[i + 1] != ']' {
                            expecting_range_end = true;
                            i += 1;
                            continue;
                        }
                        prev_value = Some('-' as u32);
                        i += 1;
                        continue;
                    }

                    let save_i = i;
                    let val = resolve_class_escape(&chars, &mut i);

                    if expecting_range_end {
                        expecting_range_end = false;
                        if let (Some(start_val), Some(end_val)) = (prev_value, val)
                            && start_val > end_val
                        {
                            return Err(format!(
                                "Invalid regular expression: /{}/ : Range out of order in character class",
                                source
                            ));
                        }
                        prev_value = val;
                        continue;
                    }

                    prev_value = val;
                    if i == save_i {
                        i += 1;
                    }
                }

                if i < len {
                    i += 1; // skip ']'
                }
            }
            has_atom = true;
            continue;
        }

        if c == '(' {
            i += 1;
            if i < len && chars[i] == '?' {
                i += 1;
                if i < len {
                    match chars[i] {
                        ':' | '=' | '!' => {
                            i += 1;
                        }
                        '<' if i + 1 < len && (chars[i + 1] == '=' || chars[i + 1] == '!') => {
                            i += 2;
                        }
                        '<' if i + 1 < len && chars[i + 1] != '=' && chars[i + 1] != '!' => {
                            i += 1; // skip '<'
                            let (name, new_i) =
                                parse_regexp_group_name(&chars, i, source, _unicode)?;
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
            group_depth += 1;
            while alt_ids.len() <= group_depth {
                alt_ids.push(0);
            }
            has_atom = false;
            continue;
        }

        if c == ')' {
            i += 1;
            group_depth = group_depth.saturating_sub(1);
            has_atom = true;
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
                    // Not a quantifier, just a literal '{'
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
    if has_bare_k_escape && (_unicode || has_any_named_group) {
        return Err(format!(
            "Invalid regular expression: /{}/ : Invalid escape",
            source
        ));
    }

    // \k<name without closing > is an error if pattern has named groups or in unicode mode
    if has_incomplete_backref && (_unicode || has_any_named_group) {
        return Err(format!(
            "Invalid regular expression: /{}/ : Invalid named reference",
            source
        ));
    }

    // Check for dangling \k<name> backreferences
    if !backref_names.is_empty() {
        let defined_names: std::collections::HashSet<&str> =
            named_groups.iter().map(|(n, _, _)| n.as_str()).collect();
        for bref in &backref_names {
            if !defined_names.contains(bref.as_str()) && (_unicode || has_any_named_group) {
                return Err(format!(
                    "Invalid regular expression: /{}/ : Invalid named reference",
                    source
                ));
            }
        }
    }

    // Validate property escapes by running translate_js_pattern
    if _unicode {
        translate_js_pattern(source, _flags)?;
    }

    Ok(())
}

#[allow(dead_code)]
fn build_fancy_regex(source: &str, flags: &str) -> Result<fancy_regex::Regex, String> {
    let pattern = translate_js_pattern(source, flags)?;
    fancy_regex::Regex::new(&pattern).map_err(|e| e.to_string())
}

enum CompiledRegex {
    Fancy(fancy_regex::Regex),
    Standard(regex::Regex),
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

type DupGroupMap = std::collections::HashMap<String, Vec<(String, u32)>>;

fn build_regex(source: &str, flags: &str) -> Result<CompiledRegex, String> {
    build_regex_ex(source, flags).map(|(re, _)| re)
}

fn build_regex_ex(source: &str, flags: &str) -> Result<(CompiledRegex, DupGroupMap), String> {
    let tr = translate_js_pattern_ex(source, flags)?;
    let dup_map = tr.dup_group_map;
    match fancy_regex::Regex::new(&tr.pattern) {
        Ok(r) => Ok((CompiledRegex::Fancy(r), dup_map)),
        Err(_) => regex::Regex::new(&tr.pattern)
            .map(|r| (CompiledRegex::Standard(r), dup_map))
            .map_err(|e| e.to_string()),
    }
}

fn regex_captures(re: &CompiledRegex, text: &str) -> Option<RegexCaptures> {
    regex_captures_at(re, text, 0)
}

fn regex_captures_at(re: &CompiledRegex, text: &str, pos: usize) -> Option<RegexCaptures> {
    match re {
        CompiledRegex::Fancy(r) => {
            let caps = r.captures_from_pos(text, pos).ok()??;
            let names: Vec<Option<String>> = r
                .capture_names()
                .map(|n| n.map(|s| s.to_string()))
                .collect();
            let mut groups = Vec::new();
            for i in 0..caps.len() {
                groups.push(caps.get(i).map(|m| RegexMatch {
                    start: m.start(),
                    end: m.end(),
                    text: m.as_str().to_string(),
                }));
            }
            Some(RegexCaptures { groups, names })
        }
        CompiledRegex::Standard(r) => {
            let caps = if pos == 0 {
                r.captures(text)?
            } else {
                r.captures_at(text, pos)?
            };
            let names: Vec<Option<String>> = r
                .capture_names()
                .map(|n| n.map(|s| s.to_string()))
                .collect();
            let mut groups = Vec::new();
            for i in 0..caps.len() {
                groups.push(caps.get(i).map(|m| RegexMatch {
                    start: m.start(),
                    end: m.end(),
                    text: m.as_str().to_string(),
                }));
            }
            Some(RegexCaptures { groups, names })
        }
    }
}

#[allow(dead_code)]
fn regex_is_match(re: &CompiledRegex, text: &str) -> bool {
    match re {
        CompiledRegex::Fancy(r) => r.is_match(text).unwrap_or(false),
        CompiledRegex::Standard(r) => r.is_match(text),
    }
}

#[allow(dead_code)]
fn build_rust_regex(source: &str, flags: &str) -> Result<regex::Regex, String> {
    let mut pattern = String::new();
    if flags.contains('i') {
        pattern.push_str("(?i)");
    }
    if flags.contains('s') {
        pattern.push_str("(?s)");
    }
    if flags.contains('m') {
        pattern.push_str("(?m)");
    }
    pattern.push_str(source);
    regex::Regex::new(&pattern).map_err(|e| e.to_string())
}

fn extract_source_flags(interp: &Interpreter, this_val: &JsValue) -> Option<(String, String, u64)> {
    if let JsValue::Object(o) = this_val
        && let Some(obj) = interp.get_object(o.id)
    {
        let b = obj.borrow();
        let source = if let JsValue::String(s) = b.get_property("__original_source__") {
            s.to_rust_string()
        } else {
            return None;
        };
        let flags = if let JsValue::String(s) = b.get_property("__original_flags__") {
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

#[allow(dead_code)]
fn get_last_index(interp: &Interpreter, obj_id: u64) -> f64 {
    if let Some(obj) = interp.get_object(obj_id) {
        to_number(&obj.borrow().get_property("lastIndex"))
    } else {
        0.0
    }
}

#[allow(dead_code)]
fn set_last_index(interp: &Interpreter, obj_id: u64, val: f64) {
    if let Some(obj) = interp.get_object(obj_id) {
        obj.borrow_mut()
            .insert_builtin("lastIndex".to_string(), JsValue::Number(val));
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
        let desc = obj.borrow().get_property_descriptor(key);
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

fn regexp_exec_abstract(interp: &mut Interpreter, rx_id: u64, s: &str) -> Completion {
    let rx_val = JsValue::Object(crate::types::JsObject { id: rx_id });
    let exec_val = match interp.get_object_property(rx_id, "exec", &rx_val) {
        Completion::Normal(v) => v,
        other => return other,
    };
    if interp.is_callable(&exec_val) {
        let result = interp.call_function(
            &exec_val,
            &rx_val,
            &[JsValue::String(regex_output_to_js_string(s))],
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
            let src = if let JsValue::String(s) = b.get_property("__original_source__") {
                s.to_rust_string()
            } else {
                String::new()
            };
            let fl = if let JsValue::String(s) = b.get_property("__original_flags__") {
                s.to_rust_string()
            } else {
                String::new()
            };
            drop(b);
            (src, fl)
        } else {
            return Completion::Normal(JsValue::Null);
        };
        regexp_exec_raw(interp, rx_id, &source, &flags, s)
    }
}

/// AdvanceStringIndex per spec. `index` is in UTF-16 code units.
fn advance_string_index(s: &str, index: usize, unicode: bool) -> usize {
    if !unicode {
        return index + 1;
    }
    let utf16_len: usize = s.chars().map(|c| c.len_utf16()).sum();
    if index + 1 >= utf16_len {
        return index + 1;
    }
    // Find the character at the given UTF-16 index
    let byte_offset = utf16_to_byte_offset(s, index);
    if byte_offset >= s.len() {
        return index + 1;
    }
    let c = s[byte_offset..].chars().next().unwrap_or('\0');
    // If the code point takes 2 UTF-16 code units (surrogate pair), advance by 2
    if c.len_utf16() == 2 {
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
    captures: &[JsValue],
    named_captures: &JsValue,
    replacement: &str,
) -> Result<String, JsValue> {
    let tail_pos = position + matched.len();
    let mut result = String::new();
    let bytes = replacement.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    let m = captures.len();
    while i < len {
        if bytes[i] == b'$' && i + 1 < len {
            match bytes[i + 1] {
                b'$' => {
                    result.push('$');
                    i += 2;
                }
                b'&' => {
                    result.push_str(matched);
                    i += 2;
                }
                b'`' => {
                    if position > 0 && position <= s.len() {
                        result.push_str(&s[..position]);
                    }
                    i += 2;
                }
                b'\'' => {
                    if tail_pos < s.len() {
                        result.push_str(&s[tail_pos..]);
                    }
                    i += 2;
                }
                c if c.is_ascii_digit() => {
                    let d1 = (c - b'0') as usize;
                    // Try two-digit reference first
                    if i + 2 < len && bytes[i + 2].is_ascii_digit() {
                        let d2 = (bytes[i + 2] - b'0') as usize;
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
                    // Single-digit reference
                    if d1 >= 1 && d1 <= m {
                        let cap = &captures[d1 - 1];
                        if !cap.is_undefined() {
                            let cap_s = interp.to_string_value(cap)?;
                            result.push_str(&cap_s);
                        }
                    } else {
                        result.push('$');
                        result.push(c as char);
                    }
                    i += 2;
                }
                b'<' => {
                    if matches!(named_captures, JsValue::Undefined) {
                        result.push('$');
                        result.push('<');
                        i += 2;
                    } else {
                        let start = i + 2;
                        if let Some(end_pos) = replacement[start..].find('>') {
                            let group_name = &replacement[start..start + end_pos];
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
                                group_name,
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
                            i = start + end_pos + 1;
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
            result.push(bytes[i] as char);
            i += 1;
        }
    }
    Ok(result)
}

fn regexp_exec_raw(
    interp: &mut Interpreter,
    this_id: u64,
    source: &str,
    flags: &str,
    input: &str,
) -> Completion {
    let global = flags.contains('g');
    let sticky = flags.contains('y');
    let has_indices = flags.contains('d');

    // Spec: Let lastIndex be ? ToLength(? Get(R, "lastIndex")).
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

    // lastIndex is in UTF-16 code units; convert to byte offset for string slicing
    let input_utf16_len: usize = input.chars().map(|c| c.len_utf16()).sum();
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
    let last_index_byte = utf16_to_byte_offset(input, last_index_utf16);

    let (re, dup_map) = match build_regex_ex(source, flags) {
        Ok(r) => r,
        Err(_) => return Completion::Normal(JsValue::Null),
    };

    let caps = match regex_captures_at(&re, input, last_index_byte) {
        Some(c) => c,
        None => {
            if (global || sticky)
                && let Err(e) = set_last_index_strict(interp, this_id, 0.0)
            {
                return Completion::Throw(e);
            }
            return Completion::Normal(JsValue::Null);
        }
    };

    let full_match = caps.get(0).unwrap();
    // Convert absolute byte offsets to UTF-16 code unit offsets
    let match_start_utf16 = byte_offset_to_utf16(input, full_match.start);
    let match_end_utf16 = byte_offset_to_utf16(input, full_match.end);

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

    let mut elements: Vec<JsValue> = Vec::new();
    elements.push(JsValue::String(regex_output_to_js_string(&full_match.text)));
    for i in 1..caps.len() {
        match caps.get(i) {
            Some(m) => elements.push(JsValue::String(regex_output_to_js_string(&m.text))),
            None => elements.push(JsValue::Undefined),
        }
    }

    let has_named = caps.names.iter().any(|n| n.is_some());
    let groups_val = if has_named {
        let groups_obj = interp.create_object();
        groups_obj.borrow_mut().prototype = None;
        if dup_map.is_empty() {
            for (i, name_opt) in caps.names.iter().enumerate() {
                if let Some(name) = name_opt {
                    let val = match caps.get(i) {
                        Some(m) => JsValue::String(regex_output_to_js_string(&m.text)),
                        None => JsValue::Undefined,
                    };
                    groups_obj.borrow_mut().insert_value(name.to_string(), val);
                }
            }
        } else {
            // Build a set of internal names that belong to duplicate groups
            let mut internal_to_original: std::collections::HashMap<String, String> =
                std::collections::HashMap::new();
            for (orig_name, variants) in &dup_map {
                for (internal_name, _) in variants {
                    internal_to_original.insert(internal_name.clone(), orig_name.clone());
                }
            }
            // Track which original dup names we've already set
            let mut dup_names_set: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            // First pass: handle non-duplicate named groups normally
            for (i, name_opt) in caps.names.iter().enumerate() {
                if let Some(name) = name_opt {
                    if internal_to_original.contains_key(name) {
                        // This is a duplicate group variant — skip for now
                        continue;
                    }
                    let val = match caps.get(i) {
                        Some(m) => JsValue::String(regex_output_to_js_string(&m.text)),
                        None => JsValue::Undefined,
                    };
                    groups_obj.borrow_mut().insert_value(name.to_string(), val);
                }
            }
            // Second pass: for each duplicate group name, find which variant matched
            for (orig_name, variants) in &dup_map {
                if dup_names_set.contains(orig_name) {
                    continue;
                }
                dup_names_set.insert(orig_name.clone());
                let mut matched_val = JsValue::Undefined;
                for (internal_name, _) in variants {
                    // Find the index of this internal name in caps.names
                    for (i, name_opt) in caps.names.iter().enumerate() {
                        if let Some(n) = name_opt
                            && n == internal_name
                            && let Some(m) = caps.get(i)
                        {
                            matched_val = JsValue::String(regex_output_to_js_string(&m.text));
                            break;
                        }
                    }
                    if !matches!(matched_val, JsValue::Undefined) {
                        break;
                    }
                }
                groups_obj
                    .borrow_mut()
                    .insert_value(orig_name.to_string(), matched_val);
            }
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
            let mut index_pairs: Vec<JsValue> = Vec::new();
            for i in 0..caps.len() {
                match caps.get(i) {
                    Some(m) => {
                        let cap_start = byte_offset_to_utf16(input, m.start);
                        let cap_end = byte_offset_to_utf16(input, m.end);
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
            if has_named {
                let idx_groups = interp.create_object();
                idx_groups.borrow_mut().prototype = None;
                for (i, name_opt) in caps.names.iter().enumerate() {
                    if let Some(name) = name_opt {
                        let val = match caps.get(i) {
                            Some(m) => {
                                let cap_start = byte_offset_to_utf16(input, m.start);
                                let cap_end = byte_offset_to_utf16(input, m.end);
                                interp.create_array(vec![
                                    JsValue::Number(cap_start as f64),
                                    JsValue::Number(cap_end as f64),
                                ])
                            }
                            None => JsValue::Undefined,
                        };
                        idx_groups.borrow_mut().insert_value(name.to_string(), val);
                    }
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
    interp.global_env.borrow().get("Symbol").and_then(|sv| {
        if let JsValue::Object(so) = sv {
            interp.get_object(so.id).map(|sobj| {
                let val = sobj.borrow().get_property(name);
                to_js_string(&val)
            })
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
                let input = match to_regex_input(interp, &arg) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                if let Some((source, flags, obj_id)) = extract_source_flags(interp, this_val) {
                    return regexp_exec_raw(interp, obj_id, &source, &flags, &input);
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
                let input = match to_regex_input(interp, &arg) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                let result = regexp_exec_abstract(interp, obj_id, &input);
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
                let s = match to_regex_input(interp, &arg) {
                    Ok(s) => s,
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
                    return regexp_exec_abstract(interp, rx_id, &s);
                }

                // 6. Let fullUnicode be flags contains "u" or "v".
                let full_unicode = flags_str.contains('u') || flags_str.contains('v');

                // 6b. Perform ? Set(rx, "lastIndex", +0𝔽, true).
                if let Err(e) = set_last_index_strict(interp, rx_id, 0.0) {
                    return Completion::Throw(e);
                }

                let mut results: Vec<JsValue> = Vec::new();
                loop {
                    let result = regexp_exec_abstract(interp, rx_id, &s);
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
                            let match_str = match interp.to_string_value(&matched_val) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            };
                            results.push(JsValue::String(JsString::from_str(&match_str)));
                            if match_str.is_empty() {
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
                let s = match to_regex_input(interp, &arg) {
                    Ok(s) => s,
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
                let result = regexp_exec_abstract(interp, rx_id, &s);
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
                let s = match to_regex_input(interp, &string_arg) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                let length_s = s.len();

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
                loop {
                    // 11a. Let result be ? RegExpExec(rx, S).
                    let result = regexp_exec_abstract(interp, rx_id, &s);
                    match result {
                        Completion::Normal(JsValue::Null) => break,
                        Completion::Normal(ref result_val)
                            if matches!(result_val, JsValue::Object(_)) =>
                        {
                            let result_obj = result_val.clone();
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
                                    other => return other,
                                };
                            let match_str = match interp.to_string_value(&matched_val) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            };
                            if match_str.is_empty() {
                                // a. Let thisIndex be ? ToLength(? Get(rx, "lastIndex")).
                                let rx_val = JsValue::Object(crate::types::JsObject { id: rx_id });
                                let li_val =
                                    match interp.get_object_property(rx_id, "lastIndex", &rx_val) {
                                        Completion::Normal(v) => v,
                                        other => return other,
                                    };
                                let li_num = match interp.to_number_value(&li_val) {
                                    Ok(n) => n,
                                    Err(e) => return Completion::Throw(e),
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
                                    Err(e) => return Completion::Throw(e),
                                }
                            }
                        }
                        Completion::Normal(_) => break,
                        other => return other,
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
                        other => return other,
                    };
                    let n_captures = {
                        let n = match interp.to_number_value(&len_val) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        };
                        let len = if n.is_nan() || n <= 0.0 {
                            0.0
                        } else {
                            n.min(9007199254740991.0).floor()
                        };
                        (len as usize).max(1) // at least 1
                    };
                    // nCaptures = max(nCaptures - 1, 0) — number of capture groups
                    let n_cap = if n_captures > 0 { n_captures - 1 } else { 0 };

                    // d. Let matched be ? ToString(? Get(result, "0")).
                    let matched_val = match interp.get_object_property(result_id, "0", result_val) {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    let matched = match interp.to_string_value(&matched_val) {
                        Ok(s) => s,
                        Err(e) => return Completion::Throw(e),
                    };
                    // Get match_length in byte offsets within the PUA-mapped string
                    let matched_pua = match to_regex_input(interp, &matched_val) {
                        Ok(s) => s,
                        Err(e) => return Completion::Throw(e),
                    };
                    let match_length = matched_pua.len();

                    // e. Let position be ? ToIntegerOrInfinity(? Get(result, "index")).
                    let index_val = match interp.get_object_property(result_id, "index", result_val)
                    {
                        Completion::Normal(v) => v,
                        other => return other,
                    };
                    // Convert position from UTF-16 code units to byte offset in PUA-mapped string
                    let position = {
                        let n = match interp.to_number_value(&index_val) {
                            Ok(n) => n,
                            Err(e) => return Completion::Throw(e),
                        };
                        let int = to_integer_or_infinity(n);
                        let utf16_pos = int.max(0.0) as usize;
                        utf16_to_byte_offset(&s, utf16_pos).min(length_s)
                    };

                    // g-i. Get captures
                    let mut captures: Vec<JsValue> = Vec::new();
                    for n in 1..=n_cap {
                        let cap_n =
                            match interp.get_object_property(result_id, &n.to_string(), result_val)
                            {
                                Completion::Normal(v) => v,
                                other => return other,
                            };
                        if !cap_n.is_undefined() {
                            let cap_str = match interp.to_string_value(&cap_n) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
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
                            other => return other,
                        };

                    let replacement = if functional_replace {
                        // k. If functionalReplace is true, then
                        let mut replacer_args: Vec<JsValue> = Vec::new();
                        replacer_args.push(JsValue::String(JsString::from_str(&matched)));
                        for cap in &captures {
                            replacer_args.push(cap.clone());
                        }
                        replacer_args.push(JsValue::Number(position as f64));
                        replacer_args.push(JsValue::String(JsString::from_str(&s)));
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
                                Err(e) => return Completion::Throw(e),
                            },
                            other => return other,
                        }
                    } else {
                        // l. Else (string replace)
                        let template = replace_str.as_ref().unwrap();
                        let named_captures_obj = if !named_captures.is_undefined() {
                            // i. Set namedCaptures to ? ToObject(namedCaptures).
                            match interp.to_object(&named_captures) {
                                Completion::Normal(v) => v,
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => JsValue::Undefined,
                            }
                        } else {
                            JsValue::Undefined
                        };
                        match get_substitution(
                            interp,
                            &matched,
                            &s,
                            position,
                            &captures,
                            &named_captures_obj,
                            template,
                        ) {
                            Ok(s) => s,
                            Err(e) => return Completion::Throw(e),
                        }
                    };

                    // p. If position >= nextSourcePosition, then
                    if position >= next_source_position {
                        accumulated_result.push_str(&s[next_source_position..position]);
                        accumulated_result.push_str(&replacement);
                        next_source_position = position + match_length;
                    }
                }

                // 15. Return accumulatedResult + remainder of S.
                if next_source_position < length_s {
                    accumulated_result.push_str(&s[next_source_position..]);
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
                let s = match to_regex_input(interp, &arg) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };

                // 3. Let C be ? SpeciesConstructor(rx, %RegExp%).
                let rx_val = JsValue::Object(crate::types::JsObject { id: rx_id });
                let regexp_ctor = interp
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

                let size = s.len();

                // 12. If size = 0, then
                if size == 0 {
                    // a. Let z be ? RegExpExec(splitter, S).
                    let z = regexp_exec_abstract(interp, splitter_id, &s);
                    match z {
                        Completion::Normal(ref v) if matches!(v, JsValue::Null) => {
                            a.push(JsValue::String(JsString::from_str(&s)));
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
                    let z = regexp_exec_abstract(interp, splitter_id, &s);
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
                    // Push substring from p to q
                    let t = &s[p..q];
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
                let t = &s[p..size];
                a.push(JsValue::String(JsString::from_str(t)));
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
                let s = match to_regex_input(interp, &arg) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };

                // 3. Let C be ? SpeciesConstructor(R, %RegExp%).
                let rx_val = JsValue::Object(crate::types::JsObject { id: rx_id });
                let regexp_ctor = interp
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

                // Create iterator
                let iter_obj = interp.create_object();
                iter_obj.borrow_mut().class_name = "RegExp String Iterator".to_string();
                if let Some(ref ip) = interp.iterator_prototype {
                    iter_obj.borrow_mut().prototype = Some(ip.clone());
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

                let next_fn = interp.create_function(JsFunction::native(
                    "next".to_string(),
                    0,
                    |interp, this_val, _args| {
                        if let JsValue::Object(o) = this_val
                            && let Some(obj) = interp.get_object(o.id)
                        {
                            let state = obj.borrow().iterator_state.clone();
                            let matcher_id_val = obj.borrow().get_property("__matcher__");
                            let full_unicode = obj.borrow().get_property("__full_unicode__");
                            let full_unicode = matches!(full_unicode, JsValue::Boolean(true));

                            if let Some(IteratorState::RegExpStringIterator {
                                ref source,
                                ref flags,
                                ref string,
                                global,
                                last_index,
                                done,
                            }) = state
                            {
                                if done {
                                    return Completion::Normal(
                                        interp.create_iter_result_object(JsValue::Undefined, true),
                                    );
                                }

                                // If we have a matcher object, use RegExpExec
                                if let JsValue::Number(mid) = matcher_id_val {
                                    let mid = mid as u64;
                                    let result = regexp_exec_abstract(interp, mid, string);
                                    let result_val = match result {
                                        Completion::Normal(v) => v,
                                        other => return other,
                                    };

                                    if matches!(result_val, JsValue::Null) {
                                        if let Some(obj2) = interp.get_object(o.id) {
                                            obj2.borrow_mut().iterator_state =
                                                Some(IteratorState::RegExpStringIterator {
                                                    source: source.clone(),
                                                    flags: flags.clone(),
                                                    string: string.clone(),
                                                    global,
                                                    last_index,
                                                    done: true,
                                                });
                                        }
                                        return Completion::Normal(
                                            interp.create_iter_result_object(
                                                JsValue::Undefined,
                                                true,
                                            ),
                                        );
                                    }

                                    if !global {
                                        if let Some(obj2) = interp.get_object(o.id) {
                                            obj2.borrow_mut().iterator_state =
                                                Some(IteratorState::RegExpStringIterator {
                                                    source: source.clone(),
                                                    flags: flags.clone(),
                                                    string: string.clone(),
                                                    global,
                                                    last_index,
                                                    done: true,
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
                                                    source: source.clone(),
                                                    flags: flags.clone(),
                                                    string: string.clone(),
                                                    global,
                                                    last_index,
                                                    done: true,
                                                });
                                        }
                                        return Completion::Normal(
                                            interp.create_iter_result_object(result_val, false),
                                        );
                                    };
                                    let match_str_val = match interp.get_object_property(
                                        result_id,
                                        "0",
                                        &result_val,
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
                                            mid,
                                            "lastIndex",
                                            &matcher_val2,
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
                                            advance_string_index(string, this_index, full_unicode);
                                        if let Err(e) = spec_set(
                                            interp,
                                            mid,
                                            "lastIndex",
                                            JsValue::Number(next_index as f64),
                                            true,
                                        ) {
                                            return Completion::Throw(e);
                                        }
                                    }

                                    // Update iterator state (last_index not used when matcher-based)
                                    if let Some(obj2) = interp.get_object(o.id) {
                                        obj2.borrow_mut().iterator_state =
                                            Some(IteratorState::RegExpStringIterator {
                                                source: source.clone(),
                                                flags: flags.clone(),
                                                string: string.clone(),
                                                global,
                                                last_index, // not used, matcher has its own lastIndex
                                                done: false,
                                            });
                                    }
                                    return Completion::Normal(
                                        interp.create_iter_result_object(result_val, false),
                                    );
                                }

                                // Fallback: use raw regex (legacy path)
                                let re = match build_regex(source, flags) {
                                    Ok(r) => r,
                                    Err(_) => {
                                        obj.borrow_mut().iterator_state =
                                            Some(IteratorState::RegExpStringIterator {
                                                source: source.clone(),
                                                flags: flags.clone(),
                                                string: string.clone(),
                                                global,
                                                last_index,
                                                done: true,
                                            });
                                        return Completion::Normal(
                                            interp.create_iter_result_object(
                                                JsValue::Undefined,
                                                true,
                                            ),
                                        );
                                    }
                                };

                                if last_index > string.len() {
                                    obj.borrow_mut().iterator_state =
                                        Some(IteratorState::RegExpStringIterator {
                                            source: source.clone(),
                                            flags: flags.clone(),
                                            string: string.clone(),
                                            global,
                                            last_index,
                                            done: true,
                                        });
                                    return Completion::Normal(
                                        interp.create_iter_result_object(JsValue::Undefined, true),
                                    );
                                }

                                match regex_captures(&re, &string[last_index..]) {
                                    None => {
                                        obj.borrow_mut().iterator_state =
                                            Some(IteratorState::RegExpStringIterator {
                                                source: source.clone(),
                                                flags: flags.clone(),
                                                string: string.clone(),
                                                global,
                                                last_index,
                                                done: true,
                                            });
                                        Completion::Normal(
                                            interp.create_iter_result_object(
                                                JsValue::Undefined,
                                                true,
                                            ),
                                        )
                                    }
                                    Some(caps) => {
                                        let full = caps.get(0).unwrap();
                                        let match_start = last_index + full.start;
                                        let match_end = last_index + full.end;

                                        let mut elements: Vec<JsValue> = Vec::new();
                                        elements
                                            .push(JsValue::String(JsString::from_str(&full.text)));
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
                                                JsValue::String(JsString::from_str(string)),
                                            );
                                            robj.borrow_mut().insert_value(
                                                "groups".to_string(),
                                                JsValue::Undefined,
                                            );
                                        }

                                        let new_last_index = if global {
                                            if full.text.is_empty() {
                                                match_end + 1
                                            } else {
                                                match_end
                                            }
                                        } else {
                                            last_index
                                        };

                                        let new_done = !global;

                                        obj.borrow_mut().iterator_state =
                                            Some(IteratorState::RegExpStringIterator {
                                                source: source.clone(),
                                                flags: flags.clone(),
                                                string: string.clone(),
                                                global,
                                                last_index: new_last_index,
                                                done: new_done,
                                            });

                                        Completion::Normal(
                                            interp.create_iter_result_object(result_arr, false),
                                        )
                                    }
                                }
                            } else {
                                Completion::Normal(
                                    interp.create_iter_result_object(JsValue::Undefined, true),
                                )
                            }
                        } else {
                            Completion::Normal(
                                interp.create_iter_result_object(JsValue::Undefined, true),
                            )
                        }
                    },
                ));
                iter_obj
                    .borrow_mut()
                    .insert_builtin("next".to_string(), next_fn);

                // @@iterator returns self
                let iter_self_fn = interp.create_function(JsFunction::native(
                    "[Symbol.iterator]".to_string(),
                    0,
                    |_interp, this, _args| Completion::Normal(this.clone()),
                ));
                if let Some(key) = interp.get_symbol_iterator_key() {
                    iter_obj.borrow_mut().insert_property(
                        key,
                        PropertyDescriptor::data(iter_self_fn, true, false, true),
                    );
                }

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
                    if to_boolean(&val) {
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
        for &(prop_name, flag_char) in flag_props {
            let name = prop_name.to_string();
            let getter = self.create_function(JsFunction::native(
                format!("get {}", name),
                0,
                move |interp, this_val, _args| {
                    let obj_ref = match this_val {
                        JsValue::Object(o) => o,
                        _ => {
                            return Completion::Throw(interp.create_type_error(&format!(
                                "RegExp.prototype.{} requires that 'this' be an Object",
                                name
                            )));
                        }
                    };
                    let obj = match interp.get_object(obj_ref.id) {
                        Some(o) => o,
                        None => return Completion::Normal(JsValue::Undefined),
                    };
                    // Check if this has [[OriginalFlags]] (is a RegExp)
                    if obj.borrow().class_name != "RegExp" {
                        // If this is RegExp.prototype itself, return undefined
                        if obj_ref.id == regexp_proto_id {
                            return Completion::Normal(JsValue::Undefined);
                        }
                        return Completion::Throw(interp.create_type_error(&format!(
                            "RegExp.prototype.{} requires that 'this' be a RegExp object",
                            name
                        )));
                    }
                    let flags_val = obj.borrow().get_property("__original_flags__");
                    if let JsValue::String(s) = flags_val {
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
                        let err = interp.create_type_error(
                            "RegExp.prototype.source requires that 'this' be an Object",
                        );
                        return Completion::Throw(err);
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
                    return Completion::Throw(interp.create_type_error(
                        "RegExp.prototype.source requires that 'this' be a RegExp object",
                    ));
                }
                let source_val = obj.borrow().get_property("__original_source__");
                if let JsValue::String(ref s) = source_val {
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

        let regexp_proto_rc = regexp_proto.clone();

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
                            to_boolean(&matcher)
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
                        .global_env
                        .borrow()
                        .get("RegExp")
                        .unwrap_or(JsValue::Undefined);
                    if same_value(&regexp_fn, &ctor) {
                        return Completion::Normal(pattern_arg.clone());
                    }
                }

                // Handle RegExp/RegExp-like argument: extract source/flags
                let (pattern_str, flags_str) =
                    if is_regexp_obj && let JsValue::Object(ref o) = pattern_arg {
                        // Use Get() for observable property access
                        let src = match interp.get_object_property(o.id, "source", &pattern_arg) {
                            Completion::Normal(v) => match interp.to_string_value(&v) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            },
                            Completion::Throw(e) => return Completion::Throw(e),
                            _ => String::new(),
                        };
                        let flg = if matches!(flags_arg, JsValue::Undefined) {
                            match interp.get_object_property(o.id, "flags", &pattern_arg) {
                                Completion::Normal(v) => match interp.to_string_value(&v) {
                                    Ok(s) => s,
                                    Err(e) => return Completion::Throw(e),
                                },
                                Completion::Throw(e) => return Completion::Throw(e),
                                _ => String::new(),
                            }
                        } else {
                            match interp.to_string_value(&flags_arg) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            }
                        };
                        (src, flg)
                    } else {
                        let p = if matches!(pattern_arg, JsValue::Undefined) {
                            String::new()
                        } else {
                            match interp.to_string_value(&pattern_arg) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            }
                        };
                        let f = if matches!(flags_arg, JsValue::Undefined) {
                            String::new()
                        } else {
                            match interp.to_string_value(&flags_arg) {
                                Ok(s) => s,
                                Err(e) => return Completion::Throw(e),
                            }
                        };
                        (p, f)
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
                let mut seen = std::collections::HashSet::new();
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
                let source_str = if pattern_str.is_empty() {
                    "(?:)".to_string()
                } else {
                    pattern_str.clone()
                };

                let mut obj = JsObjectData::new();
                obj.prototype = Some(regexp_proto_rc.clone());
                obj.class_name = "RegExp".to_string();
                // Store internal slots as non-enumerable hidden properties
                obj.insert_property(
                    "__original_source__".to_string(),
                    PropertyDescriptor::data(
                        JsValue::String(JsString::from_str(&source_str)),
                        false,
                        false,
                        false,
                    ),
                );
                obj.insert_property(
                    "__original_flags__".to_string(),
                    PropertyDescriptor::data(
                        JsValue::String(JsString::from_str(&flags_str)),
                        false,
                        false,
                        false,
                    ),
                );
                obj.insert_property(
                    "lastIndex".to_string(),
                    PropertyDescriptor::data(JsValue::Number(0.0), true, false, false),
                );
                let rc = Rc::new(RefCell::new(obj));
                let id = interp.allocate_object_slot(rc);
                Completion::Normal(JsValue::Object(crate::types::JsObject { id }))
            },
        ));
        // RegExp.escape (§22.2.5.1)
        let escape_fn = self.create_function(JsFunction::native(
            "escape".to_string(),
            1,
            |interp, _this, args| {
                let arg = args.first().cloned().unwrap_or(JsValue::Undefined);
                if !matches!(arg, JsValue::String(_)) {
                    let err = interp.create_type_error("RegExp.escape requires a string argument");
                    return Completion::Throw(err);
                }
                let s = match interp.to_string_value(&arg) {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                let mut result = String::new();
                for (i, c) in s.chars().enumerate() {
                    if i == 0 && (c.is_ascii_alphanumeric()) {
                        result.push_str(&format!("\\x{:02x}", c as u32));
                    } else {
                        result.push_str(&encode_for_regexp_escape(c));
                    }
                }
                Completion::Normal(JsValue::String(JsString::from_str(&result)))
            },
        ));

        if let JsValue::Object(ref o) = regexp_ctor
            && let Some(obj) = self.get_object(o.id)
        {
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

            regexp_proto
                .borrow_mut()
                .insert_builtin("constructor".to_string(), regexp_ctor.clone());
        }
        self.global_env
            .borrow_mut()
            .declare("RegExp", BindingKind::Var);
        let _ = self.global_env.borrow_mut().set("RegExp", regexp_ctor);

        self.regexp_prototype = Some(regexp_proto);
    }

    pub(crate) fn get_symbol_key(&self, name: &str) -> Option<String> {
        get_symbol_key(self, name)
    }
}
