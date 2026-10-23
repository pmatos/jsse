---
name: fuzzing
description: >-
  This skill should be used when the user asks to "fuzz the parser",
  "run cargo fuzz", "fuzz jsse vs node", "check for fuzzer crashes",
  "triage a fuzz crash", "minimize a fuzz artifact", or wants to drive
  a cargo-fuzz run or triage a crash/divergence it found. Covers both
  the parse, execute, typed_array, execute_ast and differential targets under fuzz/.
version: 0.1.0
---

# Fuzzing

Five `cargo-fuzz` targets live under `fuzz/`: `parse` (parsing
arbitrary bytes must never panic), `execute` (run in-process under a step
budget on the bytecode VM and the tree-walker; panics and differences between
the two are findings), `typed_array` (the same oracle over generated
ArrayBuffer/typed array/`DataView`/`Atomics` operation sequences, whose input is
a byte-encoded op list, not JS source), `execute_ast` (the same check on programs printed from
an `arbitrary`-decoded AST, so they parse) and `differential` (jsse vs `node` on the same source).
See `docs/adr/2026-10-23-1400-execute-fuzz-target.md` for `execute`,
`docs/adr/2026-10-23-1600-typed-array-fuzz-target.md` for `typed_array`,
`docs/adr/2026-10-23-1630-execute-ast-fuzz-target.md` for `execute_ast`, and `docs/adr/0004-fuzz-lib-target-and-subprocess-differential.md`
for why they're built the way they are, and the `Divergence Tier` entry
in `CONTEXT.md` for the tiering `differential` uses.

This skill has two responsibilities, run separately: **drive a run**
(read-only with respect to engine code — it only ever touches `fuzz/`
build/corpus/artifact state), then, only if it finds something,
**triage the result**.

## Running a fuzz target

1. **Prerequisites:**
   - `cargo install cargo-fuzz --locked` (needs a `nightly` toolchain:
     `rustup toolchain install nightly` if not already present).
   - `differential` additionally needs `cargo build --release` — it shells
     out to `target/release/jsse`, and skips cleanly (not a crash) if that
     binary is missing. It also needs `node` on `PATH`; also skips
     cleanly if absent.
2. **Regression replay (fast, deterministic, safe to run anytime):**
   ```bash
   cargo +nightly fuzz run parse -- -runs=0 fuzz/corpus/parse
   cargo +nightly fuzz run execute -- -runs=0 -max_len=16384 fuzz/corpus/execute
   cargo +nightly fuzz run typed_array -- -runs=0 -max_len=768 fuzz/corpus/typed_array
   cargo +nightly fuzz run execute_ast -- -runs=0 -max_len=4096 fuzz/corpus/execute_ast
   cargo +nightly fuzz run differential -- -runs=0 fuzz/corpus/differential
   ```
   This replays the committed seed corpus with no new mutation — if this fails, something
   in the current tree broke a previously-clean input; that's worth
   investigating immediately, unlike a new-mutation finding (see below).
3. **New-mutation fuzzing (can find fresh, previously-unknown bugs at any
   time — this is expected, not a sign something is broken):**
   ```bash
   cargo +nightly fuzz run parse -- -max_total_time=60
   cargo +nightly fuzz run execute -- -max_total_time=60 -max_len=16384 -timeout=30
   cargo +nightly fuzz run typed_array -- -max_total_time=60 -max_len=768 -timeout=30
   cargo +nightly fuzz run execute_ast -- -max_total_time=60 -max_len=4096 -timeout=30
   cargo +nightly fuzz run differential -- -max_total_time=60 -timeout=30
   ```
   `parse` runs at libFuzzer's normal in-process speed; `execute` runs two
   interpreters per input, about 200 executions/sec under ASan; `typed_array`
   does the same but with larger generated programs, about 10 executions/sec;
   `execute_ast` runs only parseable programs, so each input pays two full
   interpreter setups and runs at roughly a fifth of `execute`'s rate.
   `differential` is much slower (two subprocess spawns per iteration,
   tens of iterations/sec, not thousands) by design — see the ADR. Budget
   accordingly; CI's nightly `Fuzz` workflow gives `differential` 5 minutes.
4. **A crash writes an artifact** to `fuzz/artifacts/<target>/crash-<hash>`
   (or `fuzz/artifacts/<target>/oom-<hash>` / `timeout-<hash>`) and prints
   its path. That file is the input to triage below.

## Where CI runs each target

- `parse`, `execute`, `typed_array` and `execute_ast`: ClusterFuzzLite under AddressSanitizer. `clusterfuzzlite-pr.yml`
  is a blocking per-PR check (120 s of fuzzing, shared between the targets); `clusterfuzzlite-batch.yml`
  runs 30 minutes daily and stores the merged corpus as a GitHub Actions
  artifact that later batch and PR runs start from.
- `differential`: `fuzz.yml`, nightly, 5 minutes, no sanitizer (needs `node`
  and `target/release/jsse`).

### Triaging a ClusterFuzzLite finding

1. Open the failed `ClusterFuzzLite PR` or `ClusterFuzzLite batch` run. The
   log shows the ASan report and testcase path; the `clusterfuzzlite-*-crashes`
   artifact holds the testcase file(s).
2. Download it (`gh run download <run-id> -n clusterfuzzlite-pr-crashes`) and
   reproduce locally: `cargo +nightly fuzz run --debug-assertions parse <testcase>`
   (cargo-fuzz uses ASan by default; CI builds with `--debug-assertions`, so
   overflow panics only reproduce with it), then
   `cargo +nightly fuzz tmin --debug-assertions parse <testcase>`.
3. Decide if the PR caused it: re-run against `main`'s commit. A crash that
   also reproduces on `main` is a latent bug unrelated to the PR (the
   continuous build normally filters these out, but not every one); file it
   under "Root-cause and file" below and re-run the check.
4. To seed a local session with the stored corpus, download the latest corpus
   artifact from the batch workflow's runs (`gh run list -w "ClusterFuzzLite batch"`,
   `gh run download <run-id>`) and pass the extracted directory to
   `cargo +nightly fuzz run parse`.

## Triaging a finding

**First, tell the two failure shapes apart** — they need different next steps:

- **`parse` crashed** (a real panic/abort — this target has no
  other way to fail): always worth investigating. Go to "Root-cause and
  file" below.
- **`execute` failed**, one of:
  - a panic, abort or ASan report: same flow as `parse`.
  - a panic starting `execution divergence`: the bytecode VM and the
    tree-walker disagree (an "Execution Divergence", defined in the `execute` ADR). The
    message shows both signatures. Reproduce with the CLI in both modes and
    diff stdout and the exit code: `jsse f.js` vs `jsse --no-bytecode f.js`.
    The CLI differs from the fuzz runner (real timers and `Math.random`), so a
    script using those may need the fuzz run to reproduce. Minimize with
    `tmin`, then decide against `spec/` which mode is wrong. File one issue per
    root cause. If the difference is spec-unspecified (for example error
    message wording) the fix is to normalise it in `RunSignature` with a
    comment naming the unspecified clause, never to hide a spec-defined one.
  - on `execute_ast` the artifact holds `Arbitrary` bytes, not source. Print
    the source with a throwaway test calling
    `jsse_fuzz::js_print::print(&Program::arbitrary(&mut Unstructured::new(&bytes))?)`
    (see `fuzz/tests/ast_target.rs`), then triage and `tmin` that source through
    `execute`. Fix generator rules in `fuzz/src/js_print*.rs`, never in the
    engine, when the printer emitted an early error.
  - a libFuzzer `timeout-*` or `oom-*` artifact: work the step budget does not
    count (regex backtracking, huge `repeat`, some arrow-function loops). A
    real missing resource limit; file it.
  - Inputs that exhaust the step budget are skipped, never a finding.
- **`typed_array` failed**: same shapes as `execute`. The artifact is a byte-encoded
  op list, so print the script it generates with
  `JSSE_FUZZ_DUMP_SCRIPT=1 cargo +nightly fuzz run typed_array <artifact>` and
  triage that JS (`jsse f.js` vs `jsse --no-bytecode f.js`; the script needs
  `print` and `$262`). `tmin` works on the byte input.
- **`differential` panicked**: the panic message says which tier fired.
  Only Tier 1 and Tier 2 panic (see the `Divergence Tier` glossary entry);
  Tier 3 is recorded, never a panic, so if `differential` panics at all,
  it's already a Tier 1 or Tier 2 finding, not noise.

### Reproduce and minimize

```bash
cargo +nightly fuzz run <target> fuzz/artifacts/<target>/crash-<hash>
cargo +nightly fuzz tmin <target> fuzz/artifacts/<target>/crash-<hash>
```
`tmin` writes a smaller `minimized-from-*` artifact that reproduces the
same failure — use that (not the original, often much larger) input in
the issue report.

For `parse`, also reproduce outside the fuzzer to get a full
Rust backtrace, since the release build used in CI/most local runs has
overflow-checks off and won't panic on the same input a debug build will:
```bash
cargo build   # dev profile: overflow-checks on
RUST_BACKTRACE=1 ./target/debug/jsse <minimized-input-file>
```

### Root-cause and file — never fix here, and never fix toward `node`

This is the rule this whole tooling exists to enforce, so it's worth
stating plainly: **a fuzzer finding is a lead, not a patch to make in
whatever session found it.**

1. Read the relevant `spec/` clause for the construct involved. The spec
   is authority #1; test262 is #2; `node`'s behavior (for a `differential`
   finding) is a *data point for reproducing the divergence*, never a
   justification for what the fix should do. Matching `node`'s observed
   behavior instead of the spec is not a fix — see `AGENTS.md`'s
   authority order.
2. `gh issue create` with: the minimized repro, the exact command used to
   trigger it, the tier (for `differential`) or panic message/backtrace
   (for `parse`), and — if you have a hypothesis — what looks
   wrong, without asserting the fix. Link back to whichever issue drove
   the fuzzing session, if any.
3. Stop there. The fix (if one is warranted) is its own PR, scoped to the
   engine behavior, with its own test262/`test262-extra/` coverage — not
   folded into a tooling change, a triage session, or a batch of unrelated
   fixes.

### Tier 3 is usually not worth filing

A Tier 3 `differential` sample (both sides threw, different error class;
or both timed out) is almost always "jsse hasn't implemented this yet" or
a host-global gap (`print`/`gc`/`$262` shape differences from the node
prelude, `scripts/node-test262-prelude.js`). Only promote one to a filed
issue if reading the actual stderr from both sides shows something that
looks like a genuine spec violation rather than an expected gap — most
don't, and filing every Tier 3 sample would just create noise.

## Comparing target coverage

To compare two targets on the same budget, run each into its own scratch
directory with a fixed seed, seeded and from an empty corpus, and compare
libFuzzer's final `cov:` edge count (all targets link the same engine, so the
counts are comparable). Budget by executions (`-runs=10000`) and by wall time
(`-max_total_time=300`) because `execute_ast` pays two interpreter setups per
valid program:

```bash
D=$(mktemp -d)
cargo +nightly fuzz build -O --debug-assertions execute execute_ast
B=fuzz/target/x86_64-unknown-linux-gnu/release
$B/execute     $D/ex  fuzz/corpus/execute     -max_len=16384 -runs=10000 -seed=1 -timeout=30
$B/execute_ast $D/ast fuzz/corpus/execute_ast -max_len=4096  -runs=10000 -seed=1 -timeout=30
```

## Corpus hygiene

`fuzz/corpus/<target>/` holds the *committed* seed corpus (copied from
`tests/*.js` and `test262-extra/*.js`; `execute` also carries ~40 test262 tests
generated by `scripts/gen-fuzz-execute-corpus.py` with the harness inlined, curated small for `differential`
since it pays a subprocess-spawn cost per seed). Running `cargo fuzz run`
without `-runs=0` writes newly-discovered inputs into this same directory
by default — expect `git status` to show new untracked files after a
local new-mutation session. That's normal; nothing here separates "seed"
from "discovered" corpus into different directories. Don't commit
discovered-corpus growth casually — if a specific discovered input is
worth keeping as a regression seed (e.g. after fixing the bug it found),
add it deliberately, not as a side effect of `git add -A`. ClusterFuzzLite's
corpus lives in a CI artifact, not in the tree.
