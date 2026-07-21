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
| `LIB_BUNDLE_PREFIXES` | ordered prefix files; when set, build one isolated copy of the bundle after each prefix (for the same corpus in multiple modes) |
| `LIB_SEPARATE_BUNDLES` | with bundle prefixes, run each prefixed copy in a separate engine process and concatenate their output before verdict evaluation (default `0`) |
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
in-process so those suites execute on jsse. It provides three frontends over one
shared core (which includes QUnit's own `equiv`, ported verbatim, for
`deepEqual`):

- a **QUnit adapter** installed as a global `QUnit` — suites that do
  `root.QUnit || require('qunit-extras')` (e.g. lodash) pick it up, so the
  bundled framework stays dormant; and
- a **TAP-emitting `describe`/`it`/`test`/`before`/`after` runner**
  (mocha/jest shape) as the reusable spine for later library clusters; and
- a **tape assertion-object adapter**, selected only on JSSE so Node still uses
  real tape as the framework oracle.

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

The QUnit adapter's assertion counting mirrors qunitjs 2.x exactly (`config.stats.all
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

The full end-to-end validation of the QUnit adapter's counting and `deepEqual`
is the lodash cross-check below (jsse adapter vs. Node's real `qunit-extras`,
6,794 assertions). The tape fixture covers its lifecycle and assertion surface;
qs provides the Node-cross-checked end-to-end proof.

Tape-based suites alias `require('tape')` to `node-tape-module.js`. On JSSE the
module selects the in-process adapter; on Node it selects bundled real tape.
The adapter counts individual assertions (including skipped subtests) and
supports nested tests, plans, teardown/interception, and tape's common
equality/throw/match assertions.

Dependencies that import Node's `buffer` module instead of reading the global
can similarly alias it to `node-buffer-module.js`. The selector exports the
shared JS-only Buffer shim on JSSE and Node's native `node:buffer` module on the
reference path. `node-util-module.js` provides the equivalent selector for the
shared `util.format`/`util.inspect` implementation, while
`node-string-decoder-module.js` supplies iconv-lite's buffered decoder seam,
including UTF-8, UTF-16LE, base64, and base64url continuation state across
`write()`/`end()` chunk boundaries.

Full **chai** and **uvu** adapters remain deferred. Luxon settles the Jest seam
concretely: `gen-luxon-entry.js` bundles the published `expect/build/matchers`
core and supplies only the small invocation wrapper and two matchers its suite
needs outside that core (`toThrow` and one inline snapshot).

## Current status

| Library | Ref | Result on jsse | Notes |
|---|---|---|---|
| `acorn` | 8.16.0 | ✅ 13,507 (cross-checked) | ~35 s. Pinned pre-8.17.0; see below. |
| `decimal.js` | v10.6.0 | ✅ 22,624 (cross-checked) | seconds |
| `big.js` | v6.2.2 | ✅ 47,456 (cross-checked) | ~7 min — heavy arbitrary-precision division/sqrt/pow on the tree-walker |
| `lodash` | 4.17.21 | ✅ 6,794 (cross-checked) | QUnit via the shared harness; a few tests skipped on jsse — see below |
| `ajv` | v8.17.1 | ⚠️ 5,466 / 5,480 (Node: 5,480) | ~4 min; four codegen option variants across drafts 6, 7, 2019-09, and 2020-12; residuals tracked in #274 and #275 |
| `prismjs` | v1.30.0 | ✅ 2,563 (cross-checked) | token streams for ~290 grammars |
| `uglify-js` | v3.19.3 | ✅ 4,233 (cross-checked) | ~15 min; complete compress DSL parse/transform/mangle/codegen corpus |
| `highlight.js` | 11.11.2 | ✅ 731 (cross-checked) | 536 markup + 195 auto-detection fixtures across 192 grammars; ~30 min |
| `js-sha256` | v0.11.1 | ✅ 916 (cross-checked) | Pure-JS SHA-224/SHA-256 and HMAC vectors; string, Buffer, TypedArray, and ArrayBuffer inputs |
| `qs` | v6.15.3 | ✅ 1,013 (cross-checked) | tape corpus: nested parse/stringify, limits, charsets, Buffer, pollution guards, Map/WeakMap side channels |
| `js-md5` | v0.8.3 | ✅ 550 (cross-checked) | Pure-JS MD5 and HMAC-MD5 vectors; UTF-8 strings, Buffer, TypedArray, and ArrayBuffer inputs |
| `luxon` | 3.7.2 | ⚠️ 1,045 / 1,152 | exact count cross-checked; blocked on #262–#265. Node itself now fails 13/1,152 too (ICU/CLDR drift on the reference host, not a jsse bug — see below) |
| `zod` | v4.4.3 | ❌ hangs (jsse#340) | normal + jitless; jsse livelocks indefinitely (spinning thread, never prints a result) instead of completing — see below. Last known result before the regression: 2,176 / 2,184, residuals tracked in #313–#315 |
| `moment` | 2.30.1 | ✅ 162,868 assertions (cross-checked) | 3,871 tests, 0 failures — fixed by #311/PR #326 |
| `bignumber.js` | v9.1.2 | ✅ 65,143 (cross-checked) | unblocked by #238 |
| `esprima` | (unreleased) `512cd66` | ⚠️ 80,100 / 80,153 (Node: 80,153) | ~65 min; ~1,650 unit fixtures + api/grammar/hostile suites + a 78,402-scenario test262 grammar corpus; residuals tracked in #357 and #358 |
| `uuid` | v14.0.1 | ✅ 75 (cross-checked) | Node's own `node:test`/`node:assert/strict` upstream suite, unmodified; browser build so v3/v5 use pure-JS MD5/SHA-1 and v1/v4/v6/v7 draw randomness via a `crypto.getRandomValues`/`randomUUID` shim (`node-crypto-shim.js`) backed by `__host_random_bytes` |

### Zod normal and jitless corpus

**Currently hangs jsse indefinitely (jsse#340), a regression discovered
2026-07-20.** `./scripts/run-library-tests.sh zod` (and running either
generated bundle directly, e.g. `jsse --node final-0.cjs`) prints only the TAP
version header and then never produces another line — confirmed
reproducible in isolation, not a parallel-run/resource-contention artifact.
`/proc` inspection shows the main thread parked on a futex while a second
thread spins, burning close to 100% CPU forever: a livelock, not merely a slow
run. This is distinct from #310 below (#310 is jsse exiting early after
losing one delayed callback; #340 is jsse never printing anything and never
exiting). Timing/code-area suspicion, not a confirmed root cause: same-day
commit `03df6fe` (PR #326) reworked GC temp-root handling around binary
operators, and zod's validation codegen is unusually binary-op-heavy at scale.
The description below reflects the suite's last known-good state, before this
regression, and is what running it again should reproduce once #340 is fixed.

`gen-zod-entry.js` statically imports all 79 v4 classic runtime test files.
Native Vitest at v4.4.3 reports 1,092 tests; the harness runs the identical IIFE
once normally and once with Zod's global `jitless` option, locking the combined
count at 2,184. The two modes use separate engine processes so module caches,
GC state, and pending host jobs cannot leak across the boundary. Node runs the
same two generated files and is green at 2,184 / 2,184.

The adapter uses Jest's published matcher core plus Vitest's pretty-printer;
type-only `expectTypeOf` calls remain runtime no-ops. Node-only test imports are
replaced by symmetric, bundle-local portability modules. The upstream 10 MiB
base64 throughput input is bounded to 64 KiB, and one artificial 500 ms async
delay is changed to a zero-delay timer (#310). One jitless-only async function
refinement is visibly skipped under #309; all other registered bodies run.

Before the #340 regression, jsse reported 2,176 passing and eight failures:
Date parsing (#313), array outputs that fail `Object.isFrozen` after Zod
freezes them (#314), and the Node-specific text of a `JSON.parse` error
snapshot (#315), each repeated in normal and jitless mode. These remain
failing assertions rather than skips.

### qs iconv-lite shim

qs uses only iconv-lite's core `encode`/`decode` surface. Its JSSE-only shim
therefore hides the fake `process.versions.node` marker so iconv-lite does not
activate optional Node stream and Buffer-prototype extensions; real Node keeps
the marker and exercises its normal path. JSSE now emits the same receiver-aware
strict-assignment `TypeError` message as Node after the #318 fix landed on main
([#325](https://github.com/pmatos/jsse/pull/325)), so qs's upstream assertion
runs unmodified on both engines.

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

### UglifyJS compress fixtures

`scripts/gen-uglify-js-entry.js` embeds UglifyJS's implementation sources and
all 126 `test/compress/*.js` DSL files into a deterministic, filesystem-free
entry. All 4,233 cases run UglifyJS's parser, compressor and tree transforms,
scope analysis, optional identifier/property mangling, exact code-generation
comparison, AST validation, and output reparse. The native runner's
`expect_stdout` stage is excluded because it executes generated programs through
Node's `vm` or child processes rather than testing the transformation pipeline.

The suite exposed a non-Unicode RegExp range gap: character classes spanning
UTF-16 surrogates did not include jsse's internal PUA-mapped code units, and the
functional `@@replace` path converted matched/replacement strings lossily. The
engine now preserves those code units and the UglifyJS suite is skip-free.

### highlight.js markup and auto-detection fixtures

`scripts/gen-highlightjs-entry.js` registers all 192 built-in grammars from the
pinned source tree and embeds its filesystem fixtures into one deterministic
bundle. The 536 markup fixtures run in highlight.js debug mode and compare the
generated HTML byte-for-byte with upstream's expected output after the same
whitespace trimming as its Mocha suite.

The auto-detection corpus contributes 195 more cases after applying upstream's
`autoDetection()` filter to its 198 inputs (G-code, properties, and plain text
opt out).
Each eligible input competes against the complete grammar set, exercising the
relevance-scoring state machine rather than a single-language fast path. This
is the expensive half of the run: roughly 30 minutes on jsse's tree-walker.

Upstream currently comments out its dynamic auto-detection assertions, and
eight ambiguous samples are won by a different grammar when all languages
compete. The generator records Node's winners for pinned 11.11.2 and runs the
public production mode for detection; debug mode exposes an upstream Nix
zero-width assertion on otherwise valid inputs. Both engines therefore compare
against the same fixed Node oracle and still report the exact 731-case count.

### Luxon

Luxon's 58 Jest files are statically bundled by `gen-luxon-entry.js`; the
generated entry uses Jest's pinned `expect@29.7.0` matcher core and the shared
TAP runner. Both engines run under `TZ=America/New_York` and must report exactly
1,152 tests. jsse currently executes every test and passes 1,045.

As of 2026-07-20, real Node itself also fails 13/1,152 on this host — all ICU
week-numbering/locale-formatting tests (`getMinimumDaysInFirstWeek`,
`weeksInLocalWeekYear`, `Interval#toLocaleString`). This looks like Node/ICU
version drift on the reference machine since the harness was last validated
(Node's bundled ICU/CLDR updates over time and this harness has no Node-version
pin), not a jsse regression — no new jsse-side issue was filed for it.

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
`LC_ALL=en_US.utf8`. Both jsse and Node pass all 3,871 registered tests and
all 162,868 assertions, cross-checked. jsse previously failed 198 of those
assertions with `TypeError: Cannot convert object to primitive value`,
concentrated in week/year and locale parser/formatting cases (#311); fixed by
PR #326 (`03df6fe`), which corrected GC temp-root handling around binary
operators so a persistent root captured during sustained execution was no
longer dropped.

### esprima test262 grammar corpus

esprima's own suites (`test/utils/create-testcases.js` + `evaluate-testcase.js`
ported inline, `test/api-tests.js` unmodified via the `assert` alias, an
everything.js smoke parse, and the two hostile-environment checks — see
`scripts/libs/esprima-jsse-entry.js`) cover esprima's unit-level surface, but
issue #295 also calls for a full test262 grammar-conformance pass. Reproducing
test262-stream's live enumeration pipeline at runtime needs `fs`/`stream`/a
real checkout — none of which exist in a jsse bundle — so
`scripts/gen-esprima-test262-corpus.js` runs that traversal once, in Node, at
`lib_prepare` time, and freezes the result into a bundlable data file
(`test/jsse-test262-corpus.js`).

It clones test262 pinned at `36d2d2d348d83e9d6554af59a672fbcd9413914b` and
walks it with `test262-stream` (`omitRuntime: true` — parsing only, never
evaluating), storing each file's raw source once (not per scenario — the
"strict mode" scenario is always exactly `'"use strict";\n' + source`, so the
runtime reconstructs it instead of storing a second copy) plus every
default/strict/module scenario test262 defines for it. A leading-boilerplate
strip (copyright header + YAML frontmatter) shrinks the unique-source payload
from 55.5 MiB to 19.3 MiB, verified parse-neutral per file before applying it.
`expected` per scenario is recorded by running local Node against the exact
compiled `dist/esprima.js` esprima-jsse-entry.js's test262 runner replays at
run time — not test262-stream's harness-bloated `contents` and not the
spec-ideal outcome — so a jsse run can only diverge from what's recorded by
actually behaving differently from Node on the identical esprima code;
esprima's own pre-existing spec gaps (tracked in
`test/test-262-whitelist.txt`) don't masquerade as new jsse bugs. The
whitelist is cross-referenced only as a generation-time sanity log.
Both the strip-safety check and the recording pass run in
`gen-esprima-test262-corpus-worker.js`, spawned as fresh child processes, so
the two concerns never share process state; the recording pass replays the
exact (module, string) sequence in the exact file/scenario order the runtime
test262 block replays, so generation and runtime are structurally the same
parse sequence. The final corpus is a `module.exports = "<JSON-encoded
string>"` (double-encoded so jsse's tokenizer scans one string token rather
than building an AST for tens of thousands of nested literals) at 31 MiB,
close to the ~26 MiB the issue estimates.

Generation is deterministic: a from-scratch Node replay of the recorded
(module, source) sequence reproduces the recorded `expected` values exactly,
every time. jsse's 53 residual failures (of 80,153 total, cross-checked
against Node) are genuine, reproducible jsse-vs-Node divergences, not corpus
flakiness — see "Engine bugs surfaced" below for what they are.

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
- **bignumber.js strict-mode constructor return (jsse#238, fixed).** In a
  strict-mode constructor, `return <call>` whose call returns a non-object made
  jsse's `new` return that value instead of `this`. bignumber's constructor does
  `return parseNumeric(...)` (which returns `undefined`), so
  `new BigNumber("Infinity"|"NaN")` yielded `undefined` on jsse. Minimal repro:
  `'use strict'; function u(x){x.z=1} function F(){this.a=1;return u(this)} typeof new F()`
  → was `undefined` on jsse, `object` on Node (sloppy mode is correct). Now
  fixed; the suite is green on jsse (65,143, cross-checked).
- **Luxon Intl/system-zone gaps (jsse#262–#265).** The pinned suite and bundle
  are green on Node (1,152 tests) as of when these issues were filed — see the
  Luxon section above for a newer, unrelated 13-test Node-side ICU-drift
  finding. jsse runs the same 1,152 cases and passes 1,045; the remaining
  failures stay visible until the four root Intl and host time-zone gaps above
  land.

- **Moment sustained object-to-primitive failures (jsse#311, fixed).** The
  pinned bundle is green on Node (3,871 tests, 162,868 assertions). jsse used
  to fail 198 callbacks with `TypeError: Cannot convert object to primitive
  value` during sustained execution; fixed by PR #326 (`03df6fe`), which
  stopped a persistent GC root from being dropped around binary-operator
  evaluation. jsse is now green too (162,868/162,868, cross-checked).

- **Zod livelock (jsse#340, open).** See the Zod section above. jsse hangs
  indefinitely running the zod normal/jitless corpus instead of completing —
  a spinning thread burns ~100% CPU while the main thread waits on a futex
  that's never signaled. Discovered 2026-07-20, same day as the moment fix
  above; suspected (unconfirmed) to be an edge case in the same GC temp-root
  rework, since zod's validation codegen is unusually binary-operator-heavy.

- **Astral-plane identifiers rejected (jsse#357, open).** esprima's own
  parser, running unmodified inside jsse, rejects identifiers containing
  astral-plane (surrogate-pair) Unicode code points that are valid
  `ID_Start`/`ID_Continue` characters — e.g. `var \u{1E800};` throws
  `Unexpected token ILLEGAL` on jsse but parses on Node. The same check gates
  regex named-group names, so those are hit too. Accounts for 49 of the
  esprima test262-corpus/unit-fixture residuals above (18 test262 identifier
  files × 2 scenarios, 1 test262 named-groups file × 2 scenarios, 9 esprima
  unit fixtures, 2 everything.js smoke fixtures).

- **Regex property-escape range endpoint not rejected (jsse#358, open).**
  `NonemptyClassRanges` static semantics require a Syntax Error when either
  endpoint of a character-class range is a Unicode property escape
  (`\p{...}`), since it denotes a whole class rather than one character.
  esprima's own regex-validation logic misses this early error when run on
  jsse (`/[￿-\p{Hex}]/u` and `/[\p{Hex}--]/u` both parse instead of
  throwing) but catches it correctly on Node. Accounts for the remaining 4
  esprima test262-corpus residuals above.

- **Literal `{}` in a regex rejected instead of Annex B fallback (jsse#359,
  open).** Found via manual exploration while wiring the esprima harness, not
  surfaced by any esprima/test262 scenario checked so far. Per Annex B's
  `ExtendedPatternCharacter`, a `{` not starting a valid quantifier should be
  a literal character in non-`u`/non-`v` mode; jsse rejects `/{}/g` outright,
  and as an uncatchable parse-time error for a regex *literal* specifically
  (vs. a catchable `SyntaxError` for the equivalent `new RegExp("{}", "g")`).

### uuid: resolving `node:test` / `node:assert/strict`, and `crypto.getRandomValues`

uuid's own upstream suite (`src/test/*.test.ts`, compiled by `tsc`) imports
`node:test` and `node:assert/strict` directly and is bundled unmodified — no
tape/QUnit-style adapter file like other libraries. Two new pieces make that
possible:

- **`node-crypto-shim.js`** (shared, opt-in via `LIB_SHIMS`) installs
  `globalThis.crypto.getRandomValues`/`.randomUUID`, backed by the
  `__host_random_bytes` syscall-floor primitive (#229). It only activates when
  `crypto` isn't already present — real Node has had a native global `crypto`
  since Node 19 (some older Node point releases only expose it under
  `--experimental-global-webcrypto` when running a plain script file, which
  would make this shim install unnecessarily and then throw, since
  `__host_random_bytes` doesn't exist on Node; run the harness with a Node ≥ 20
  from `~/.nvm/versions/node/` if you hit that).
- **`libs/uuid-jsse-require-shim.js`** resolves those two specifiers for jsse
  only. esbuild's `--platform=node` build (the default) treats Node-builtin
  specifiers as external, compiling each import down to a literal
  `require(specifier)` call via esbuild's own `__require` fallback helper —
  which, at call time, uses whatever `require` identifier is in scope. On real
  Node that's the CJS module's own `require` (visible from a nested IIFE via
  closure), so Node keeps resolving both specifiers natively — the whole point
  being that Node runs the *unmodified* upstream suite against its own
  `node:test`/`node:assert` as an independent oracle, not a jsse-side
  reimplementation. On jsse there is no ambient `require`, so this shim
  installs one as a global, mapping `"node:test"` to a thin wrapper around
  `node-test-harness.js`'s shared TAP `describe`/`test` globals (registering an
  arity-0 function so the harness's done-callback-vs-promise heuristic always
  takes the promise branch, while still handing the real callback a minimal
  `t` TestContext supporting just `t.mock.method`/`t.mock.reset` — the only
  TestContext surface the suite uses, for its "uses native `crypto.randomUUID`"
  tests) and `"node:assert/strict"` to `globalThis.__jsseAssertStrict`.
- **`libs/uuid-assert-connector.js`** sets `globalThis.__jsseAssertStrict` from
  a real, pinned copy of the `assert` npm package (browserify's pure-JS port of
  Node's own `assert` module) rather than a hand-rolled reimplementation. It's
  vendored via a relative `require("../node_modules/assert")` — deliberately
  *not* the bare specifier `"assert"`, which esbuild would also auto-external
  under `--platform=node` — the same trick `node-tape-module.js` uses for
  `tape`.
- The guard in both new shims keys off `__host_write` (the #229 syscall
  floor), not `process.versions.node`: `node-shim.js` installs a *fake*
  `process` with `versions.node` set (to pass UMD-style Node checks in
  bundled libraries), so any later shim checking `process.versions.node`
  would incorrectly think it's running on real Node.
- The suite is pinned to uuid's **browser** build (`dist/`, not `dist-node/`)
  by running `tsc` directly and swapping in the `*-browser.ts`-derived
  `md5.js`/`sha1.js` — the same rename `scripts/build.sh` does — so v3/v5
  exercise uuid's own pure-JS MD5/SHA-1 instead of `node:crypto`'s
  `createHash`, keeping this slice free of any `node:crypto` dependency.
