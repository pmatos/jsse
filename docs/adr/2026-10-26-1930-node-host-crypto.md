# Node host mode: the crypto module

Status: accepted (#1226), epic #1214. Extends `2026-10-26-0854-node-host-mode.md`.

**`crypto`** (#1226). `require("crypto")` / `require("node:crypto")` is
`node-lib/crypto.js` over a private native binding, `__host_crypto`
(`src/interpreter/builtins/node_host_crypto.rs`). The loader captures the
object, deletes the global and seeds it as the private module
`internal/host/crypto`, adding `randomBytes` (the prelude's
`__host_random_bytes`, kept as a global for the shim) and `webcrypto` (the
global `crypto` object of `scripts/node-crypto-shim.js`). Nothing here exists
without `--node`; the default global environment is unchanged.

## Decisions

1. **Native incremental hashing, not JS buffering.** `create(algo, outputLength,
   hmacKey)`, `update(handle, string | typed array)`, `copy(handle,
   outputLength)` and `digest(handle)` work on a handle table in
   `NodeHostState::crypto`; a string goes in as UTF-8 (lone surrogates become
   U+FFFD, as in Node), a typed array is read in place. `digestOnce`
   (`hash()`, `subtle.digest`) needs no handle. Buffer sources (ArrayBuffer,
   typed array, DataView) are read from the engine's own slots, never through
   properties a caller can shadow; a detached buffer reads as empty, as in
   Node. `digest` releases the handle; handles are never reused. XOF output
   is capped at 2^31-1 bytes and allocated fallibly (a `RangeError`, not an
   abort). A `Hash` dropped without `digest()` keeps
   its state until exit: the engine runs no `FinalizationRegistry` callbacks,
   so the module does not pretend to use one. Revisit if they ever run or an
   opaque-handle object kind appears.
2. **Dependencies.** RustCrypto `md-5`, `sha1`, `sha2`, `sha3` (all 0.10, one
   shared `digest` 0.10, MIT OR Apache-2.0, pure Rust). They are the
   algorithm implementations; each was cheaper and safer than writing the
   compression functions here. HMAC is 25 lines over the same hashers
   (RFC 2104; block size = sponge rate for SHA-3), so the `hmac` crate is not
   added. SHA-2 0.11 / SHA-3 0.12 were passed over: they move to `digest` 0.11
   and would duplicate the dependency tree.
3. **Algorithms.** md5, sha1, sha224, sha256, sha384, sha512, sha512-224,
   sha512-256, sha3-224/256/384/512, shake128, shake256 (with
   `outputLength`; default 16/32 bytes plus Node's DEP0198 warning). Names
   match OpenSSL's aliases case-insensitively through an explicit table
   (`RSA-SHA256`, `sha-256`, `sha256WithRSAEncryption`, `SHA-512/256`, ...),
   verified against Node. `getHashes()` lists only what is supported. Not
   provided: blake2, ripemd160, sm3, md5-sha1, and the `id-rsassa-...sha3`
   style aliases other than SHA-3's.
4. **Module surface.** `createHash`, `createHmac`, `hash`, `getHashes`,
   `timingSafeEqual` (native, no early exit), `randomBytes`, `randomFill`,
   `randomFillSync`, `randomInt`, `randomUUID`, `getRandomValues`, `Hash`,
   `Hmac`, `webcrypto`, `subtle`. `Hash`/`Hmac` keep Node's behaviour
   (`copy` on Hash only, `ERR_CRYPTO_HASH_FINALIZED`, Hmac `digest()` after
   finalisation returns an empty buffer, OpenSSL-shaped errors) but are plain
   objects: jsse has no `stream` module, so `write`/`pipe` are absent. HMAC
   keys are strings or buffer sources (no `KeyObject`). Not provided: ciphers,
   keys, sign/verify, DH/ECDH, pbkdf2/scrypt/hkdf, `randomUUIDv7`, `Certificate`.
5. **Random.** Entropy is `getrandom`: `randomFill` fills the target buffer in place natively, the rest uses `__host_random_bytes`.
   `randomBytes`/`randomFill` callbacks run from `setImmediate` (Node: after
   the threadpool job); `randomInt` uses a 6 KiB cache with rejection sampling
   and its callback runs from `process.nextTick`, as Node does on a warm cache.
6. **Web Crypto.** `require("crypto").webcrypto === globalThis.crypto`. The
   shim object keeps its own `getRandomValues`/`randomUUID` (their error
   shapes are the older ones); the loader gives it a lazy `subtle` accessor so
   `internal/crypto/webcrypto` is parsed on first use. `subtle.digest` takes
   SHA-1, SHA-256/384/512 and SHA3-256/384/512 (names case-insensitive,
   string or `{ name }`), snapshots the input synchronously and always
   returns a promise; every other `SubtleCrypto` method rejects with
   `NotSupportedError`.

## Verification

- `scripts/run-node-crypto-oracle.sh` diffs the 1,300-line corpus
  `scripts/crypto-fixtures/{hashes,random}.js` on jsse `--node` and Node: every
  algorithm over known strings, block-boundary lengths with every two-chunk
  split and byte-wise streaming, 1 MiB / 16 MiB generated inputs streamed in
  odd chunks and through offset views, `copy()`, all input and output
  encodings, HMAC keys around each block size as string/Buffer/ArrayBuffer/
  DataView/Uint16Array, `hash()`, `timingSafeEqual`, error codes and messages,
  random values by shape, `subtle.digest`. It takes about 20 s on jsse.
- `tests/node_crypto.rs` (known answers, streaming, errors, random, webcrypto;
  bytecode, `--no-bytecode` and `JSSE_GC_STRESS=3`; the default environment)
  and the unit tests in `node_host_crypto.rs` need no Node.
- Known divergence found on the way, outside this module: the prelude's
  `Buffer.from(str, "base64")` decodes strings with characters outside the
  alphabet differently from Node (#1328; the corpus skips that case).
