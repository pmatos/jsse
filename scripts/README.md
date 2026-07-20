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
| `LIB_SHIMS` | ordered array of additional shim files; use when a library needs more than `LIB_SHIM` |
| `LIB_ENV` | host-process environment assignments applied to both engine runs (e.g. `("TZ=America/New_York")`). Reaches each engine's **native** layer only — jsse's Rust `Date`/`Intl` and Node's ICU read the OS `TZ`/`LANG`; **not** reflected in jsse's JS-visible `process.env`, which the `--node` shim leaves `{}`. Don't use it for values a library reads from `process.env` in JS. |
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
their root-object detection. The TAP frontend supports Jest's array-table
`test.each` form in addition to the basic globals. Mocha-style `describe.only`
and `it.only` filter the registered suite tree globally, including nested
focus, direct-test precedence, and focused skipped tests; the exclusive table
form `test.only.each` registers each generated row as a focused test.

Layer it into a library by setting `LIB_SHIM="node-test-harness.js"` (it is
prepended after `node-shim.js` and `node-buffer-shim.js`). Like the other shims
it is **inert on Node** — it activates only under jsse's `--node` host mode
(keyed off the `__host_write` syscall floor, which real Node never has). That
inertness is what makes the cross-check meaningful: on Node the suite's *own*
framework runs, so the assertion count jsse reports through the adapter is
checked against the count real QUnit/mocha report on Node, not against itself.
Suites whose native CLI cannot run from a single bundle may prepend
`node-test-harness-force.js` before the harness to opt into the same in-process
runner on both engines. Luxon uses this path because Jest's CLI needs
filesystem/workers; its generated entry bundles Jest's published matcher core,
so assertions still use Jest semantics while Node checks the identical test
bundle and exact count.

The adapter's assertion counting mirrors qunitjs 2.x exactly (`config.stats.all
+= assertions.length` per test; an `expect(n)` mismatch or a zero-assertion test
each push one failing assertion), so the `PASS: p  FAIL: f  TOTAL: t` summary it
prints — the line the verdict parses — equals the one real `qunit-extras` prints
on Node. `config.noglobals` is intentionally **not** enforced on the jsse side:
the Node oracle enforces it and the suite passes it there, so enforcing on jsse
could only add jsse-specific failures the oracle lacks and diverge the count.
QUnit uses default autostart after synchronous registration (and `QUnit.load()`
re-checks it), while nested modules inherit outer hooks with QUnit's module and
per-test ordering. Async QUnit tests (`assert.async`) and callback-style TAP
tests/hooks (`function (done) { ... }`) are bounded by a 10 s timeout so a
completion callback that never fires becomes a failure instead of stalling the
run.

QUnit suites whose tests and hooks are entirely synchronous may set
`QUnit.config.sync = true` before `QUnit.start()`. This opt-in executes the
suite in the main script instead of scheduling one Promise continuation per
test, which keeps large synchronous corpora out of jsse's bounded host-async
drain window. Async behavior remains the default; the synchronous mode rejects
Promise-returning tests/hooks and incomplete `assert.async()` tokens. Moment
uses this mode because its 3,871 synchronous tests take roughly 35 minutes on
the tree-walker.

The assembled bundle uses a `.cjs` suffix so Node always evaluates the
reference oracle as CommonJS. This is independent of any unrelated ancestor
`package.json` that may declare `"type": "module"` above the `/tmp` cache.

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

> Full **chai**, **tape**, and **uvu** adapters remain deferred. Luxon settles
> the Jest seam concretely: `gen-luxon-entry.js` bundles the published
> `expect/build/matchers` core and supplies only the small invocation wrapper
> and two matchers its suite needs outside that core (`toThrow` and one inline
> snapshot).

## Current status

| Library | Ref | Result on jsse | Notes |
|---|---|---|---|
| `acorn` | 8.16.0 | ✅ 13,507 (cross-checked) | ~35 s. Pinned pre-8.17.0; see below. |
| `decimal.js` | v10.6.0 | ✅ 22,624 (cross-checked) | seconds |
| `big.js` | v6.2.2 | ✅ 47,456 (cross-checked) | ~7 min — heavy arbitrary-precision division/sqrt/pow on the tree-walker |
| `lodash` | 4.17.21 | ✅ 6,794 (cross-checked) | QUnit via the shared harness; a few tests skipped on jsse — see below |
| `ajv` | v8.17.1 | ⚠️ 5,466 / 5,480 (Node: 5,480) | ~4 min; four codegen option variants across drafts 6, 7, 2019-09, and 2020-12; residuals tracked in #274 and #275 |
| `prismjs` | v1.30.0 | ✅ 2,563 (cross-checked) | token streams for ~290 grammars |
| `js-sha256` | v0.11.1 | ✅ 916 (cross-checked) | Pure-JS SHA-224/SHA-256 and HMAC vectors; string, Buffer, TypedArray, and ArrayBuffer inputs |
| `luxon` | 3.7.2 | ⚠️ 1,045 / 1,152 | exact count cross-checked; Node is 1,152 / 1,152; blocked on #262–#265 |
| `moment` | 2.30.1 | ⚠️ 198 failing assertions across 3,871 tests | exact registered-test count cross-checked; Node is green with 162,868 assertions; residual tracked in #311 |
| `bignumber.js` | v9.1.2 | ⚠️ blocked | see below; green on Node today |

### PrismJS token-stream fixtures

`scripts/gen-prism-entry.js` embeds Prism core, dependency-ordered grammar
components, and all 2,563 non-HTML `.test` fixtures into a deterministic,
filesystem-free entry. Each fixture gets a fresh Prism instance and its
simplified token stream is compared byte-for-byte with the upstream expected
JSON. The 11 `.html.test` fixtures are excluded because they test Prism's DOM
markup rendering instead of tokenization.

JSSE and Node execute all 2,563 fixtures. The cross-check requires both engines
to report the same fixture count, so engine-specific skips cannot masquerade as
a successful run.

### Luxon

Luxon's 58 Jest files are statically bundled by `gen-luxon-entry.js`; the
generated entry uses Jest's pinned `expect@29.7.0` matcher core and the shared
TAP runner. Both engines run under `TZ=America/New_York` and must report exactly
1,152 tests. Node is green; jsse currently executes every test and passes 1,045.

`patch-luxon-icu.js` contains four oracle-portability adjustments for literals
that changed in CLDR 47 / ICU 78. They do not alter the count: two old locale
fixtures are gated only on Node's advertised CLDR version, and two Coptic-era
assertions accept the old and current ICU spellings.

The jsse failures are left visible rather than converted to skip assertions.
They are concentrated in four follow-ups surfaced by the suite:

- #262 — `Intl.DateTimeFormat` locale names/patterns are partially hard-coded.
- #263 — Node host mode does not honor `TZ` for the system time zone.
- #264 — unknown IANA-shaped identifiers are accepted as valid time zones.
- #265 — `Intl.Locale#getWeekInfo()` omits `minimalDays`.

### Moment

Moment's 137 locale definitions, 52 core QUnit files, and 138 locale QUnit
files are statically imported by `gen-moment-entry.js`. This replaces only
upstream's filesystem discovery and dynamic locale `require()` path; the test
bodies remain upstream code. `patch-moment-bundle.js` makes one deprecation
expectation explicit because esbuild deduplicates a hooks module that upstream
transpiles separately. It strengthens the check symmetrically on both engines
and does not change the registered-test count.

Both engines run the same generated bundle and shared synchronous QUnit
adapter under `TZ=America/New_York`, `LANG=en_US.utf8`, and
`LC_ALL=en_US.utf8`. Node passes all 3,871 registered tests and 162,868
assertions. jsse also executes all 3,871 tests, recording 153,088 passing and
198 failing assertions out of 153,286 reached assertions; callbacks that throw
early account for the lower assertion total. The failures remain visible and
are tracked in #311. The first 100 diagnostics all report
`TypeError: Cannot convert object to primitive value`, concentrated in week/year
and locale parser/formatting cases.

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

### Engine bugs surfaced

- **AJV schema compilation and sustained codegen.** AJV's upstream
  JSON-Schema-Test-Suite is inlined into a 5,480-case bundle, then each schema
  runs through four normal AJV option variants (roughly 22,000 generated
  validator executions). Node passes all 5,480 registered cases; jsse passes
  5,466 deterministically. Meta-schema validation is disabled symmetrically
  while #266 is open. The remaining generated-validator result mismatches are
  tracked in #274 and the two catchable call-depth failures in #275.

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
- **Luxon Intl/system-zone gaps (jsse#262–#265).** The pinned suite and bundle
  are green on Node (1,152 tests). jsse runs the same 1,152 cases and passes
  1,045; the remaining failures stay visible until the four root Intl and host
  time-zone gaps above land.

- **Moment sustained object-to-primitive failures (jsse#311).** The pinned
  bundle is green on Node (3,871 tests, 162,868 assertions). jsse executes every
  registered test but 198 callbacks fail with `TypeError: Cannot convert object
  to primitive value`; representative operations pass in a fresh bundle, so
  the follow-up tracks the sustained-execution/state interaction.
