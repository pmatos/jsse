//! The `crypto` binding for Node host mode (issue #1226): a hidden `__host_crypto`
//! global that `node-builtin-loader.js` seeds as the private module
//! `internal/host/crypto`. Digest and HMAC state lives in `NodeHostState::crypto`
//! behind integer handles. See `docs/adr/2026-10-26-1930-node-host-crypto.md`.

use super::super::*;
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha224, Sha256, Sha384, Sha512, Sha512_224, Sha512_256};
use sha3::digest::{ExtendableOutput, Update, XofReader};
use sha3::{Sha3_224, Sha3_256, Sha3_384, Sha3_512};
use shake::{Shake128, Shake256};
use std::collections::HashMap;

fn fixed<D: Digest>(h: D, _out_len: usize) -> Result<Vec<u8>, ()> {
    Ok(h.finalize().to_vec())
}

fn xof<D: ExtendableOutput>(h: D, out_len: usize) -> Result<Vec<u8>, ()> {
    let mut out = Vec::new();
    out.try_reserve_exact(out_len).map_err(|_| ())?;
    out.resize(out_len, 0);
    h.finalize_xof().read(&mut out);
    Ok(out)
}

/// One row per algorithm: variant, hasher type, canonical name, default digest
/// size, HMAC block size (the sponge rate for SHA-3), finaliser and OpenSSL's
/// case-insensitive spellings (Node: `RSA-SHA256`, `SHA-256`,
/// `sha256WithRSAEncryption`, `SHA-512/256`, ...).
macro_rules! algorithms {
    ($($v:ident($t:ty) $name:literal $size:literal $block:literal $fin:ident [$($alias:literal),*];)*) => {
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Algo {
            $($v),*
        }

        #[derive(Clone)]
        enum Hasher {
            $($v($t)),*
        }

        impl Algo {
            fn parse(name: &str) -> Option<Algo> {
                const ALIASES: &[(&str, Algo)] = &[$($(($alias, Algo::$v)),*),*];
                let lower = name.to_ascii_lowercase();
                ALIASES.iter().find(|(n, _)| *n == lower).map(|(_, a)| *a)
            }

            fn name(self) -> &'static str {
                match self {
                    $(Algo::$v => $name),*
                }
            }

            /// The default (for the XOFs) output size in bytes.
            fn digest_size(self) -> usize {
                match self {
                    $(Algo::$v => $size),*
                }
            }

            fn block_size(self) -> usize {
                match self {
                    $(Algo::$v => $block),*
                }
            }

            fn is_xof(self) -> bool {
                match self {
                    $(Algo::$v => stringify!($fin) == "xof"),*
                }
            }
        }

        impl Hasher {
            fn new(algo: Algo) -> Hasher {
                match algo {
                    $(Algo::$v => Hasher::$v(<$t>::default())),*
                }
            }

            fn update(&mut self, data: &[u8]) {
                match self {
                    $(Hasher::$v(h) => Update::update(h, data)),*
                }
            }

            /// `out_len` only matters for the XOFs; `Err` is an allocation failure.
            fn finalize(self, out_len: usize) -> Result<Vec<u8>, ()> {
                match self {
                    $(Hasher::$v(h) => $fin(h, out_len)),*
                }
            }
        }
    };
}

algorithms! {
    Md5(Md5) "md5" 16 64 fixed ["md5", "rsa-md5", "md5withrsaencryption", "ssl3-md5"];
    Sha1(Sha1) "sha1" 20 64 fixed ["sha1", "sha-1", "rsa-sha1", "sha1withrsaencryption", "ssl3-sha1", "rsa-sha1-2"];
    Sha224(Sha224) "sha224" 28 64 fixed ["sha224", "sha-224", "sha2-224", "rsa-sha224", "sha224withrsaencryption"];
    Sha256(Sha256) "sha256" 32 64 fixed ["sha256", "sha-256", "sha2-256", "rsa-sha256", "sha256withrsaencryption"];
    Sha384(Sha384) "sha384" 48 128 fixed ["sha384", "sha-384", "sha2-384", "rsa-sha384", "sha384withrsaencryption"];
    Sha512(Sha512) "sha512" 64 128 fixed ["sha512", "sha-512", "sha2-512", "rsa-sha512", "sha512withrsaencryption"];
    Sha512_224(Sha512_224) "sha512-224" 28 128 fixed ["sha512-224", "sha512-224withrsaencryption", "sha-512/224", "sha2-512/224", "rsa-sha512/224"];
    Sha512_256(Sha512_256) "sha512-256" 32 128 fixed ["sha512-256", "sha512-256withrsaencryption", "sha-512/256", "sha2-512/256", "rsa-sha512/256"];
    Sha3_224(Sha3_224) "sha3-224" 28 144 fixed ["sha3-224", "rsa-sha3-224", "id-rsassa-pkcs1-v1_5-with-sha3-224"];
    Sha3_256(Sha3_256) "sha3-256" 32 136 fixed ["sha3-256", "rsa-sha3-256", "id-rsassa-pkcs1-v1_5-with-sha3-256"];
    Sha3_384(Sha3_384) "sha3-384" 48 104 fixed ["sha3-384", "rsa-sha3-384", "id-rsassa-pkcs1-v1_5-with-sha3-384"];
    Sha3_512(Sha3_512) "sha3-512" 64 72 fixed ["sha3-512", "rsa-sha3-512", "id-rsassa-pkcs1-v1_5-with-sha3-512"];
    Shake128(Shake128) "shake128" 16 168 xof ["shake128", "shake-128"];
    Shake256(Shake256) "shake256" 32 136 xof ["shake256", "shake-256"];
}

const MAX_OUTPUT: usize = i32::MAX as usize;

struct Entry {
    algo: Algo,
    hasher: Hasher,
    out_len: usize,
    /// For HMAC: the outer-pad key block, hashed over the inner digest.
    outer_key: Option<Vec<u8>>,
}

impl Entry {
    fn finish(self) -> Result<Vec<u8>, ()> {
        let inner = self.hasher.finalize(self.out_len)?;
        let Some(key) = self.outer_key else {
            return Ok(inner);
        };
        let mut outer = Hasher::new(self.algo);
        outer.update(&key);
        outer.update(&inner);
        outer.finalize(self.out_len)
    }
}

fn hmac_entry(algo: Algo, key: &[u8], out_len: usize) -> Result<Entry, ()> {
    let block = algo.block_size();
    let mut key_block = if key.len() > block {
        let mut h = Hasher::new(algo);
        h.update(key);
        h.finalize(algo.digest_size())?
    } else {
        key.to_vec()
    };
    key_block.resize(block, 0);
    let mut hasher = Hasher::new(algo);
    hasher.update(&key_block.iter().map(|b| b ^ 0x36).collect::<Vec<_>>());
    Ok(Entry {
        algo,
        hasher,
        out_len,
        outer_key: Some(key_block.iter().map(|b| b ^ 0x5c).collect()),
    })
}

#[derive(Default)]
pub(crate) struct CryptoState {
    next: u64,
    entries: HashMap<u64, Entry>,
}

impl CryptoState {
    fn insert(&mut self, entry: Entry) -> f64 {
        self.next += 1;
        self.entries.insert(self.next, entry);
        self.next as f64
    }
}

type Native = fn(&mut Interpreter, &JsValue, &[JsValue]) -> Completion;

fn arg(args: &[JsValue], i: usize) -> &JsValue {
    args.get(i).unwrap_or(JsValue::undefined_ref())
}

fn js_str(s: &str) -> JsValue {
    JsValue::string(JsString::from_str(s))
}

fn handle(interp: &mut Interpreter, v: &JsValue) -> Result<u64, JsValue> {
    interp.to_number_value(v).map(|n| n as u64)
}

fn stale(interp: &mut Interpreter) -> Completion {
    Completion::Throw(interp.create_type_error("crypto: invalid or finished handle"))
}

macro_rules! tri {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(e) => return Completion::Throw(e),
        }
    };
}

/// The bytes behind an ArrayBuffer, typed array or DataView, read through the
/// engine's own slots (never through properties the caller can shadow). A
/// detached buffer is empty, as it is for Node.
pub(super) struct Window {
    pub(super) buffer: std::rc::Rc<std::cell::RefCell<BufferData>>,
    pub(super) offset: usize,
    pub(super) len: usize,
}

fn data_view_byte_length(dv: &DataViewInfo) -> usize {
    let buf_len = dv.buffer.borrow().len();
    if dv.is_length_tracking {
        buf_len.saturating_sub(dv.byte_offset)
    } else if dv.byte_offset + dv.byte_length > buf_len {
        0
    } else {
        dv.byte_length
    }
}

pub(super) fn window(interp: &Interpreter, v: &JsValue) -> Option<Window> {
    let cell = interp.get_object_cell(v.as_object_id()?)?;
    let obj = cell.borrow();
    let (buffer, offset, len, detached) = match &obj.kind {
        ObjectKind::TypedArray(ta) => (
            ta.buffer.clone(),
            ta.byte_offset,
            typed_array_byte_length(ta),
            ta.is_detached.get(),
        ),
        ObjectKind::DataView(dv) => (
            dv.buffer.clone(),
            dv.byte_offset,
            data_view_byte_length(dv),
            dv.is_detached.get(),
        ),
        ObjectKind::ArrayBuffer(ab) => {
            let len = ab.data.borrow().len();
            let detached = ab.detached.as_ref().is_some_and(|d| d.get());
            (ab.data.clone(), 0, len, detached)
        }
        _ => return None,
    };
    Some(if detached {
        Window {
            buffer,
            offset: 0,
            len: 0,
        }
    } else {
        Window {
            buffer,
            offset,
            len,
        }
    })
}

enum Source {
    Text(String),
    Bytes(Window),
}

impl Source {
    fn with<R>(&self, f: impl FnOnce(&[u8]) -> R) -> Option<R> {
        match self {
            Source::Text(s) => Some(f(s.as_bytes())),
            Source::Bytes(w) => w
                .buffer
                .borrow()
                .with_read(|b| b.get(w.offset..w.offset + w.len).map(f)),
        }
    }
}

/// A string (as UTF-8, lone surrogates replaced) or a byte window.
fn source(interp: &mut Interpreter, v: &JsValue) -> Result<Source, JsValue> {
    if v.is_string() {
        return Ok(Source::Text(interp.to_string_lossy(v)?));
    }
    window(interp, v)
        .map(Source::Bytes)
        .ok_or_else(|| interp.create_type_error("crypto: expected a string or buffer source"))
}

fn with_bytes<R>(
    interp: &mut Interpreter,
    v: &JsValue,
    f: impl FnOnce(&[u8]) -> R,
) -> Result<R, JsValue> {
    let src = source(interp, v)?;
    src.with(f)
        .ok_or_else(|| interp.create_type_error("crypto: out-of-range view"))
}

fn bytes_value(interp: &mut Interpreter, bytes: Result<Vec<u8>, ()>) -> Completion {
    match bytes {
        Ok(bytes) => super::typedarray::create_uint8array_from_bytes(interp, &bytes),
        Err(()) => Completion::Throw(interp.create_range_error("crypto: output allocation failed")),
    }
}

fn out_len_arg(interp: &mut Interpreter, v: &JsValue, default: usize) -> Result<usize, JsValue> {
    match v.as_number() {
        Some(n) if n >= 0.0 && n <= MAX_OUTPUT as f64 => Ok(n as usize),
        Some(_) => Err(interp.create_range_error("crypto: invalid output length")),
        None => Ok(default),
    }
}

/// (name) -> [canonicalName, digestSize, isXof] | undefined
fn info(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let name = tri!(interp.to_string_lossy(arg(args, 0)));
    Completion::Normal(match Algo::parse(&name) {
        None => JsValue::UNDEFINED,
        Some(a) => interp.create_array(vec![
            js_str(a.name()),
            JsValue::number(a.digest_size() as f64),
            JsValue::boolean(a.is_xof()),
        ]),
    })
}

fn algo_arg(interp: &mut Interpreter, v: &JsValue) -> Result<Algo, JsValue> {
    let name = interp.to_string_lossy(v)?;
    Algo::parse(&name).ok_or_else(|| interp.create_type_error("crypto: unknown algorithm"))
}

/// (canonicalName, outputLength | undefined, hmacKey | undefined) -> handle
fn create(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let algo = tri!(algo_arg(interp, arg(args, 0)));
    let out_len = tri!(out_len_arg(interp, arg(args, 1), algo.digest_size()));
    let entry = if arg(args, 2).is_undefined() {
        Entry {
            algo,
            hasher: Hasher::new(algo),
            out_len,
            outer_key: None,
        }
    } else {
        let built = tri!(with_bytes(interp, arg(args, 2), |k| hmac_entry(
            algo, k, out_len
        )));
        tri!(built.map_err(|()| interp.create_range_error("crypto: output allocation failed")))
    };
    Completion::Normal(JsValue::number(interp.node_host.crypto.insert(entry)))
}

/// (canonicalName, outputLength | undefined, data) -> Uint8Array, with no handle.
fn digest_once(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let algo = tri!(algo_arg(interp, arg(args, 0)));
    let out_len = tri!(out_len_arg(interp, arg(args, 1), algo.digest_size()));
    let mut hasher = Hasher::new(algo);
    tri!(with_bytes(interp, arg(args, 2), |b| hasher.update(b)));
    bytes_value(interp, hasher.finalize(out_len))
}

fn update(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let id = tri!(handle(interp, arg(args, 0)));
    let src = tri!(source(interp, arg(args, 1)));
    let Some(entry) = interp.node_host.crypto.entries.get_mut(&id) else {
        return stale(interp);
    };
    if src.with(|b| entry.hasher.update(b)).is_none() {
        return Completion::Throw(interp.create_type_error("crypto: out-of-range view"));
    }
    Completion::Normal(JsValue::UNDEFINED)
}

fn copy(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let id = tri!(handle(interp, arg(args, 0)));
    let Some(src) = interp.node_host.crypto.entries.get(&id) else {
        return stale(interp);
    };
    let (algo, hasher, current, outer_key) = (
        src.algo,
        src.hasher.clone(),
        src.out_len,
        src.outer_key.clone(),
    );
    let out_len = tri!(out_len_arg(interp, arg(args, 1), current));
    let entry = Entry {
        algo,
        hasher,
        out_len,
        outer_key,
    };
    Completion::Normal(JsValue::number(interp.node_host.crypto.insert(entry)))
}

fn digest(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let id = tri!(handle(interp, arg(args, 0)));
    match interp.node_host.crypto.entries.remove(&id) {
        Some(entry) => bytes_value(interp, entry.finish()),
        None => stale(interp),
    }
}

/// (a, b) -> boolean | null; null when the byte lengths differ.
fn timing_safe_equal(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let a = tri!(with_bytes(interp, arg(args, 0), <[u8]>::to_vec));
    let equal = tri!(with_bytes(interp, arg(args, 1), |b| {
        (a.len() == b.len()).then(|| {
            let diff = a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y));
            std::hint::black_box(diff) == 0
        })
    }));
    Completion::Normal(equal.map_or(JsValue::NULL, JsValue::boolean))
}

/// (target, offset, size): fill that byte range of an ArrayBuffer or view in place.
fn random_fill(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let Some(w) = window(interp, arg(args, 0)) else {
        return Completion::Throw(interp.create_type_error("crypto: expected a buffer source"));
    };
    let offset = tri!(out_len_arg(interp, arg(args, 1), 0));
    let size = tri!(out_len_arg(
        interp,
        arg(args, 2),
        w.len.saturating_sub(offset)
    ));
    if offset + size > w.len {
        return Completion::Throw(interp.create_range_error("crypto: fill range exceeds buffer"));
    }
    let start = w.offset + offset;
    let filled = w.buffer.borrow_mut().with_write(|b| {
        b.get_mut(start..start + size)
            .is_some_and(|window| getrandom::fill(window).is_ok())
    });
    if !filled {
        return Completion::Throw(
            interp.create_error("Error", "crypto: OS entropy source unavailable"),
        );
    }
    Completion::Normal(JsValue::UNDEFINED)
}

impl Interpreter {
    pub(super) fn setup_node_host_crypto(&mut self) {
        let binding = self.create_object_id();
        let functions: [(&str, usize, Native); 8] = [
            ("info", 1, info),
            ("create", 3, create),
            ("digestOnce", 3, digest_once),
            ("update", 2, update),
            ("copy", 2, copy),
            ("digest", 1, digest),
            ("timingSafeEqual", 2, timing_safe_equal),
            ("randomFill", 3, random_fill),
        ];
        for (name, arity, native) in functions {
            let f = self.create_function(JsFunction::native(String::new(), arity, native));
            self.get_object_cell_expect(binding)
                .borrow_mut()
                .insert_value(name, f);
        }
        self.install_host_global("__host_crypto", JsValue::object(binding));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    fn hash(name: &str, data: &[u8], out_len: usize) -> String {
        let algo = Algo::parse(name).expect("algo");
        let mut h = Hasher::new(algo);
        h.update(data);
        hex(&h.finalize(out_len).expect("digest"))
    }

    fn hmac(name: &str, key: &[u8], data: &[u8]) -> String {
        let algo = Algo::parse(name).expect("algo");
        let mut e = hmac_entry(algo, key, algo.digest_size()).expect("key");
        e.hasher.update(data);
        hex(&e.finish().expect("digest"))
    }

    #[test]
    fn aliases_resolve_to_canonical_names() {
        for (alias, want) in [
            ("SHA256", "sha256"),
            ("sha-256", "sha256"),
            ("SHA2-256", "sha256"),
            ("RSA-SHA256", "sha256"),
            ("sha256WithRSAEncryption", "sha256"),
            ("SHA-512/256", "sha512-256"),
            ("sha2-512/224", "sha512-224"),
            ("ssl3-md5", "md5"),
            ("SHA3-256", "sha3-256"),
            ("Shake128", "shake128"),
        ] {
            assert_eq!(Algo::parse(alias).map(Algo::name), Some(want), "{alias}");
        }
        assert!(Algo::parse("blake2b512").is_none());
        assert!(Algo::parse("sha-").is_none());
        assert!(Algo::parse("sha512/256").is_none());
        assert!(Algo::parse("sha-3-256").is_none());
        assert!(Algo::parse("sha256 ").is_none());
        assert!(Algo::parse("").is_none());
    }

    #[test]
    fn known_digests() {
        assert_eq!(hash("md5", b"abc", 0), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            hash("sha1", b"abc", 0),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hash("sha256", b"", 0),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hash("sha3-256", b"abc", 0),
            "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"
        );
        assert_eq!(hash("shake128", b"", 5), "7f9c2ba4e8");
    }

    #[test]
    fn hmac_matches_rfc_4231_case_2() {
        assert_eq!(
            hmac("sha256", b"Jefe", b"what do ya want for nothing?"),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn hmac_key_longer_than_block_is_hashed_first() {
        let long = [0xaau8; 131];
        assert_eq!(
            hmac(
                "sha256",
                &long,
                b"Test Using Larger Than Block-Size Key - Hash Key First"
            ),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }
}
