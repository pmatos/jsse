# Plan: issue #655 — JetStream runner gaps (`self`, `JetStream` shim) and failure diagnosability

## 1. Problem restated

Eight JetStream workloads fail under `scripts/run-jetstream.py`. Hand-running each
with the runner's exact script (jsse 0.8.2 release build, JetStream `c603c04`)
classifies them (full evidence is in
https://github.com/pmatos/jsse/issues/655#issuecomment-5752014609):

| Workload | Real cause | Class |
|---|---|---|
| `bigint-paillier` | `self` undefined; JetStream's shell driver does `globalObject.self = globalObject` (`JetStreamDriver.js:788`), the runner does not | runner gap |
| `bigint-noble-secp256k1` / `-ed25519` / `-bls12-381` | bundle reads `typeof self === 'object' && 'crypto' in self ? self.crypto : undefined` → "The environment doesn't have randomBytes function". Same missing `self` (bls takes 13 s because it fails after real work) | runner gap |
| `Babylon`, `first-inspector-code-load`, `multi-inspector-code-load` | `ReferenceError: JetStream is not defined`: at `c603c04` they use `JetStream.preload.<name>` + `await JetStream.getString(path)`; the runner still injects stale `globalThis.<name> = "<source>"` strings | runner gap |
| `async-fs` | `runIteration()` throws `TypeError: Cannot read properties of undefined (reading 'byteLength')` mid-run (a `File`'s `DataView` reads back `undefined`); allocation-volume dependent, so a GC/rooting-style engine defect | **engine bug → filed as #679** |

All seven async workloads read "no JSON output" for one reason: `build_async_harness`
never observes a rejection, and jsse (like a plain shell) drops an unobserved rejection
with exit 0 and empty stdout/stderr. Separately, the `no JSON output` result dict keeps
`stdout[:500]` but not stderr, so nothing survived to diagnose from.

Fix on the runner side: define `self`, replace the stale preload globals with a
`JetStream` shim (`preload` path map + `getString`/`getBinary`), make the async harness
report a rejection as a parseable line, and keep stdout **and** stderr in every failure
result. The engine bug is out of this PR (#679).

## 2. Spec basis

N/A: no JavaScript behavior change. Everything here is benchmark-harness tooling
under `scripts/`; `self` and `JetStream` are host globals of the *runner's prelude*
(WHATWG HTML / JetStream shell conventions, not ECMAScript), and they must stay in
`build_polyfill_preamble` / the generated script, never in jsse's own global object
(same precedent as `scripts/node-shim.js`: "never baked into jsse's globals, so
test262 is unaffected"). The engine-side follow-up (#679) is spec-governed (job-queue
draining, HostEnqueuePromiseJob, object lifetime) and is not planned here.

## 3. Files to touch

- `scripts/run-jetstream.py`
  - `build_polyfill_preamble()` — add guarded `self` definition.
  - `build_preload_code()` — emit a `globalThis.JetStream` shim instead of raw-source globals.
  - `build_async_harness()` — observe rejection, print `{"error": ...}` (stdout) and the text (stderr).
  - `run_benchmark_once()` — parse the error line; retain stdout + stderr on every failure; distinct reason when an async harness never settled.
  - `process_result()` in `main()` — show a short cause on the `FAIL` line (first line of the error/stderr).
  - module docstring / help text if it mentions preloads.
- `scripts/test_benchmark_protocol.py` — new test classes (see §4); a small module-level helper to pick an engine (`target/release/jsse`, else `node`, else skip) shared with `HarnessNameHygieneTests`.
- `README.md` — "Running JetStream 3": one paragraph on the emulated host globals (`self`, `JetStream.preload/getString/getBinary`) and that failures now keep stdout/stderr in `--json` output.
- No `docs/adr/` entry (no architectural decision), no `CONTEXT.md` change.
- **Not touched:** anything under `src/`, `spec/`, `test262/`, `test262-pass.txt`.

## 4. TDD slices

Test runner for all slices: `uv run python -m unittest scripts.test_benchmark_protocol -v`
(CI runs `uv run python -m unittest discover -s scripts -p 'test_*.py'`, `.github/workflows/ci.yml:61`).

Existing constraint to honour: `HarnessNameHygieneTests.test_harnesses_declare_nothing_at_top_level`
(#673) — the generated script shares one Script scope with benchmark sources, so **nothing
new may add a top-level `const/let/var/class/function`**. Wrap new prelude code in `if` blocks
or an IIFE and assign through `globalThis.`.

1. **`self` is defined by the preamble** (vertical: paillier + noble unblocked)
   - Red: `PolyfillPreambleTests` in `test_benchmark_protocol.py`: (a) preamble text contains a guarded `globalThis.self = globalThis`; (b) running `preamble + 'print(self === globalThis)'` on the engine prints `true`; (c) a pre-existing `self` is not clobbered (`var`-less: define `globalThis.self = 42` before the preamble, expect it to survive).
   - Green: in `build_polyfill_preamble()` add `if (typeof self === "undefined") { globalThis.self = globalThis; }`.
   - Extend the hygiene test to also scan the preamble and the preload code (today it only scans the harnesses).

2. **`JetStream` shim replaces raw-source preload globals** (Babylon + the two code-load workloads)
   - Red: `PreloadShimTests`: write a temp preload file containing backticks, `${x}`, backslashes, `</script>`, U+2028 and non-ASCII; `build_preload_code({"blob": "dir/blob.js"}, tmp)` then run `code + async check` on the engine: `await JetStream.getString(JetStream.preload.blob)` equals the file bytes; `JetStream.getString("unknown")` rejects with a clear error; `getBinary` rejects with a "not supported by run-jetstream.py" error; missing file still returns `None` (existing "skipped: preload files not found" behavior); no top-level declarations.
   - Green: rewrite `build_preload_code()` to emit
     `;(() => { const contents = <json.dumps({path: text}) >; globalThis.JetStream = { preload: <json.dumps({name: path})>, getString: async (p) => …, getBinary: async (p) => { throw … } }; })();`
     JSON-encoded (more robust than the current backtick escaping; embedded contents, **not** routing through `--node`/`__host_*`, which would change the measured environment). jsse has no `read`/`readFile`, so contents must be inlined by the runner, keyed by the same path strings the `preload` map returns.
   - Decision log: only Babylon, `first-inspector-code-load`, `multi-inspector-code-load` have non-empty preloads (verified by sweeping `BENCHMARKS` against `JetStream.*` uses in the sources), so blast radius is exactly the three failing workloads. The old `globalThis.airBlob`-style globals are dead at `c603c04`; drop them (confirm with `grep -rn "airBlob\|basicBlob\|inspectorBlob\|babylonBlob\|inspectorPayloadBlob" /tmp/JetStream --include=*.js` that no source reads the bare globals).

3. **Async rejection is reported, not swallowed**
   - Red: (a) text test: `build_async_harness(...)` output attaches a rejection handler that prints a JSON `{"error": ...}` line and writes the message to stderr (`printErr`); (b) engine test: preamble + `class Benchmark { async init() { throw new Error("boom") } runIteration() {} }` + async harness prints a last stdout line whose JSON has `"error"` containing `boom`; (c) `run_benchmark_once` with a fake engine (python script, same pattern as `RunnerCliTests.setUp`) that prints `{"error": "boom"}` returns `status == "error"`, `reason` containing `boom`, and keeps `stdout`/`stderr`.
   - Green: `.catch((e) => { const message = …; printErr(message); print(JSON.stringify({ error: message })); })` on the IIFE (hygiene: still no top-level declaration; keep the leading `;`), and in `run_benchmark_once` check `data.get("error")` before `data["results"]`.
   - Note for the PR body: attaching a rejection handler changes jsse's behavior for `async-fs` (the same script *completes* once any reaction is attached — see #679), so `async-fs` may flip to PASS after this change without the engine bug being fixed. That is expected and is **not** evidence #679 is resolved; the PR must say so.

4. **Failure output is always retained**
   - Red: fake-engine tests through `run_benchmark_once`: (a) exit 0, no stdout, stderr `"warn"` → `status == "error"`, result has both `stdout` and `stderr` keys, `stderr == "warn"`; (b) async benchmark that never settles (fake engine exits 0 with empty output) → reason states the async harness never settled (no result, no error reported), distinct from the sync "no JSON output"; (c) exit 1 with both streams → both kept; (d) an oversized stream is clipped to a bounded size with head and tail kept and a truncation marker.
   - Green: small `_clip(text, limit=2000)` helper and a single `_failure(name, reason, result, elapsed)` constructor used by the exit-code, no-JSON and error-line paths (replaces the three hand-built dicts); `-v` prints stderr/stdout on the no-JSON path too. `process_result()` prints the first non-empty line of the error/stderr after the reason.
   - Timeout results are unchanged (out of scope, see §7).

5. **End-to-end vertical test**
   - Red: `test_async_benchmark_with_preload_and_self_runs_end_to_end`: temp JetStream dir with `bench.js` (`class Benchmark { async init() { if (self !== globalThis) throw new Error("no self"); this.text = await JetStream.getString(JetStream.preload.blob); } runIteration() { if (!this.text.length) throw new Error("empty"); } }`) and a preload file; `run_benchmark_once(...)` against the real engine (skip if only node is available? node lacks jsse's semantics but the shim is plain JS, so node is acceptable) returns `status == "pass"`.
   - Green: already satisfied by slices 1–3; this slice guards the wiring in `run_benchmark_once` (preload code is emitted before the sources, harness after).

6. **Docs + real-workload verification (no new code)**
   - README paragraph (see §3).
   - Build the engine for verification only: `cargo build --release -j4` with an explicit long timeout (no Rust changes, so nothing else in the tree moves). Then, from the workspace, with the issue's flags:
     `uv run python scripts/run-jetstream.py --engine target/release/jsse --jetstream /tmp/JetStream --test bigint-paillier,Babylon,first-inspector-code-load,multi-inspector-code-load,bigint-noble-secp256k1,bigint-noble-ed25519,async-fs,bigint-noble-bls12-381 --iterations 1 --timeout 120 --no-idle-gate --json $TMPDIR/jetstream-655.json`
     Expect 7 PASS. `async-fs` may PASS, or FAIL with the now-visible `TypeError ... byteLength`, or report "never settled"; whichever it is, record it in the PR body and link #679. Do not write into `jetstream-results.json`.
   - Also run `bigint-noble-bls12-381` once at its default iteration count (4 × ~13 s ≈ 53 s per measurement, ×3 repeats sequential) to confirm it fits inside the per-measurement timeout.
   - Gates, as separate commands: `uv run python -m unittest discover -s scripts -p 'test_*.py'`; `ruff check scripts/` and `ruff format --check scripts/` (pre-commit runs ruff; use `uvx ruff` if not installed); no `cargo` gate needed since `src/` is untouched (a plain `cargo build --release` is only for the manual runs above).

## 5. Test surface

- test262: none. No engine, parser, or builtin file changes; `test262-pass.txt` and `test262-extra/` are unaffected. Not planned: any `test262/test/...` run.
- Custom: none under `tests/` or `test262-extra/` (no JS-visible engine behavior). The new coverage lives in `scripts/test_benchmark_protocol.py`, which is the gate that actually covers the runner (`uv run python -m unittest discover -s scripts -p 'test_*.py'`, CI `ci.yml:61`).
- Manual: the 8-workload run in slice 6; `--bytecode` is not needed (issue reports identical failure; the fix is runner-side).
- `scripts/run-node-shim-selftest.sh` / `run-shim-fixtures.sh` / `run-library-tests.sh`: unaffected (the runner does not share code with those shims; do not add `self` to `node-shim.js`).

## 6. Regression risk

- `test262-pass.txt` baseline: cannot move (no `src/` change; not planning `--update-baseline`).
- Shared machinery leaned on: none of the tree-walker hot paths, `property.rs`, GC, `ObjectKind`, or the bytecode path is touched. The only shared surface is the *runner*: `build_polyfill_preamble()` and the harnesses run for **every** one of the 48 benchmarks, so:
  - the new prelude/harness text must keep passing the #673 hygiene test (no top-level bindings) — otherwise workloads with colliding top-level names (`benchmark`, `__iterations`, …) regress to an early SyntaxError;
  - defining `self` could change behavior of the ~10 sources that mention `self` (`doxbee-*`, `proxy-*`, `pdfjs`, `typescript-octane`, …). Most use it as a local name, but any that feature-detect `self` (browser-vs-shell probing) will now take the browser-ish branch — the same branch JetStream's own shell driver gives them, so this is correct, but re-run `doxbee-promise,doxbee-async,proxy-mobx,proxy-vue,pdfjs,typescript-octane` at `--iterations 1` before/after and diff PASS/FAIL to confirm no change.
  - `.catch` on the async harness changes engine-visible ordering (an extra reaction on the harness promise). Re-run two or three currently passing async workloads (`doxbee-async`, `promises`-style ones) before/after to confirm scores are unaffected within noise.
- Timeout risk (state the outcome in the PR): the newly-running workloads move from a fast FAIL to real runs. At default iteration counts the tightest fits are `bigint-noble-secp256k1` (120 iterations, ~1 s each), `async-fs` (80 iterations, ~2 s each) and `Babylon` (120 iterations, ~0.8 s each); at `--timeout 120` these may land as TIME rather than PASS. A TIME result is more honest than the old FAIL and is not a regression, but it must be reported, not hidden; do not lower iteration counts to make them fit.
- Report-format compatibility: `--json` consumers (`--compare`, baseline files) read `status`, `scores`, `name`; adding `stderr`/`stdout`/`reason` keys to error results is additive.

## 7. Out of scope

- The `async-fs` engine defect (#679): no engine fix, no GC changes, no attempt to diagnose further here.
- Making jsse report unhandled promise rejections to stderr / exit non-zero (a host/CLI design decision, would have made this issue self-diagnosing; noted in #679 as a separate usability gap — file separately if wanted).
- Retaining output on `timeout` results (`subprocess.TimeoutExpired` buffers) — same idea, different failure shape; follow-up.
- Emulating JetStream's `top` object (`currentResolve/currentReject`), Workers, `isInBrowser`/`isD8`, `getBinary` for real binary preloads, `dynamicImport`, or file `read`/`load` host functions (`mandreel`, `pdfjs`, `typescript-octane`, `ML` mention them; tracked elsewhere, e.g. #54).
- Switching the runner to drive JetStream's own `cli.js` (would need `load`/`readFile`/`runString`/`Realm`-style host functions jsse does not have).
- The hardcoded `dir="/tmp"` for the generated temp script, formatting changes, and any refactor of `run_benchmark`/`main` beyond the `_failure` helper.
- Any `test262-pass.txt` update, `spec/` or `test262/` change.
