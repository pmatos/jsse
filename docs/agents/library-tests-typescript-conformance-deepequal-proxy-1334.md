# Library Tests: deep equality over Proxies (#1334)

Part of epic #1214. Under `--node`, `util.isDeepStrictEqual`, `assert.deepEqual` and
`assert.deepStrictEqual` read an object's own non-index keys through
`internalBinding('util').getOwnNonIndexProperties` (`node-lib/internal/binding/util.js`).
Node implements that with V8 `GetPropertyNames(kOwnOnly, filter, kSkipIndices)`, which for a
Proxy runs the `ownKeys` trap (with its invariant checks) and, for `ONLY_ENUMERABLE`, the
`getOwnPropertyDescriptor` trap per remaining key. The `__host_util_own_non_index` primitive
is trap-free (it reads the property map), so a Proxy array whose `ownKeys` trap omitted
`length` compared equal instead of throwing.

- **Fix**: `getOwnNonIndexProperties` detects a Proxy (`host.proxyDetails`) and takes a
  `Reflect.ownKeys` / `Reflect.getOwnPropertyDescriptor` path that mirrors V8: trap order is
  kept, `SKIP_SYMBOLS` drops symbols before any descriptor trap, array indices (canonical
  `0..2^32-2`; `'4294967295'` and `'01'` are not) are skipped before any descriptor trap, and a
  revoked Proxy throws a `TypeError`. Non-Proxy objects keep the native fast path.
- **Guard**: `tests/node_deep_equal_proxy.rs` runs `scripts/util-fixtures/proxy-own-keys.js`
  on jsse `--node` (bytecode and `--no-bytecode`) and compares it with
  `proxy-own-keys.expected`, Node v26.8.2's output for the same script (regenerate with
  `node scripts/util-fixtures/proxy-own-keys.js`).
- **assert fixture**: the `test-assert-deep` "Check proxies" skip is replaced by a text patch
  in `scripts/assert-fixtures/manifest.txt`; the test now runs on jsse. Only the engine's
  `ownKeys` invariant message differs from V8's ("did not include all non-configurable own
  keys of the proxy target" versus "did not include 'length'"), which is engine-level text.
