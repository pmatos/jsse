//! The `crypto` binding for Node host mode (issue #1226): one hidden global,
//! `__host_crypto`, with incremental digests and HMAC behind integer handles
//! and a constant-time compare. `node-builtin-loader.js` captures the object,
//! deletes the global and seeds it as the private module `internal/host/crypto`;
//! `node-lib/internal/crypto/hash.js` is the only consumer. State lives in
//! `NodeHostState::crypto`, so nothing here exists without `--node`. `digest`
//! releases a handle; one dropped without `digest` stays until exit (the engine
//! runs no FinalizationRegistry callbacks). See `docs/adr/2026-10-26-1930-node-host-crypto.md`.

use super::super::*;
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha224, Sha256, Sha384, Sha512, Sha512_224, Sha512_256};
use sha3::digest::{ExtendableOutput, Update, XofReader};
use sha3::{Sha3_224, Sha3_256, Sha3_384, Sha3_512, Shake128, Shake256};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Algo {
    Md5,
    Sha1,
    Sha224,
    Sha256,
    Sha384,
    Sha512,
    Sha512_224,
    Sha512_256,
    Sha3_224,
    Sha3_256,
    Sha3_384,
    Sha3_512,
    Shake128,
    Shake256,
}

impl Algo {
    /// OpenSSL's spellings, matched case-insensitively (Node: `RSA-SHA256`,
    /// `SHA-256`, `sha256WithRSAEncryption`, `SHA-512/256`, ...).
    fn parse(name: &str) -> Option<Algo> {
        const ALIASES: &[(&str, Algo)] = &[
            ("md5", Algo::Md5),
            ("rsa-md5", Algo::Md5),
            ("md5withrsaencryption", Algo::Md5),
            ("ssl3-md5", Algo::Md5),
            ("sha1", Algo::Sha1),
            ("sha-1", Algo::Sha1),
            ("rsa-sha1", Algo::Sha1),
            ("sha1withrsaencryption", Algo::Sha1),
            ("ssl3-sha1", Algo::Sha1),
            ("rsa-sha1-2", Algo::Sha1),
            ("sha224", Algo::Sha224),
            ("sha-224", Algo::Sha224),
            ("sha2-224", Algo::Sha224),
            ("rsa-sha224", Algo::Sha224),
            ("sha224withrsaencryption", Algo::Sha224),
            ("sha256", Algo::Sha256),
            ("sha-256", Algo::Sha256),
            ("sha2-256", Algo::Sha256),
            ("rsa-sha256", Algo::Sha256),
            ("sha256withrsaencryption", Algo::Sha256),
            ("sha384", Algo::Sha384),
            ("sha-384", Algo::Sha384),
            ("sha2-384", Algo::Sha384),
            ("rsa-sha384", Algo::Sha384),
            ("sha384withrsaencryption", Algo::Sha384),
            ("sha512", Algo::Sha512),
            ("sha-512", Algo::Sha512),
            ("sha2-512", Algo::Sha512),
            ("rsa-sha512", Algo::Sha512),
            ("sha512withrsaencryption", Algo::Sha512),
            ("sha512-224", Algo::Sha512_224),
            ("sha512-224withrsaencryption", Algo::Sha512_224),
            ("sha-512/224", Algo::Sha512_224),
            ("sha2-512/224", Algo::Sha512_224),
            ("rsa-sha512/224", Algo::Sha512_224),
            ("sha512-256", Algo::Sha512_256),
            ("sha512-256withrsaencryption", Algo::Sha512_256),
            ("sha-512/256", Algo::Sha512_256),
            ("sha2-512/256", Algo::Sha512_256),
            ("rsa-sha512/256", Algo::Sha512_256),
            ("sha3-224", Algo::Sha3_224),
            ("rsa-sha3-224", Algo::Sha3_224),
            ("id-rsassa-pkcs1-v1_5-with-sha3-224", Algo::Sha3_224),
            ("sha3-256", Algo::Sha3_256),
            ("rsa-sha3-256", Algo::Sha3_256),
            ("id-rsassa-pkcs1-v1_5-with-sha3-256", Algo::Sha3_256),
            ("sha3-384", Algo::Sha3_384),
            ("rsa-sha3-384", Algo::Sha3_384),
            ("id-rsassa-pkcs1-v1_5-with-sha3-384", Algo::Sha3_384),
            ("sha3-512", Algo::Sha3_512),
            ("rsa-sha3-512", Algo::Sha3_512),
            ("id-rsassa-pkcs1-v1_5-with-sha3-512", Algo::Sha3_512),
            ("shake128", Algo::Shake128),
            ("shake-128", Algo::Shake128),
            ("shake256", Algo::Shake256),
            ("shake-256", Algo::Shake256),
        ];
        let lower = name.to_ascii_lowercase();
        ALIASES.iter().find(|(n, _)| *n == lower).map(|(_, a)| *a)
    }

    fn name(self) -> &'static str {
        match self {
            Algo::Md5 => "md5",
            Algo::Sha1 => "sha1",
            Algo::Sha224 => "sha224",
            Algo::Sha256 => "sha256",
            Algo::Sha384 => "sha384",
            Algo::Sha512 => "sha512",
            Algo::Sha512_224 => "sha512-224",
            Algo::Sha512_256 => "sha512-256",
            Algo::Sha3_224 => "sha3-224",
            Algo::Sha3_256 => "sha3-256",
            Algo::Sha3_384 => "sha3-384",
            Algo::Sha3_512 => "sha3-512",
            Algo::Shake128 => "shake128",
            Algo::Shake256 => "shake256",
        }
    }

    /// The default (for the XOFs) output size in bytes.
    fn digest_size(self) -> usize {
        match self {
            Algo::Md5 | Algo::Shake128 => 16,
            Algo::Sha1 => 20,
            Algo::Sha224 | Algo::Sha512_224 | Algo::Sha3_224 => 28,
            Algo::Sha256 | Algo::Sha512_256 | Algo::Sha3_256 | Algo::Shake256 => 32,
            Algo::Sha384 | Algo::Sha3_384 => 48,
            Algo::Sha512 | Algo::Sha3_512 => 64,
        }
    }

    /// The HMAC block size; for SHA-3 and SHAKE it is the sponge rate.
    fn block_size(self) -> usize {
        match self {
            Algo::Md5 | Algo::Sha1 | Algo::Sha224 | Algo::Sha256 => 64,
            Algo::Sha384 | Algo::Sha512 | Algo::Sha512_224 | Algo::Sha512_256 => 128,
            Algo::Sha3_224 => 144,
            Algo::Sha3_256 => 136,
            Algo::Sha3_384 => 104,
            Algo::Sha3_512 => 72,
            Algo::Shake128 => 168,
            Algo::Shake256 => 136,
        }
    }

    fn is_xof(self) -> bool {
        matches!(self, Algo::Shake128 | Algo::Shake256)
    }
}

#[derive(Clone)]
enum Hasher {
    Md5(Md5),
    Sha1(Sha1),
    Sha224(Sha224),
    Sha256(Sha256),
    Sha384(Sha384),
    Sha512(Sha512),
    Sha512_224(Sha512_224),
    Sha512_256(Sha512_256),
    Sha3_224(Sha3_224),
    Sha3_256(Sha3_256),
    Sha3_384(Sha3_384),
    Sha3_512(Sha3_512),
    Shake128(Shake128),
    Shake256(Shake256),
}

impl Hasher {
    fn new(algo: Algo) -> Hasher {
        match algo {
            Algo::Md5 => Hasher::Md5(Md5::new()),
            Algo::Sha1 => Hasher::Sha1(Sha1::new()),
            Algo::Sha224 => Hasher::Sha224(Sha224::new()),
            Algo::Sha256 => Hasher::Sha256(Sha256::new()),
            Algo::Sha384 => Hasher::Sha384(Sha384::new()),
            Algo::Sha512 => Hasher::Sha512(Sha512::new()),
            Algo::Sha512_224 => Hasher::Sha512_224(Sha512_224::new()),
            Algo::Sha512_256 => Hasher::Sha512_256(Sha512_256::new()),
            Algo::Sha3_224 => Hasher::Sha3_224(Sha3_224::new()),
            Algo::Sha3_256 => Hasher::Sha3_256(Sha3_256::new()),
            Algo::Sha3_384 => Hasher::Sha3_384(Sha3_384::new()),
            Algo::Sha3_512 => Hasher::Sha3_512(Sha3_512::new()),
            Algo::Shake128 => Hasher::Shake128(Shake128::default()),
            Algo::Shake256 => Hasher::Shake256(Shake256::default()),
        }
    }

    fn update(&mut self, data: &[u8]) {
        match self {
            Hasher::Md5(h) => Digest::update(h, data),
            Hasher::Sha1(h) => Digest::update(h, data),
            Hasher::Sha224(h) => Digest::update(h, data),
            Hasher::Sha256(h) => Digest::update(h, data),
            Hasher::Sha384(h) => Digest::update(h, data),
            Hasher::Sha512(h) => Digest::update(h, data),
            Hasher::Sha512_224(h) => Digest::update(h, data),
            Hasher::Sha512_256(h) => Digest::update(h, data),
            Hasher::Sha3_224(h) => Digest::update(h, data),
            Hasher::Sha3_256(h) => Digest::update(h, data),
            Hasher::Sha3_384(h) => Digest::update(h, data),
            Hasher::Sha3_512(h) => Digest::update(h, data),
            Hasher::Shake128(h) => Update::update(h, data),
            Hasher::Shake256(h) => Update::update(h, data),
        }
    }

    /// `out_len` only matters for the XOFs.
    fn finalize(self, out_len: usize) -> Vec<u8> {
        match self {
            Hasher::Md5(h) => h.finalize().to_vec(),
            Hasher::Sha1(h) => h.finalize().to_vec(),
            Hasher::Sha224(h) => h.finalize().to_vec(),
            Hasher::Sha256(h) => h.finalize().to_vec(),
            Hasher::Sha384(h) => h.finalize().to_vec(),
            Hasher::Sha512(h) => h.finalize().to_vec(),
            Hasher::Sha512_224(h) => h.finalize().to_vec(),
            Hasher::Sha512_256(h) => h.finalize().to_vec(),
            Hasher::Sha3_224(h) => h.finalize().to_vec(),
            Hasher::Sha3_256(h) => h.finalize().to_vec(),
            Hasher::Sha3_384(h) => h.finalize().to_vec(),
            Hasher::Sha3_512(h) => h.finalize().to_vec(),
            Hasher::Shake128(h) => squeeze(h, out_len),
            Hasher::Shake256(h) => squeeze(h, out_len),
        }
    }
}

fn squeeze(h: impl ExtendableOutput, out_len: usize) -> Vec<u8> {
    let mut out = vec![0u8; out_len];
    h.finalize_xof().read(&mut out);
    out
}

struct Entry {
    algo: Algo,
    hasher: Hasher,
    out_len: usize,
    /// For HMAC: the outer-pad key block, hashed over the inner digest.
    outer_key: Option<Vec<u8>>,
}

impl Entry {
    fn finish(self) -> Vec<u8> {
        let inner = self.hasher.finalize(self.out_len);
        let Some(key) = self.outer_key else {
            return inner;
        };
        let mut outer = Hasher::new(self.algo);
        outer.update(&key);
        outer.update(&inner);
        outer.finalize(self.out_len)
    }
}

fn hmac_entry(algo: Algo, key: &[u8]) -> Entry {
    let block = algo.block_size();
    let mut key_block = if key.len() > block {
        let mut h = Hasher::new(algo);
        h.update(key);
        h.finalize(algo.digest_size())
    } else {
        key.to_vec()
    };
    key_block.resize(block, 0);
    let mut hasher = Hasher::new(algo);
    hasher.update(&key_block.iter().map(|b| b ^ 0x36).collect::<Vec<_>>());
    Entry {
        algo,
        hasher,
        out_len: algo.digest_size(),
        outer_key: Some(key_block.iter().map(|b| b ^ 0x5c).collect()),
    }
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

/// Run `f` over the bytes of a string (as UTF-8, lone surrogates replaced) or a
/// typed array / DataView window, without copying the latter.
fn with_bytes<R>(
    interp: &mut Interpreter,
    v: &JsValue,
    f: impl FnOnce(&[u8]) -> R,
) -> Result<R, JsValue> {
    if v.is_string() {
        let s = interp.to_string_lossy(v)?;
        return Ok(f(s.as_bytes()));
    }
    let view = v
        .as_object_id()
        .and_then(|id| interp.get_object_cell(id))
        .and_then(|cell| {
            let obj = cell.borrow();
            let ta = obj.typed_array_info()?;
            (!ta.is_detached.get()).then(|| (ta.buffer.clone(), ta.byte_offset, ta.byte_length))
        });
    let Some((buffer, offset, len)) = view else {
        return Err(interp.create_type_error("crypto: expected a string or Uint8Array"));
    };
    buffer.borrow().with_read(|b| {
        b.get(offset..offset + len)
            .map(f)
            .ok_or_else(|| interp.create_type_error("crypto: detached or out-of-range view"))
    })
}

fn bytes_value(interp: &mut Interpreter, bytes: Vec<u8>) -> Completion {
    super::typedarray::create_uint8array_from_bytes(interp, &bytes)
}

macro_rules! tri {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(e) => return Completion::Throw(e),
        }
    };
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

/// (canonicalName, outputLength | undefined, hmacKey | undefined) -> handle
fn create(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let name = tri!(interp.to_string_lossy(arg(args, 0)));
    let Some(algo) = Algo::parse(&name) else {
        return Completion::Throw(interp.create_type_error("crypto: unknown algorithm"));
    };
    let out_len = match arg(args, 1).as_number() {
        Some(n) => n as usize,
        None => algo.digest_size(),
    };
    let entry = if arg(args, 2).is_undefined() {
        Entry {
            algo,
            hasher: Hasher::new(algo),
            out_len,
            outer_key: None,
        }
    } else {
        let mut e = tri!(with_bytes(interp, arg(args, 2), |k| hmac_entry(algo, k)));
        e.out_len = out_len;
        e
    };
    Completion::Normal(JsValue::number(interp.node_host.crypto.insert(entry)))
}

fn update(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let id = tri!(handle(interp, arg(args, 0)));
    let Some(mut entry) = interp.node_host.crypto.entries.remove(&id) else {
        return stale(interp);
    };
    let result = with_bytes(interp, arg(args, 1), |b| entry.hasher.update(b));
    interp.node_host.crypto.entries.insert(id, entry);
    tri!(result);
    Completion::Normal(JsValue::UNDEFINED)
}

fn copy(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let id = tri!(handle(interp, arg(args, 0)));
    let Some(src) = interp.node_host.crypto.entries.get(&id) else {
        return stale(interp);
    };
    let entry = Entry {
        algo: src.algo,
        hasher: src.hasher.clone(),
        out_len: match arg(args, 1).as_number() {
            Some(n) => n as usize,
            None => src.out_len,
        },
        outer_key: src.outer_key.clone(),
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

/// (a, b) -> boolean; the views must have the same length.
fn timing_safe_equal(interp: &mut Interpreter, _t: &JsValue, args: &[JsValue]) -> Completion {
    let a = tri!(with_bytes(interp, arg(args, 0), <[u8]>::to_vec));
    let equal = tri!(with_bytes(interp, arg(args, 1), |b| {
        let diff = a
            .iter()
            .zip(b)
            .fold(a.len() ^ b.len(), |acc, (x, y)| acc | usize::from(x ^ y));
        std::hint::black_box(diff) == 0
    }));
    Completion::Normal(JsValue::boolean(equal))
}

impl Interpreter {
    pub(super) fn setup_node_host_crypto(&mut self) {
        let binding = self.create_object_id();
        let functions: [(&str, usize, Native); 6] = [
            ("info", 1, info),
            ("create", 3, create),
            ("update", 2, update),
            ("copy", 2, copy),
            ("digest", 1, digest),
            ("timingSafeEqual", 2, timing_safe_equal),
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
        hex(&h.finalize(out_len))
    }

    fn hmac(name: &str, key: &[u8], data: &[u8]) -> String {
        let mut e = hmac_entry(Algo::parse(name).expect("algo"), key);
        e.hasher.update(data);
        hex(&e.finish())
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
