# Node-compat library-test harness

Run real-world npm libraries' own test suites on jsse as engine stress tests
(part of the Node host-compat epic). The recipe generalizes the original acorn
harness: clone a pinned library, bundle its test entry with esbuild into a
single IIFE, prepend a Node-globals shim, and run it on `target/release/jsse`.

The Node/host surface ships **only** as prepended JS shims
(`scripts/node-shim.js` for `process`/`console`, `scripts/node-buffer-shim.js`
for `Buffer`/`TextEncoder`/`TextDecoder`) — never added to jsse's default global
object, so test262 is unaffected. Both are prepended to every bundle.
Everything here lives under `scripts/`; no `src/` change is required to add a
library.

## Buffer / TextEncoder / TextDecoder (`node-buffer-shim.js`)

`Buffer` is the highest-value host object: many libraries reference it (or
`TextEncoder`) at import time and fail to load without it. The shim implements
it in pure JS as a subclass of `Uint8Array` riding jsse's existing
`TypedArray`/`ArrayBuffer`/`DataView` — so it needs **zero new engine object
kinds**, and `instanceof Uint8Array` holds. It covers `Buffer.from`/`alloc`/
`allocUnsafe`/`concat`/`isBuffer`/`byteLength`/`isEncoding`/`compare`; the
`utf8`/`hex`/`base64`/`base64url`/`latin1`/`ascii`/`ucs2` encodings; fixed- and
variable-width `read*`/`write*` (LE/BE, including BigInt64); shared-memory
`slice`/`subarray`; and `equals`/`compare`/`indexOf`/`copy`/`fill`/`write`/
`toJSON`. `TextEncoder`/`TextDecoder` cover UTF-8 (surrogate handling, `fatal`,
BOM). Like `node-shim.js`, every global is guarded so the shim is inert on Node.

### Shim fixtures (`run-shim-fixtures.sh`)

`scripts/shim-fixtures/*.fixture.js` are self-verifying tests for the shims: each
assertion checks a value captured from Node's native `Buffer`/`TextEncoder`.

```sh
./scripts/run-shim-fixtures.sh [--node] [--no-cross-check]
```

The runner prepends both shims to each fixture and runs it on **jsse** (shims
active) and **Node** (shims inert → native APIs). Passing on Node proves the
asserted values are correct; passing on jsse proves the shim matches Node. Both
engines must report the same assertion count, so a fixture cannot silently skip
checks on jsse.

The shim's readable-output layer (`process`, the full `console` method set, and
the `util.format` / `util.inspect` core they share) is built on top of the
flag-gated Rust "syscall floor" (issue #229): `__host_write` (byte-accurate fd
I/O), `__host_hrtime` (monotonic clock), and `__host_exit` (real process exit).
The runner therefore invokes **jsse with `--node`** so those primitives exist
(never Node — it has no such flag and doesn't need one). The shim is guarded to
be a complete no-op on real Node, where `process`, the full `console`, and
`require('util')` already exist; that inertness is what lets the identical bundle
run on Node as the reference oracle. When the floor is absent (jsse without
`--node`) each surface degrades to a pure-JS fallback.

## Running

```sh
./scripts/run-library-tests.sh <lib> [--clean] [--node] [--no-cross-check]
```

- `<lib>` — a config name under `scripts/libs/` (e.g. `decimal.js`, `big.js`, `acorn`).
- `--clean` — wipe this library's cache (`/tmp/jsse-libtests/<lib>/`) and rebuild.
- `--node` — run the identical bundle on Node only (reference oracle / debugging).
- `--no-cross-check` — run on jsse only, skip the Node count comparison.

By default the runner runs the bundle on **jsse and Node** and requires:

1. the library's own verdict passes on jsse, **and**
2. jsse's reported test count equals Node's.

Step 2 closes a false-pass hole: a suite that self-reports "X of X passed" on
jsse alone can't distinguish "all passed" from "jsse silently ran fewer tests".
Node is the reference count. (The final "In total…" line comes from the
runners' `console.log`, so the verdict is robust regardless of how
`process.stdout.write` is shimmed.)

## Adding a library

Create `scripts/libs/<lib>.sh`. It is sourced by the runner and sets variables
and (optionally) overrides hook functions:

| Variable | Meaning |
|---|---|
| `LIB_REPO` (required) | git URL to clone |
| `LIB_REF` (required) | **pinned** tag/branch/sha (`git clone --depth 1 --branch`) |
| `LIB_ENTRY` (required) | bundle entry file, relative to the repo root |
| `LIB_ESBUILD_PLATFORM` | esbuild `--platform` (default `node`; acorn uses `neutral`) |
| `LIB_ESBUILD_EXTRA` | bash array of extra esbuild flags (e.g. `(--main-fields=main,module)`) |
| `LIB_SHIM` | extra per-lib shim file (relative to `scripts/`) layered after `node-shim.js` |
| `LIB_EXPECT_COUNT` | if set, both engines must report exactly this count (belt-and-suspenders against silent bundling drift) |
| `LIB_TIMEOUT` | seconds; wrap each engine run so a hang/slow suite reports cleanly |

Hook functions:

- `lib_prepare()` — runs once after clone, `cd`'d into the repo. Do `npm install`,
  builds, source patches, or entry generation here. Default: no-op.
- `lib_verdict <output_file> <exit_code>` — echo `PASS <n>` / `FAIL <n>` (n = the
  cross-checked test count) and return 0/1. Default: succeed iff exit code 0.

A shared helper `verdict_in_total <output_file>` is available for suites that end
with `In total, X of Y tests passed` (PASS iff `X == Y && Y > 0`, count `Y`).

Pin everything. `esbuild` itself is pinned (`ESBUILD_VERSION` in
`run-library-tests.sh`) and installed once into `/tmp/jsse-libtests/tooling/`.

### MikeMcl libraries (decimal.js / big.js / bignumber.js)

These share a self-contained runner that loads each module with a **dynamic**
`require(PREFIX + name)`, which esbuild cannot bundle. `scripts/gen-mikemcl-entry.js`
reads the original entry, extracts the module list / require prefix / harness
global, and emits an equivalent entry using literal `require()` calls. Each
config's `lib_prepare` invokes it (see `scripts/libs/decimal.js.sh`).

## Shim self-test

`scripts/run-node-shim-selftest.sh` exercises the readable-output layer directly,
independent of any library:

```sh
./scripts/run-node-shim-selftest.sh [--no-build]
```

It concatenates `node-shim.js` in front of `node-shim.selftest.js` (exactly as
the runner prepends the shim to a bundle), runs the result on **jsse `--node`**
and on **Node**, and requires both to exit 0 and emit byte-identical stdout.
Node is the oracle for the deterministic surfaces — the `util.format` specifiers
(`%s %d %i %f %j %c %%`), byte-accurate `process.stdout.write`, and the
`console.count`/`group`/`assert` output shapes are asserted exactly. The
byte-exact `%s` guarantee covers primitives and objects with a user-defined
`toString`; `%s`/`%o`/`%O` of plain objects and arrays route through
`util.inspect`, which is intentionally best-effort (depth, cycles, common types)
— it does not invoke getters, but it is only smoke-tested structurally and never
byte-compared against Node. (Fully Node-accurate `%s` object dispatch is tracked
separately.)

## Current status

| Library | Ref | Result on jsse | Notes |
|---|---|---|---|
| `acorn` | 8.16.0 | ✅ 13,507 (cross-checked) | ~35 s. Pinned pre-8.17.0; see below. |
| `decimal.js` | v10.6.0 | ✅ 22,624 (cross-checked) | seconds |
| `big.js` | v6.2.2 | ✅ 47,456 (cross-checked) | ~7 min — heavy arbitrary-precision division/sqrt/pow on the tree-walker |
| `bignumber.js` | v9.1.2 | ⚠️ blocked | see below; green on Node today |

### Engine bugs surfaced (tracked separately — out of scope for the no-`src` harness slice)

- **acorn 8.17.0+ deep-recursion abort.** 8.17.0 added a parser stack-guard test
  (`"[".repeat(2000)`) expecting the engine to *throw* a stack-space error. jsse's
  tree-walker amplifies acorn's recursive-descent frames and its Rust stack aborts
  (SIGABRT) before acorn's guard fires. Pinned to 8.16.0 until jsse raises a
  catchable `RangeError` on deep recursion; then bump the pin.
- **bignumber.js strict-mode constructor return (jsse#238).** In a strict-mode
  constructor, `return <call>` whose call returns a non-object makes jsse's `new`
  return that value instead of `this`. bignumber's constructor does
  `return parseNumeric(...)` (which returns `undefined`), so
  `new BigNumber("Infinity"|"NaN")` yields `undefined` on jsse. Minimal repro:
  `'use strict'; function u(x){x.z=1} function F(){this.a=1;return u(this)} typeof new F()`
  → `undefined` on jsse, `object` on Node (sloppy mode is correct). The config is
  correct and green on Node; it goes green on jsse once the bug is fixed.
