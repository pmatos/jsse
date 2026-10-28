//! Base64 and hex codecs behind `Uint8Array.fromBase64`/`fromHex`/`toBase64`/`toHex`/`setFromBase64`/
//! `setFromHex`, split out of `typedarray.rs`.

fn base64_char_value(c: char, alphabet: &str) -> Option<u8> {
    match c {
        'A'..='Z' => Some(c as u8 - b'A'),
        'a'..='z' => Some(c as u8 - b'a' + 26),
        '0'..='9' => Some(c as u8 - b'0' + 52),
        '+' if alphabet == "base64" => Some(62),
        '/' if alphabet == "base64" => Some(63),
        '-' if alphabet == "base64url" => Some(62),
        '_' if alphabet == "base64url" => Some(63),
        _ => None,
    }
}

fn is_base64_whitespace(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\x0C' | '\r' | ' ')
}

// Decode base64 with full spec compliance.
// Returns (decoded_bytes, chars_read_from_original_input)
// max_bytes: if Some, stop decoding when output would exceed this many bytes
// Result of FromBase64/FromHex spec algorithm
pub(super) struct DecodeResult {
    pub(super) bytes: Vec<u8>,
    pub(super) read: usize, // chars read from original input
    pub(super) error: Option<String>,
}

pub(super) fn decode_base64(
    input: &str,
    alphabet: &str,
    last_chunk_handling: &str,
    max_bytes: Option<usize>,
) -> DecodeResult {
    let max = max_bytes.unwrap_or(usize::MAX);
    if max == 0 {
        return DecodeResult {
            bytes: Vec::new(),
            read: 0,
            error: None,
        };
    }

    let chars: Vec<char> = input.chars().collect();

    // Strip whitespace, tracking original positions (byte position in original string)
    let mut cleaned: Vec<(char, usize)> = Vec::new();
    for (i, &c) in chars.iter().enumerate() {
        if is_base64_whitespace(c) {
            continue;
        }
        cleaned.push((c, i));
    }

    let mut output = Vec::new();
    let mut i = 0; // index into cleaned

    while i < cleaned.len() {
        let chunk_start = i;
        let _chunk_start_output_len = output.len();

        // Collect data chars for this chunk
        let mut chunk: Vec<u8> = Vec::new();
        let mut padding_count = 0;
        let mut saw_padding = false;

        while i < cleaned.len() && chunk.len() + padding_count < 4 {
            let (c, _) = cleaned[i];
            if c == '=' {
                saw_padding = true;
                padding_count += 1;
                i += 1;
            } else if saw_padding {
                // data char after padding within a 4-char group: error
                break;
            } else if let Some(val) = base64_char_value(c, alphabet) {
                chunk.push(val);
                i += 1;
            } else {
                // invalid character
                let read = if chunk_start > 0 {
                    cleaned[chunk_start - 1].1 + 1
                } else {
                    0
                };
                return DecodeResult {
                    bytes: output,
                    read,
                    error: Some(format!("invalid character {}", c)),
                };
            }
        }

        let group_len = chunk.len() + padding_count;
        if group_len == 0 {
            break;
        }

        // Complete 4-char group (no padding)
        if chunk.len() == 4 && padding_count == 0 {
            if output.len() + 3 > max {
                i = chunk_start;
                break;
            }
            let b0 = (chunk[0] << 2) | (chunk[1] >> 4);
            let b1 = ((chunk[1] & 0x0F) << 4) | (chunk[2] >> 2);
            let b2 = ((chunk[2] & 0x03) << 6) | chunk[3];
            output.push(b0);
            output.push(b1);
            output.push(b2);
            // If we've reached maxLength, stop immediately
            if output.len() >= max {
                break;
            }
            continue;
        }

        // Padded 4-char group: 2 data + 2 pad
        if chunk.len() == 2 && padding_count == 2 && group_len == 4 {
            // Check for excess data/padding after this padded chunk BEFORE decoding
            if i < cleaned.len() {
                let read = if chunk_start > 0 {
                    cleaned[chunk_start - 1].1 + 1
                } else {
                    0
                };
                return DecodeResult {
                    bytes: output,
                    read,
                    error: Some("unexpected data after padding".to_string()),
                };
            }
            if output.len() + 1 > max {
                i = chunk_start;
                break;
            }
            if last_chunk_handling == "strict" && (chunk[1] & 0x0F) != 0 {
                let read = if chunk_start > 0 {
                    cleaned[chunk_start - 1].1 + 1
                } else {
                    0
                };
                return DecodeResult {
                    bytes: output,
                    read,
                    error: Some("non-zero padding bits".to_string()),
                };
            }
            let b0 = (chunk[0] << 2) | (chunk[1] >> 4);
            output.push(b0);
            if output.len() >= max {
                break;
            }
            continue;
        }

        // Padded 4-char group: 3 data + 1 pad
        if chunk.len() == 3 && padding_count == 1 && group_len == 4 {
            // Check for excess data after padded chunk BEFORE decoding
            if i < cleaned.len() {
                let read = if chunk_start > 0 {
                    cleaned[chunk_start - 1].1 + 1
                } else {
                    0
                };
                return DecodeResult {
                    bytes: output,
                    read,
                    error: Some("unexpected data after padding".to_string()),
                };
            }
            if output.len() + 2 > max {
                i = chunk_start;
                break;
            }
            if last_chunk_handling == "strict" && (chunk[2] & 0x03) != 0 {
                let read = if chunk_start > 0 {
                    cleaned[chunk_start - 1].1 + 1
                } else {
                    0
                };
                return DecodeResult {
                    bytes: output,
                    read,
                    error: Some("non-zero padding bits".to_string()),
                };
            }
            let b0 = (chunk[0] << 2) | (chunk[1] >> 4);
            let b1 = ((chunk[1] & 0x0F) << 4) | (chunk[2] >> 2);
            output.push(b0);
            output.push(b1);
            if output.len() >= max {
                break;
            }
            continue;
        }

        // Incomplete group (less than 4 chars total, end of input)
        // This includes: partial padding cases like "Zg=" (2 data + 1 pad = 3)
        // and unpadded partials like "Zg" (2 data, 0 pad = 2)

        if saw_padding {
            // Incomplete group with padding didn't complete to 4 chars.
            // Only allow stop-before-partial backup for 2+ data chars
            // (incomplete but potentially valid padding like "AA=").
            // With 0 or 1 data chars, padding is always invalid.
            if chunk.len() >= 2 && last_chunk_handling == "stop-before-partial" {
                i = chunk_start;
                break;
            }

            let read = if chunk_start > 0 {
                cleaned[chunk_start - 1].1 + 1
            } else {
                0
            };
            return DecodeResult {
                bytes: output,
                read,
                error: Some("invalid padding".to_string()),
            };
        }

        // Unpadded partial chunk at end: 1, 2, or 3 data chars
        if chunk.len() == 1 {
            if last_chunk_handling == "stop-before-partial" {
                i = chunk_start;
                break;
            }
            let read = if chunk_start > 0 {
                cleaned[chunk_start - 1].1 + 1
            } else {
                0
            };
            return DecodeResult {
                bytes: output,
                read,
                error: Some("incomplete chunk".to_string()),
            };
        }

        if chunk.len() == 2 {
            if last_chunk_handling == "stop-before-partial" {
                i = chunk_start;
                break;
            }
            if last_chunk_handling == "strict" {
                let read = if chunk_start > 0 {
                    cleaned[chunk_start - 1].1 + 1
                } else {
                    0
                };
                return DecodeResult {
                    bytes: output,
                    read,
                    error: Some("missing padding".to_string()),
                };
            }
            // loose: decode
            if output.len() + 1 > max {
                i = chunk_start;
                break;
            }
            let b0 = (chunk[0] << 2) | (chunk[1] >> 4);
            output.push(b0);
            continue;
        }

        if chunk.len() == 3 {
            if last_chunk_handling == "stop-before-partial" {
                i = chunk_start;
                break;
            }
            if last_chunk_handling == "strict" {
                let read = if chunk_start > 0 {
                    cleaned[chunk_start - 1].1 + 1
                } else {
                    0
                };
                return DecodeResult {
                    bytes: output,
                    read,
                    error: Some("missing padding".to_string()),
                };
            }
            // loose: decode
            if output.len() + 2 > max {
                i = chunk_start;
                break;
            }
            let b0 = (chunk[0] << 2) | (chunk[1] >> 4);
            let b1 = ((chunk[1] & 0x0F) << 4) | (chunk[2] >> 2);
            output.push(b0);
            output.push(b1);
            continue;
        }
    }

    // Calculate chars read from original input
    let chars_read = if i > 0 && i <= cleaned.len() {
        cleaned[i - 1].1 + 1
    } else if i == 0 {
        0
    } else {
        chars.len()
    };

    DecodeResult {
        bytes: output,
        read: chars_read,
        error: None,
    }
}

pub(super) fn decode_hex(input: &str, max_bytes: Option<usize>) -> DecodeResult {
    let chars: Vec<char> = input.chars().collect();
    let max = max_bytes.unwrap_or(usize::MAX);

    // Check odd length first (before maxLength check per spec)
    if !chars.len().is_multiple_of(2) {
        return DecodeResult {
            bytes: Vec::new(),
            read: 0,
            error: Some("hex string must have an even number of characters".to_string()),
        };
    }

    let mut output = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        if output.len() >= max {
            break;
        }
        let hi = match hex_digit(chars[i]) {
            Some(v) => v,
            None => {
                let read = output.len() * 2;
                return DecodeResult {
                    bytes: output,
                    read,
                    error: Some(format!("invalid hex character: {}", chars[i])),
                };
            }
        };
        let lo = match hex_digit(chars[i + 1]) {
            Some(v) => v,
            None => {
                let read = output.len() * 2;
                return DecodeResult {
                    bytes: output,
                    read,
                    error: Some(format!("invalid hex character: {}", chars[i + 1])),
                };
            }
        };
        output.push((hi << 4) | lo);
        i += 2;
    }

    let read = output.len() * 2;
    DecodeResult {
        bytes: output,
        read,
        error: None,
    }
}

fn hex_digit(c: char) -> Option<u8> {
    match c {
        '0'..='9' => Some(c as u8 - b'0'),
        'a'..='f' => Some(c as u8 - b'a' + 10),
        'A'..='F' => Some(c as u8 - b'A' + 10),
        _ => None,
    }
}

const BASE64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const BASE64URL_CHARS: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub(super) fn encode_base64(data: &[u8], alphabet: &str, omit_padding: bool) -> String {
    let table = if alphabet == "base64url" {
        BASE64URL_CHARS
    } else {
        BASE64_CHARS
    };

    let mut result = String::new();
    let chunks = data.chunks(3);

    for chunk in chunks {
        match chunk.len() {
            3 => {
                let b0 = chunk[0];
                let b1 = chunk[1];
                let b2 = chunk[2];
                result.push(table[(b0 >> 2) as usize] as char);
                result.push(table[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
                result.push(table[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
                result.push(table[(b2 & 0x3F) as usize] as char);
            }
            2 => {
                let b0 = chunk[0];
                let b1 = chunk[1];
                result.push(table[(b0 >> 2) as usize] as char);
                result.push(table[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
                result.push(table[((b1 & 0x0F) << 2) as usize] as char);
                if !omit_padding {
                    result.push('=');
                }
            }
            1 => {
                let b0 = chunk[0];
                result.push(table[(b0 >> 2) as usize] as char);
                result.push(table[((b0 & 0x03) << 4) as usize] as char);
                if !omit_padding {
                    result.push('=');
                    result.push('=');
                }
            }
            _ => {}
        }
    }
    result
}
