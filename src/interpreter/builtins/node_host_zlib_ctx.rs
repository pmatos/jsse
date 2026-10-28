//! One zlib stream for Node host mode (issue #1239): a port of `ZlibContext`
//! from Node's `src/node_zlib.cc` (v26.8.2) over `libz-rs-sys`' z_stream API.
//! Stream setup is lazy as in Node, so an init failure surfaces on the first
//! write, reset or params call.

use libz_rs_sys::{
    Z_BUF_ERROR, Z_DATA_ERROR, Z_DEFLATED, Z_ERRNO, Z_FINISH, Z_MEM_ERROR, Z_NEED_DICT, Z_NO_FLUSH,
    Z_OK, Z_STREAM_END, Z_STREAM_ERROR, Z_VERSION_ERROR, deflate, deflateEnd, deflateInit2_,
    deflateParams, deflateReset, deflateSetDictionary, inflate, inflateEnd, inflateInit2_,
    inflateReset, inflateSetDictionary, z_stream, zlibVersion,
};
use std::borrow::Cow;
use std::ffi::CStr;
use std::os::raw::c_int;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    None,
    Deflate,
    Inflate,
    Gzip,
    Gunzip,
    DeflateRaw,
    InflateRaw,
    Unzip,
}

impl Mode {
    pub(super) fn from_node(mode: u32) -> Option<Mode> {
        Some(match mode {
            1 => Mode::Deflate,
            2 => Mode::Inflate,
            3 => Mode::Gzip,
            4 => Mode::Gunzip,
            5 => Mode::DeflateRaw,
            6 => Mode::InflateRaw,
            7 => Mode::Unzip,
            _ => return None,
        })
    }

    fn deflates(self) -> bool {
        matches!(self, Mode::Deflate | Mode::Gzip | Mode::DeflateRaw)
    }

    fn inflates(self) -> bool {
        matches!(
            self,
            Mode::Inflate | Mode::Gunzip | Mode::InflateRaw | Mode::Unzip
        )
    }
}

pub(super) struct ZError {
    pub(super) message: String,
    pub(super) errno: c_int,
    pub(super) code: Cow<'static, str>,
}

fn strerror(err: c_int) -> &'static str {
    match err {
        Z_OK => "Z_OK",
        Z_STREAM_END => "Z_STREAM_END",
        Z_NEED_DICT => "Z_NEED_DICT",
        Z_ERRNO => "Z_ERRNO",
        Z_STREAM_ERROR => "Z_STREAM_ERROR",
        Z_DATA_ERROR => "Z_DATA_ERROR",
        Z_MEM_ERROR => "Z_MEM_ERROR",
        Z_BUF_ERROR => "Z_BUF_ERROR",
        Z_VERSION_ERROR => "Z_VERSION_ERROR",
        _ => "Z_UNKNOWN_ERROR",
    }
}

const GZIP_HEADER_ID1: u8 = 0x1f;
const GZIP_HEADER_ID2: u8 = 0x8b;

pub(super) struct ZCtx {
    strm: Box<z_stream>,
    mode: Mode,
    level: c_int,
    window_bits: c_int,
    mem_level: c_int,
    strategy: c_int,
    flush: c_int,
    err: c_int,
    dictionary: Vec<u8>,
    reject_garbage: bool,
    gzip_id_bytes_read: u8,
    zlib_init_done: bool,
}

impl ZCtx {
    pub(super) fn new(mode: Mode) -> ZCtx {
        // SAFETY: z_stream is plain data (integers, raw pointers and optional
        // function pointers) for which all-zero is the documented initial state.
        let strm: z_stream = unsafe { std::mem::zeroed() };
        ZCtx {
            strm: Box::new(strm),
            mode,
            level: 0,
            window_bits: 0,
            mem_level: 0,
            strategy: 0,
            flush: Z_NO_FLUSH,
            err: Z_OK,
            dictionary: Vec::new(),
            reject_garbage: false,
            gzip_id_bytes_read: 0,
            zlib_init_done: false,
        }
    }

    pub(super) fn init(
        &mut self,
        level: c_int,
        window_bits: c_int,
        mem_level: c_int,
        strategy: c_int,
        reject_garbage: bool,
        dictionary: Vec<u8>,
    ) {
        self.level = level;
        self.window_bits = window_bits;
        self.mem_level = mem_level;
        self.strategy = strategy;
        self.reject_garbage = reject_garbage;
        self.flush = Z_NO_FLUSH;
        self.err = Z_OK;
        match self.mode {
            Mode::Gzip | Mode::Gunzip => self.window_bits += 16,
            Mode::Unzip => self.window_bits += 32,
            Mode::DeflateRaw | Mode::InflateRaw => self.window_bits *= -1,
            _ => {}
        }
        self.dictionary = dictionary;
    }

    fn error_for_message(&self, message: &str) -> ZError {
        let message = if self.strm.msg.is_null() {
            message.to_string()
        } else {
            // SAFETY: zlib points `msg` at a static NUL-terminated string.
            unsafe { CStr::from_ptr(self.strm.msg) }
                .to_string_lossy()
                .into_owned()
        };
        ZError {
            message,
            errno: self.err,
            code: Cow::Borrowed(strerror(self.err)),
        }
    }

    pub(super) fn error_info(&self) -> Option<ZError> {
        match self.err {
            Z_OK | Z_BUF_ERROR => {
                if self.strm.avail_out != 0 && self.flush == Z_FINISH {
                    return Some(self.error_for_message("unexpected end of file"));
                }
                None
            }
            Z_STREAM_END => None,
            Z_NEED_DICT => Some(self.error_for_message(if self.dictionary.is_empty() {
                "Missing dictionary"
            } else {
                "Bad dictionary"
            })),
            _ => Some(self.error_for_message("Zlib error")),
        }
    }

    /// True on the call that initialised the stream (successfully or not).
    fn init_zlib(&mut self) -> bool {
        if self.zlib_init_done {
            return false;
        }
        self.strm.msg = std::ptr::null_mut();
        let size = std::mem::size_of::<z_stream>() as c_int;
        let strm: *mut z_stream = &mut *self.strm;
        // SAFETY: `strm` is a zeroed, heap-pinned z_stream.
        unsafe {
            if self.mode.deflates() {
                self.err = deflateInit2_(
                    strm,
                    self.level,
                    Z_DEFLATED,
                    self.window_bits,
                    self.mem_level,
                    self.strategy,
                    zlibVersion(),
                    size,
                );
            } else if self.mode.inflates() {
                self.err = inflateInit2_(strm, self.window_bits, zlibVersion(), size);
            }
        }
        if self.err != Z_OK {
            self.dictionary.clear();
            self.mode = Mode::None;
            return true;
        }
        let _ = self.set_dictionary();
        self.zlib_init_done = true;
        true
    }

    /// The work of one `write()`: compress or inflate `input` into `output`
    /// with `flush`; read the leftovers from `avail_in`/`avail_out`.
    pub(super) fn work(&mut self, flush: c_int, input: &[u8], output: &mut [u8]) {
        self.flush = flush;
        self.strm.next_in = input.as_ptr() as *mut u8;
        self.strm.avail_in = input.len() as u32;
        self.strm.next_out = output.as_mut_ptr();
        self.strm.avail_out = output.len() as u32;
        self.process();
        self.strm.next_in = std::ptr::null_mut();
        self.strm.next_out = std::ptr::null_mut();
    }

    pub(super) fn avail_in(&self) -> u32 {
        self.strm.avail_in
    }

    pub(super) fn avail_out(&self) -> u32 {
        self.strm.avail_out
    }

    fn process(&mut self) {
        if self.init_zlib() && self.err != Z_OK {
            return;
        }
        let strm: *mut z_stream = &mut *self.strm;
        // SAFETY: the stream is initialised and its buffers were set by `work`
        // to slices that outlive this call.
        unsafe {
            if self.mode.deflates() {
                self.err = deflate(strm, self.flush);
                return;
            }
            if self.mode == Mode::Unzip {
                self.detect_gzip();
            }
            self.err = inflate(strm, self.flush);
            if self.mode != Mode::InflateRaw
                && self.err == Z_NEED_DICT
                && !self.dictionary.is_empty()
            {
                self.err = inflateSetDictionary(
                    strm,
                    self.dictionary.as_ptr(),
                    self.dictionary.len() as u32,
                );
                if self.err == Z_OK {
                    self.err = inflate(strm, self.flush);
                } else if self.err == Z_DATA_ERROR {
                    self.err = Z_NEED_DICT;
                }
            }
            while self.strm.avail_in > 0
                && self.mode == Mode::Gunzip
                && self.err == Z_STREAM_END
                && !self.reject_garbage
                && *self.strm.next_in != 0
            {
                let _ = self.reset_stream();
                self.err = inflate(strm, self.flush);
            }
        }
    }

    /// UNZIP decides between gzip and zlib from the first two input bytes,
    /// which may arrive in separate writes.
    unsafe fn detect_gzip(&mut self) {
        if self.strm.avail_in == 0 {
            return;
        }
        // SAFETY: `avail_in > 0` bytes are readable at `next_in`.
        let (first, second) = unsafe {
            let p = self.strm.next_in;
            (*p, (self.strm.avail_in > 1).then(|| *p.add(1)))
        };
        if self.gzip_id_bytes_read == 0 {
            if first != GZIP_HEADER_ID1 {
                self.mode = Mode::Inflate;
                return;
            }
            self.gzip_id_bytes_read = 1;
            if let Some(b) = second {
                self.settle_gzip(b);
            }
        } else if self.gzip_id_bytes_read == 1 {
            self.settle_gzip(first);
        }
    }

    fn settle_gzip(&mut self, byte: u8) {
        if byte == GZIP_HEADER_ID2 {
            self.gzip_id_bytes_read = 2;
            self.mode = Mode::Gunzip;
        } else {
            self.mode = Mode::Inflate;
        }
    }

    pub(super) fn reset_stream(&mut self) -> Option<ZError> {
        if self.init_zlib() && self.err != Z_OK {
            return Some(self.error_for_message("Failed to init stream before reset"));
        }
        self.err = Z_OK;
        let strm: *mut z_stream = &mut *self.strm;
        // SAFETY: the stream is initialised.
        unsafe {
            if self.mode.deflates() {
                self.err = deflateReset(strm);
            } else if matches!(self.mode, Mode::Inflate | Mode::InflateRaw | Mode::Gunzip) {
                self.err = inflateReset(strm);
            }
        }
        if self.err != Z_OK {
            return Some(self.error_for_message("Failed to reset stream"));
        }
        self.set_dictionary()
    }

    fn set_dictionary(&mut self) -> Option<ZError> {
        if self.dictionary.is_empty() {
            return None;
        }
        self.err = Z_OK;
        let strm: *mut z_stream = &mut *self.strm;
        let (ptr, len) = (self.dictionary.as_ptr(), self.dictionary.len() as u32);
        // SAFETY: the stream is initialised; the dictionary outlives the call.
        unsafe {
            match self.mode {
                Mode::Deflate | Mode::DeflateRaw => self.err = deflateSetDictionary(strm, ptr, len),
                Mode::InflateRaw => self.err = inflateSetDictionary(strm, ptr, len),
                _ => {}
            }
        }
        if self.err != Z_OK {
            return Some(self.error_for_message("Failed to set dictionary"));
        }
        None
    }

    pub(super) fn set_params(&mut self, level: c_int, strategy: c_int) -> Option<ZError> {
        if self.init_zlib() && self.err != Z_OK {
            return Some(self.error_for_message("Failed to init stream before set parameters"));
        }
        self.err = Z_OK;
        if matches!(self.mode, Mode::Deflate | Mode::DeflateRaw) {
            let strm: *mut z_stream = &mut *self.strm;
            // zlib rejects a null `next_out`; with no output space its flush
            // reports Z_BUF_ERROR (tolerated below, as in Node) and the
            // parameters change once nothing is pending.
            self.strm.next_out = std::ptr::NonNull::<u8>::dangling().as_ptr();
            self.strm.avail_out = 0;
            self.strm.avail_in = 0;
            // SAFETY: the stream is initialised and no buffer is read or written.
            self.err = unsafe { deflateParams(strm, level, strategy) };
            self.strm.next_out = std::ptr::null_mut();
        }
        if self.err != Z_OK && self.err != Z_BUF_ERROR {
            return Some(self.error_for_message("Failed to set parameters"));
        }
        None
    }

    pub(super) fn close(&mut self) {
        if !self.zlib_init_done {
            self.dictionary.clear();
            self.mode = Mode::None;
            return;
        }
        let strm: *mut z_stream = &mut *self.strm;
        // SAFETY: the stream was initialised and is ended exactly once.
        unsafe {
            if self.mode.deflates() {
                deflateEnd(strm);
            } else if self.mode.inflates() {
                inflateEnd(strm);
            }
        }
        self.zlib_init_done = false;
        self.mode = Mode::None;
        self.dictionary.clear();
    }
}

impl Drop for ZCtx {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(mode: Mode, level: c_int, input: &[u8], flush: c_int) -> Vec<u8> {
        let window = if mode == Mode::Unzip || mode == Mode::Inflate {
            0
        } else {
            15
        };
        let mut ctx = ZCtx::new(mode);
        ctx.init(level, window, 8, 0, false, Vec::new());
        let mut out = vec![0u8; 1 << 16];
        ctx.work(flush, input, &mut out);
        assert!(ctx.error_info().is_none());
        let used = out.len() - ctx.avail_out() as usize;
        out.truncate(used);
        out
    }

    #[test]
    fn deflate_empty_input_is_the_fixed_empty_block() {
        let out = run(Mode::Deflate, -1, b"", Z_FINISH);
        assert_eq!(out, [0x78, 0x9c, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01]);
    }

    #[test]
    fn stored_level_zero_wraps_bytes_verbatim() {
        let out = run(Mode::Deflate, 0, b"abc", Z_FINISH);
        assert_eq!(
            out,
            [
                0x78, 0x01, 0x01, 0x03, 0x00, 0xfc, 0xff, b'a', b'b', b'c', 0x02, 0x4d, 0x01, 0x27
            ]
        );
    }

    #[test]
    fn unzip_detects_zlib_and_gzip() {
        let zlib = run(Mode::Deflate, 6, b"hello hello hello", Z_FINISH);
        assert_eq!(run(Mode::Unzip, 0, &zlib, Z_FINISH), b"hello hello hello");
        let gzip = run(Mode::Gzip, 6, b"hello hello hello", Z_FINISH);
        assert_eq!(gzip[..2], [0x1f, 0x8b]);
        assert_eq!(run(Mode::Unzip, 0, &gzip, Z_FINISH), b"hello hello hello");
    }

    #[test]
    fn truncated_input_with_finish_is_an_error() {
        let zlib = run(Mode::Deflate, 6, b"hello hello hello", Z_FINISH);
        let mut ctx = ZCtx::new(Mode::Inflate);
        ctx.init(-1, 0, 8, 0, false, Vec::new());
        let mut out = vec![0u8; 64];
        ctx.work(Z_FINISH, &zlib[..zlib.len() - 3], &mut out);
        let err = ctx.error_info().expect("error");
        assert_eq!(
            (err.message.as_str(), &*err.code),
            ("unexpected end of file", "Z_BUF_ERROR")
        );
        assert_eq!(err.errno, -5);
    }

    #[test]
    fn corrupt_header_reports_zlibs_message() {
        let mut ctx = ZCtx::new(Mode::Inflate);
        ctx.init(-1, 0, 8, 0, false, Vec::new());
        let mut out = vec![0u8; 64];
        ctx.work(Z_FINISH, b"not zlib data", &mut out);
        let err = ctx.error_info().expect("error");
        assert_eq!(
            (err.message.as_str(), &*err.code),
            ("incorrect header check", "Z_DATA_ERROR")
        );
    }
}
