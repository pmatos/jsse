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

## Shared test-runner harness (`node-test-harness.js`)

Many suites don't ship a self-contained runner — they lean on a framework whose
*assertion* library is pure JS but whose *runner* would otherwise need
fs/workers/vm (mocha's/jest's CLI, QUnit's Node runner, `qunit-extras`'
`setInterval` progress ticker, …). `node-test-harness.js` supplies the runner
in-process so those suites execute on jsse. It provides two frontends over one
shared core (which includes QUnit's own `equiv`, ported verbatim, for
`deepEqual`):

- a **QUnit adapter** installed as a global `QUnit` — suites that do
  `root.QUnit || require('qunit-extras')` (e.g. lodash) pick it up, so the
  bundled framework stays dormant; and
- a **TAP-emitting `describe`/`it`/`test`/`before`/`after` runner**
  (mocha/jest/tape shape) as the reusable spine for later library clusters.

It also aliases Node's `global` to `globalThis`, which many bundles rely on for
their root-object detection.

Layer it into a library by setting `LIB_SHIM="node-test-harness.js"` (it is
prepended after `node-shim.js` and `node-buffer-shim.js`). Like the other shims
it is **inert on Node** — it activates only under jsse's `--node` host mode
(keyed off the `__host_write` syscall floor, which real Node never has). That
inertness is what makes the cross-check meaningful: on Node the suite's *own*
framework runs, so the assertion count jsse reports through the adapter is
checked against the count real QUnit/mocha report on Node, not against itself.

The adapter's assertion counting mirrors qunitjs 2.x exactly (`config.stats.all
+= assertions.length` per test; an `expect(n)` mismatch or a zero-assertion test
each push one failing assertion), so the `PASS: p  FAIL: f  TOTAL: t` summary it
prints — the line the verdict parses — equals the one real `qunit-extras` prints
on Node. `config.noglobals` is intentionally **not** enforced on the jsse side:
the Node oracle enforces it and the suite passes it there, so enforcing on jsse
could only add jsse-specific failures the oracle lacks and diverge the count.
Async tests (`assert.async`) are bounded by a 30 s per-test timeout so a `done()`
that never fires becomes a failure instead of stalling the run.

### Harness self-test (`run-harness-selftest.sh`)

Because the harness is jsse-only (inert on Node), it can't be cross-checked
against Node the way the Buffer shim is. Instead `scripts/harness-fixtures/*.fixture.js`
drive the QUnit adapter and the TAP runner through a deterministic mix of
passing and failing tests, each declaring the exact summary line it must emit:

```sh
./scripts/run-harness-selftest.sh [--no-build]
```

The full end-to-end validation of the adapter's counting and `deepEqual` is the
lodash cross-check below (jsse adapter vs. Node's real `qunit-extras`, 6,794
assertions).

> The **chai**, **jest-`expect`**, **tape**, and **uvu** adapters that the epic
> anticipates are intentionally **deferred**. Unlike QUnit's global-probe
> (`root.QUnit || …`), those are consumed via `require`/`import`, so injecting a
> jsse implementation needs a different mechanism (esbuild `--alias` or bundling
> the real library) that should be settled against a concrete consuming library
> in the #233–236 clusters rather than guessed at here.

## Current status

| Library | Ref | Result on jsse | Notes |
|---|---|---|---|
| `acorn` | 8.16.0 | ✅ 13,507 (cross-checked) | ~35 s. Pinned pre-8.17.0; see below. |
| `decimal.js` | v10.6.0 | ✅ 22,624 (cross-checked) | seconds |
| `big.js` | v6.2.2 | ✅ 47,456 (cross-checked) | ~7 min — heavy arbitrary-precision division/sqrt/pow on the tree-walker |
| `lodash` | 4.17.21 | ✅ 6,794 (cross-checked) | QUnit via the shared harness; a few tests skipped on jsse — see below |
| `bignumber.js` | v9.1.2 | ⚠️ blocked | see below; green on Node today |

### lodash skip list (jsse only; each preserves the assertion count via `skipAssert`)

lodash is green and cross-checked at 6,794 assertions, with a small set of tests
routed through lodash's own `skipAssert(N)` on jsse (Node still runs them, so the
count matches). `scripts/patch-lodash-jsse.js` applies these; each is a jsse
characteristic surfaced by the suite, tracked as a follow-up:

- **"should work with extremely large arrays"** (flatten, min/max) — 500k-element
  operations run for minutes on the tree-walker (a performance limit, not a
  correctness one).
- **"should match lone surrogates"** — jsse's regex matches lone surrogates where
  lodash's word pattern expects no match.
- **createWrapper "should work when hot"** — throws `RangeError: Invalid array
  length` deep in lodash's hot-path wrapper rebuild on jsse.
- **bizarro reload + vm-`root`-of-`this`** (skipped on *both* engines) — both
  reload the lodash *source file* via a dynamic `require`/`readFileSync`, which
  doesn't exist relative to the esbuild bundle. These already fall back to
  `skipAssert` in lodash's own browser path.

Timer-heavy suites (`debounce`/`throttle`) pass because `node-test-harness.js`
backs `setTimeout`/`clearTimeout`/`setInterval` with a single-pump userland queue:
jsse's native `setTimeout` spawns a thread per call and offers no cancellation, so
running those thousands of timers natively would otherwise exhaust OS threads.

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
