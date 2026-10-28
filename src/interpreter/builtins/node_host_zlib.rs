//! The `zlib` binding for Node host mode (issue #1239): a hidden
//! `__host_zlib` global that `node-builtin-loader.js` seeds as the private
//! module `internal/host/zlib`. Stream state lives in `NodeHostState::zlib`
//! behind integer handles; the JS handle classes in `node-lib/internal/zlib/`
//! hold every JS value (callbacks, buffers). See
//! `docs/adr/2026-10-27-0450-node-host-zlib.md`.

use super::super::*;
use super::node_host_brotli_ctx::BrotliCtx;
use super::node_host_crypto::window;
use super::node_host_zlib_ctx::{Mode, ZCtx, ZError};
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Default)]
pub(crate) struct ZlibState {
    next: u64,
    streams: HashMap<u64, Codec>,
}

const BROTLI_DECODE: i64 = 8;
const BROTLI_ENCODE: i64 = 9;

enum Codec {
    Zlib(ZCtx),
    Brotli(BrotliCtx),
}

impl Codec {
    fn work(&mut self, flush: i32, input: &[u8], output: &mut [u8]) {
        match self {
            Codec::Zlib(ctx) => ctx.work(flush, input, output),
            Codec::Brotli(ctx) => ctx.work(flush as u32, input, output),
        }
    }

    fn error_info(&self) -> Option<ZError> {
        match self {
            Codec::Zlib(ctx) => ctx.error_info(),
            Codec::Brotli(ctx) => ctx.error_info(),
        }
    }

    fn avail_in(&self) -> u32 {
        match self {
            Codec::Zlib(ctx) => ctx.avail_in(),
            Codec::Brotli(ctx) => ctx.avail_in(),
        }
    }

    fn avail_out(&self) -> u32 {
        match self {
            Codec::Zlib(ctx) => ctx.avail_out(),
            Codec::Brotli(ctx) => ctx.avail_out(),
        }
    }
}

type Native = fn(&mut Interpreter, &JsValue, &[JsValue]) -> Completion;

fn arg(args: &[JsValue], i: usize) -> &JsValue {
    args.get(i).unwrap_or(JsValue::undefined_ref())
}

fn js_str(s: &str) -> JsValue {
    JsValue::string(JsString::from_str(s))
}

macro_rules! tri {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(e) => return Completion::Throw(e),
        }
    };
}

fn int(interp: &mut Interpreter, v: &JsValue) -> Result<i64, JsValue> {
    let n = interp.to_number_value(v)?;
    Ok(if n.is_finite() { n as i64 } else { 0 })
}

fn stale(interp: &mut Interpreter) -> Completion {
    Completion::Throw(interp.create_type_error("zlib: invalid or closed handle"))
}

fn error_value(interp: &mut Interpreter, err: Option<ZError>) -> Completion {
    Completion::Normal(match err {
        None => JsValue::UNDEFINED,
        Some(e) => interp.create_array(vec![
            js_str(&e.message),
            JsValue::number(f64::from(e.errno)),
            js_str(&e.code),
        ]),
    })
}

/// (mode) -> handle
fn create(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let mode = tri!(int(interp, arg(args, 0)));
    let codec = match mode {
        BROTLI_DECODE | BROTLI_ENCODE => Codec::Brotli(BrotliCtx::new(mode == BROTLI_DECODE)),
        _ => match Mode::from_node(mode as u32) {
            Some(mode) => Codec::Zlib(ZCtx::new(mode)),
            None => return Completion::Throw(interp.create_type_error("zlib: invalid mode")),
        },
    };
    let state = &mut interp.node_host.zlib;
    state.next += 1;
    state.streams.insert(state.next, codec);
    Completion::Normal(JsValue::number(state.next as f64))
}

/// (id, windowBits, level, memLevel, strategy, dictionary | undefined, rejectGarbage)
fn init(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let id = tri!(int(interp, arg(args, 0))) as u64;
    let mut numbers = [0i64; 4];
    for (slot, n) in numbers.iter_mut().enumerate() {
        *n = tri!(int(interp, arg(args, slot + 1)));
    }
    let dictionary = view_bytes(interp, arg(args, 5)).unwrap_or_default();
    let reject = interp.to_boolean_val(arg(args, 6));
    let [window_bits, level, mem_level, strategy] = numbers;
    match interp.node_host.zlib.streams.get_mut(&id) {
        Some(Codec::Zlib(ctx)) => ctx.init(
            level as i32,
            window_bits as i32,
            mem_level as i32,
            strategy as i32,
            reject,
            dictionary,
        ),
        _ => return stale(interp),
    }
    Completion::Normal(JsValue::UNDEFINED)
}

/// (id, params: Uint32Array, dictionary | undefined) -> whether the codec
/// accepted every parameter
fn brotli_init(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let id = tri!(int(interp, arg(args, 0))) as u64;
    let Some(params) = view_bytes(interp, arg(args, 1)) else {
        return Completion::Throw(interp.create_type_error("zlib: expected a Uint32Array"));
    };
    let params: Vec<u32> = params
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_ne_bytes(*c))
        .collect();
    let dictionary = view_bytes(interp, arg(args, 2)).unwrap_or_default();
    match interp.node_host.zlib.streams.get_mut(&id) {
        Some(Codec::Brotli(ctx)) => {
            Completion::Normal(JsValue::boolean(ctx.init(&params, &dictionary)))
        }
        _ => stale(interp),
    }
}

/// A copy of the bytes an ArrayBuffer view covers.
fn view_bytes(interp: &mut Interpreter, v: &JsValue) -> Option<Vec<u8>> {
    let w = window(interp, v)?;
    w.buffer
        .borrow()
        .with_read(|b| b.get(w.offset..w.offset + w.len).map(<[u8]>::to_vec))
}

fn bounds(offset: i64, len: i64, total: usize) -> Option<(usize, usize)> {
    let (offset, len) = (usize::try_from(offset).ok()?, usize::try_from(len).ok()?);
    (offset.checked_add(len)? <= total).then_some((offset, len))
}

/// (id, flush, in | null, inOff, inLen, out, outOff, outLen, writeState):
/// undefined on success, else [message, errno, code]. `writeState[0]` and
/// `[1]` receive avail_out and avail_in, as Node's `UpdateWriteResult` does.
fn write(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let id = tri!(int(interp, arg(args, 0))) as u64;
    let flush = tri!(int(interp, arg(args, 1))) as i32;
    let range_error = |interp: &mut Interpreter, what: &str| {
        Completion::Throw(interp.create_range_error(&format!("{what} buffer is out of bounds")))
    };
    let mut input = None;
    if !arg(args, 2).is_null() {
        let (off, len) = (
            tri!(int(interp, arg(args, 3))),
            tri!(int(interp, arg(args, 4))),
        );
        let Some(w) = window(interp, arg(args, 2)) else {
            return Completion::Throw(interp.create_type_error("zlib: expected a buffer"));
        };
        let Some((off, len)) = bounds(off, len, w.len) else {
            return range_error(interp, "input");
        };
        input = Some((w.buffer, w.offset + off, len));
    }
    let (off, len) = (
        tri!(int(interp, arg(args, 6))),
        tri!(int(interp, arg(args, 7))),
    );
    let Some(out_window) = window(interp, arg(args, 5)) else {
        return Completion::Throw(interp.create_type_error("zlib: expected a buffer"));
    };
    let Some((out_off, out_len)) = bounds(off, len, out_window.len) else {
        return range_error(interp, "output");
    };
    let out_start = out_window.offset + out_off;
    let Some(state_window) = window(interp, arg(args, 8)) else {
        return Completion::Throw(interp.create_type_error("zlib: expected a Uint32Array"));
    };
    if state_window.len < 8 {
        return range_error(interp, "write state");
    }

    let Some(ctx) = interp.node_host.zlib.streams.get_mut(&id) else {
        return stale(interp);
    };
    let ran = match input {
        None => out_window.buffer.borrow_mut().with_write(|b| {
            b.get_mut(out_start..out_start + out_len)
                .map(|out| ctx.work(flush, &[], out))
                .is_some()
        }),
        Some((buffer, start, len)) if Rc::ptr_eq(&buffer, &out_window.buffer) => {
            let mut shared = out_window.buffer.borrow_mut();
            shared.with_write(|b| {
                let Some(bytes) = b.get(start..start + len).map(<[u8]>::to_vec) else {
                    return false;
                };
                b.get_mut(out_start..out_start + out_len)
                    .map(|out| ctx.work(flush, &bytes, out))
                    .is_some()
            })
        }
        Some((buffer, start, len)) => buffer.borrow().with_read(|input| {
            let Some(input) = input.get(start..start + len) else {
                return false;
            };
            out_window.buffer.borrow_mut().with_write(|b| {
                b.get_mut(out_start..out_start + out_len)
                    .map(|out| ctx.work(flush, input, out))
                    .is_some()
            })
        }),
    };
    if !ran {
        return range_error(interp, "output");
    }
    if let Some(err) = ctx.error_info() {
        return error_value(interp, Some(err));
    }
    let (avail_out, avail_in) = (ctx.avail_out(), ctx.avail_in());
    let at = state_window.offset;
    state_window.buffer.borrow_mut().with_write(|b| {
        if let Some(slot) = b.get_mut(at..at + 8) {
            slot[..4].copy_from_slice(&avail_out.to_ne_bytes());
            slot[4..].copy_from_slice(&avail_in.to_ne_bytes());
        }
    });
    Completion::Normal(JsValue::UNDEFINED)
}

/// (id, level, strategy) -> undefined | [message, errno, code]
fn params(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let id = tri!(int(interp, arg(args, 0))) as u64;
    let level = tri!(int(interp, arg(args, 1))) as i32;
    let strategy = tri!(int(interp, arg(args, 2))) as i32;
    match interp.node_host.zlib.streams.get_mut(&id) {
        Some(Codec::Zlib(ctx)) => {
            let err = ctx.set_params(level, strategy);
            error_value(interp, err)
        }
        Some(Codec::Brotli(_)) => Completion::Normal(JsValue::UNDEFINED),
        None => stale(interp),
    }
}

/// (id) -> undefined | [message, errno, code]
fn reset(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let id = tri!(int(interp, arg(args, 0))) as u64;
    match interp.node_host.zlib.streams.get_mut(&id) {
        Some(Codec::Zlib(ctx)) => {
            let err = ctx.reset_stream();
            error_value(interp, err)
        }
        Some(Codec::Brotli(ctx)) => {
            ctx.reset();
            Completion::Normal(JsValue::UNDEFINED)
        }
        None => stale(interp),
    }
}

fn close(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let id = tri!(int(interp, arg(args, 0))) as u64;
    interp.node_host.zlib.streams.remove(&id);
    Completion::Normal(JsValue::UNDEFINED)
}

/// (data: string | view, value) -> the CRC-32 of `data` continuing from `value`
fn crc32(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let start = tri!(int(interp, arg(args, 1))) as u32;
    let data = arg(args, 0);
    let bytes = if data.is_string() {
        tri!(interp.to_string_lossy(data)).into_bytes()
    } else {
        let Some(w) = window(interp, data) else {
            return Completion::Throw(interp.create_type_error("zlib: expected a string or view"));
        };
        let copied = w
            .buffer
            .borrow()
            .with_read(|b| b.get(w.offset..w.offset + w.len).map(<[u8]>::to_vec));
        copied.unwrap_or_default()
    };
    let mut crc = u64::from(start);
    for chunk in bytes.chunks(1 << 30) {
        // SAFETY: `chunk` is a live slice and its length fits the C `uInt`.
        crc = unsafe { libz_rs_sys::crc32(crc, chunk.as_ptr(), chunk.len() as u32) };
    }
    Completion::Normal(JsValue::number(crc as f64))
}

impl Interpreter {
    pub(super) fn setup_node_host_zlib(&mut self) {
        let binding = self.create_object_id();
        let functions: [(&str, usize, Native); 8] = [
            ("create", 1, create),
            ("init", 7, init),
            ("brotliInit", 2, brotli_init),
            ("write", 9, write),
            ("params", 3, params),
            ("reset", 1, reset),
            ("close", 1, close),
            ("crc32", 2, crc32),
        ];
        for (name, arity, native) in functions {
            let f = self.create_function(JsFunction::native(String::new(), arity, native));
            self.get_object_cell_expect(binding)
                .borrow_mut()
                .insert_value(name, f);
        }
        self.install_host_global("__host_zlib", JsValue::object(binding));
    }
}
