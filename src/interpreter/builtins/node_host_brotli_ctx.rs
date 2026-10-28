//! One Brotli stream for Node host mode (issue #1239): a port of
//! `BrotliEncoderContext` / `BrotliDecoderContext` from Node's `src/node_zlib.cc`
//! (v26.8.2) over the safe-Rust `brotli` crate, whose stream functions have the
//! shape of the C API Node calls. A reset re-initialises the codec with default
//! parameters, as `ResetStream` does there.

use super::node_host_zlib_ctx::ZError;
use brotli::enc::StandardAlloc;
use brotli::enc::encode::{
    BrotliEncoderOperation, BrotliEncoderParameter, BrotliEncoderStateStruct,
};
use brotli::{Allocator, BrotliDecompressStream, BrotliResult, BrotliState, SliceWrapperMut};

const UNEXPECTED_EOF_ERRNO: i32 = -5;
const DECODER_PARAM_LARGE_WINDOW: usize = 1;
const DECODER_PARAMS: usize = 2;
const ENCODER_PARAM_NPOSTFIX: usize = 7;
const ENCODER_PARAM_NDIRECT: usize = 8;

type Decoder = BrotliState<StandardAlloc, StandardAlloc, StandardAlloc>;
type Encoder = BrotliEncoderStateStruct<StandardAlloc>;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum LastResult {
    Success,
    NeedsMoreInput,
    NeedsMoreOutput,
    Error,
}

pub(super) enum BrotliCtx {
    Encode {
        state: Box<Encoder>,
        last_ok: bool,
        avail_in: u32,
        avail_out: u32,
    },
    Decode {
        state: Box<Decoder>,
        flush: u32,
        last: LastResult,
        error: i32,
        avail_in: u32,
        avail_out: u32,
    },
}

fn new_encoder() -> Box<Encoder> {
    Box::new(Encoder::new(StandardAlloc::default()))
}

fn new_decoder() -> Box<Decoder> {
    Box::new(Decoder::new_strict(
        StandardAlloc::default(),
        StandardAlloc::default(),
        StandardAlloc::default(),
    ))
}

fn encoder_parameter(key: usize) -> Option<BrotliEncoderParameter> {
    use BrotliEncoderParameter::*;
    Some(match key {
        0 => BROTLI_PARAM_MODE,
        1 => BROTLI_PARAM_QUALITY,
        2 => BROTLI_PARAM_LGWIN,
        3 => BROTLI_PARAM_LGBLOCK,
        4 => BROTLI_PARAM_DISABLE_LITERAL_CONTEXT_MODELING,
        5 => BROTLI_PARAM_SIZE_HINT,
        6 => BROTLI_PARAM_LARGE_WINDOW,
        _ => return None,
    })
}

impl BrotliCtx {
    pub(super) fn new(decode: bool) -> BrotliCtx {
        if decode {
            BrotliCtx::Decode {
                state: new_decoder(),
                flush: 0,
                last: LastResult::Success,
                error: 0,
                avail_in: 0,
                avail_out: 0,
            }
        } else {
            BrotliCtx::Encode {
                state: new_encoder(),
                last_ok: false,
                avail_in: 0,
                avail_out: 0,
            }
        }
    }

    /// Applies every parameter that is not the `-1` placeholder, then the
    /// dictionary; false when the codec rejects a parameter (`SetParams`
    /// failing in Node).
    pub(super) fn init(&mut self, params: &[u32], dictionary: &[u8]) -> bool {
        if let BrotliCtx::Decode { state, .. } = self
            && !dictionary.is_empty()
        {
            let mut alloc = StandardAlloc::default();
            let mut dict = alloc.alloc_cell(dictionary.len());
            dict.slice_mut().copy_from_slice(dictionary);
            **state = Decoder::new_with_custom_dictionary(
                StandardAlloc::default(),
                StandardAlloc::default(),
                StandardAlloc::default(),
                dict,
            );
            state.large_window = false;
        }
        if !self.set_params(params) {
            return false;
        }
        if let BrotliCtx::Encode { state, .. } = self
            && !dictionary.is_empty()
        {
            state.set_custom_dictionary(dictionary.len(), dictionary);
        }
        true
    }

    fn set_params(&mut self, params: &[u32]) -> bool {
        for (key, &value) in params.iter().enumerate() {
            if value == u32::MAX {
                continue;
            }
            let accepted = match self {
                BrotliCtx::Encode { state, .. } => match encoder_parameter(key) {
                    Some(BrotliEncoderParameter::BROTLI_PARAM_MODE) => state.set_parameter(
                        BrotliEncoderParameter::BROTLI_PARAM_MODE,
                        if value > 2 { 0 } else { value },
                    ),
                    Some(p) => state.set_parameter(p, value),
                    None => matches!(key, ENCODER_PARAM_NPOSTFIX | ENCODER_PARAM_NDIRECT),
                },
                BrotliCtx::Decode { state, .. } => {
                    if key == DECODER_PARAM_LARGE_WINDOW {
                        state.large_window = value != 0;
                    }
                    key < DECODER_PARAMS
                }
            };
            if !accepted {
                return false;
            }
        }
        true
    }

    /// `ResetStream`: a fresh codec with default parameters.
    pub(super) fn reset(&mut self) {
        *self = BrotliCtx::new(matches!(self, BrotliCtx::Decode { .. }));
    }

    pub(super) fn work(&mut self, flush: u32, input: &[u8], output: &mut [u8]) {
        let (mut avail_in, mut in_off) = (input.len(), 0usize);
        let (mut avail_out, mut out_off) = (output.len(), 0usize);
        match self {
            BrotliCtx::Encode {
                state,
                last_ok,
                avail_in: left_in,
                avail_out: left_out,
            } => {
                let op = match flush {
                    0 => Some(BrotliEncoderOperation::BROTLI_OPERATION_PROCESS),
                    1 => Some(BrotliEncoderOperation::BROTLI_OPERATION_FLUSH),
                    2 => Some(BrotliEncoderOperation::BROTLI_OPERATION_FINISH),
                    3 => Some(BrotliEncoderOperation::BROTLI_OPERATION_EMIT_METADATA),
                    _ => None,
                };
                *last_ok = op.is_some_and(|op| {
                    state.compress_stream(
                        op,
                        &mut avail_in,
                        input,
                        &mut in_off,
                        &mut avail_out,
                        output,
                        &mut out_off,
                        &mut None,
                        &mut |_, _, _, _| (),
                    )
                });
                *left_in = avail_in as u32;
                *left_out = avail_out as u32;
            }
            BrotliCtx::Decode {
                state,
                flush: stored_flush,
                last,
                error,
                avail_in: left_in,
                avail_out: left_out,
            } => {
                *stored_flush = flush;
                let mut total = 0usize;
                *last = match BrotliDecompressStream(
                    &mut avail_in,
                    &mut in_off,
                    input,
                    &mut avail_out,
                    &mut out_off,
                    output,
                    &mut total,
                    state,
                ) {
                    BrotliResult::ResultSuccess => LastResult::Success,
                    BrotliResult::NeedsMoreInput => LastResult::NeedsMoreInput,
                    BrotliResult::NeedsMoreOutput => LastResult::NeedsMoreOutput,
                    BrotliResult::ResultFailure => LastResult::Error,
                };
                if *last == LastResult::Error {
                    *error = decoder_errno(state);
                }
                *left_in = avail_in as u32;
                *left_out = avail_out as u32;
            }
        }
    }

    pub(super) fn avail_in(&self) -> u32 {
        match self {
            BrotliCtx::Encode { avail_in, .. } | BrotliCtx::Decode { avail_in, .. } => *avail_in,
        }
    }

    pub(super) fn avail_out(&self) -> u32 {
        match self {
            BrotliCtx::Encode { avail_out, .. } | BrotliCtx::Decode { avail_out, .. } => *avail_out,
        }
    }

    pub(super) fn error_info(&self) -> Option<ZError> {
        match self {
            BrotliCtx::Encode { last_ok: false, .. } => Some(ZError {
                message: "Compression failed".into(),
                errno: -1,
                code: "ERR_BROTLI_COMPRESSION_FAILED".into(),
            }),
            BrotliCtx::Encode { .. } => None,
            BrotliCtx::Decode { error, .. } if *error != 0 => Some(ZError {
                message: "Decompression failed".into(),
                errno: *error,
                code: decoder_error_code(*error).into(),
            }),
            BrotliCtx::Decode { flush, last, .. }
                if *flush == 2 && *last == LastResult::NeedsMoreInput =>
            {
                Some(ZError {
                    message: "unexpected end of file".into(),
                    errno: UNEXPECTED_EOF_ERRNO,
                    code: "Z_BUF_ERROR".into(),
                })
            }
            BrotliCtx::Decode { .. } => None,
        }
    }
}

/// The decoder's error code as libbrotli numbers it. This crate classifies two
/// failures differently, and the state it failed in tells them apart (its state
/// enum is not exported to name, so it is matched by its `Debug` name; the
/// unit tests and the oracle's mutation fuzz pin the cases below):
/// nonzero padding before a metadata or uncompressed block is PADDING_1 (-14)
/// there but PADDING_2 (-15) here, and a copy running past the end of its
/// metablock is BLOCK_LENGTH_2 (-10) there, found before the padding check or
/// the ring-buffer write that report -15 and BLOCK_LENGTH_1 (-9) here.
fn decoder_errno(state: &Decoder) -> i32 {
    let errno = state.error_code as i32;
    if errno != -15 && errno != -9 {
        return errno;
    }
    let overran = state.meta_block_remaining_len < 0;
    match (errno, format!("{:?}", state.state).as_str()) {
        (-15, "BROTLI_STATE_METABLOCK_HEADER") => -14,
        (-15, "BROTLI_STATE_METABLOCK_DONE")
        | (-9, "BROTLI_STATE_DONE" | "BROTLI_STATE_COMMAND_POST_WRITE_2")
            if overran =>
        {
            -10
        }
        _ => errno,
    }
}

/// `"ERR_" + BrotliDecoderErrorString(code)`, e.g. `ERR__ERROR_FORMAT_CL_SPACE`.
fn decoder_error_code(errno: i32) -> String {
    let name = match errno {
        -1 => "_ERROR_FORMAT_EXUBERANT_NIBBLE",
        -2 => "_ERROR_FORMAT_RESERVED",
        -3 => "_ERROR_FORMAT_EXUBERANT_META_NIBBLE",
        -4 => "_ERROR_FORMAT_SIMPLE_HUFFMAN_ALPHABET",
        -5 => "_ERROR_FORMAT_SIMPLE_HUFFMAN_SAME",
        -6 => "_ERROR_FORMAT_CL_SPACE",
        -7 => "_ERROR_FORMAT_HUFFMAN_SPACE",
        -8 => "_ERROR_FORMAT_CONTEXT_MAP_REPEAT",
        -9 => "_ERROR_FORMAT_BLOCK_LENGTH_1",
        -10 => "_ERROR_FORMAT_BLOCK_LENGTH_2",
        -11 => "_ERROR_FORMAT_TRANSFORM",
        -12 => "_ERROR_FORMAT_DICTIONARY",
        -13 => "_ERROR_FORMAT_WINDOW_BITS",
        -14 => "_ERROR_FORMAT_PADDING_1",
        -15 => "_ERROR_FORMAT_PADDING_2",
        -16 => "_ERROR_FORMAT_DISTANCE",
        -19 => "_ERROR_DICTIONARY_NOT_SET",
        -20 => "_ERROR_INVALID_ARGUMENTS",
        -21 => "_ERROR_ALLOC_CONTEXT_MODES",
        -22 => "_ERROR_ALLOC_TREE_GROUPS",
        -25 => "_ERROR_ALLOC_CONTEXT_MAP",
        -26 => "_ERROR_ALLOC_RING_BUFFER_1",
        -27 => "_ERROR_ALLOC_RING_BUFFER_2",
        -30 => "_ERROR_ALLOC_BLOCK_TYPE_TREES",
        _ => "_ERROR_UNREACHABLE",
    };
    format!("ERR_{name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(ctx: &mut BrotliCtx, flush: u32, input: &[u8]) -> Vec<u8> {
        let mut out = vec![0u8; 1 << 16];
        ctx.work(flush, input, &mut out);
        let used = out.len() - ctx.avail_out() as usize;
        out.truncate(used);
        out
    }

    #[test]
    fn empty_stream_is_one_byte() {
        let mut enc = BrotliCtx::new(false);
        assert_eq!(run(&mut enc, 2, b""), [0x3b]);
        assert!(enc.error_info().is_none());
    }

    #[test]
    fn round_trips_at_the_default_quality() {
        let text = b"hello brotli hello brotli hello brotli".repeat(20);
        let mut enc = BrotliCtx::new(false);
        let packed = run(&mut enc, 2, &text);
        assert!(packed.len() < text.len());
        let mut dec = BrotliCtx::new(true);
        assert_eq!(run(&mut dec, 2, &packed), text);
        assert!(dec.error_info().is_none());
    }

    #[test]
    fn truncated_input_with_finish_is_an_error() {
        let mut enc = BrotliCtx::new(false);
        let packed = run(&mut enc, 2, &b"abcdefghij".repeat(100));
        let mut dec = BrotliCtx::new(true);
        run(&mut dec, 2, &packed[..5]);
        let err = dec.error_info().expect("error");
        assert_eq!(
            (err.message.as_str(), &*err.code, err.errno),
            ("unexpected end of file", "Z_BUF_ERROR", -5)
        );
    }

    #[test]
    fn garbage_reports_the_decoder_error_code() {
        let mut dec = BrotliCtx::new(true);
        run(&mut dec, 2, b"hello world, not brotli");
        let err = dec.error_info().expect("error");
        assert_eq!(
            (err.message.as_str(), &*err.code, err.errno),
            ("Decompression failed", "ERR__ERROR_FORMAT_CL_SPACE", -6)
        );
    }

    fn decode_error(input: &[u8]) -> (i32, String) {
        let mut dec = BrotliCtx::new(true);
        run(&mut dec, 2, input);
        let err = dec.error_info().expect("error");
        (err.errno, err.code.into_owned())
    }

    #[test]
    fn crate_error_numbers_are_mapped_to_libbrotlis() {
        assert_eq!(
            decode_error(b"this is not brotli data at all"),
            (-14, "ERR__ERROR_FORMAT_PADDING_1".to_string())
        );
        let packed = [
            0x1b, 0x8c, 0x00, 0x00, 0x24, 0x40, 0x7a, 0x90, 0x45, 0x9a, 0x01, 0x4c, 0x2f, 0xd3,
            0xe5, 0xe1, 0x69, 0xad, 0x32, 0x4e, 0x00,
        ];
        assert_eq!(decode_error(&packed_with_flip(&packed, 1, 0)).0, -10);
        assert_eq!(decode_error(&packed_with_flip(&packed, 1, 7)).0, -10);
        assert_eq!(decode_error(&packed_with_flip(&packed, 10, 5)).0, -10);
        assert_eq!(decode_error(&packed_with_flip(&packed, 19, 3)).0, -15);
    }

    fn packed_with_flip(packed: &[u8], at: usize, bit: u32) -> Vec<u8> {
        let mut bad = packed.to_vec();
        bad[at] ^= 1 << bit;
        bad
    }

    #[test]
    fn parameters_are_validated_like_node() {
        let mut enc = BrotliCtx::new(false);
        let mut params = [u32::MAX; 9];
        params[4] = 2;
        assert!(!enc.init(&params, &[]));
        params[4] = 1;
        assert!(enc.init(&params, &[]));
        let mut dec = BrotliCtx::new(true);
        let mut params = [u32::MAX; 9];
        params[1] = 1;
        assert!(dec.init(&params, &[]));
        params[2] = 1;
        assert!(!dec.init(&params, &[]));
    }
}
