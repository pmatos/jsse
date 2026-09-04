---
name: fuzzing
description: >-
  This skill should be used when the user asks to "fuzz the parser",
  "run cargo fuzz", "fuzz jsse vs node", "check for fuzzer crashes",
  "triage a fuzz crash", "minimize a fuzz artifact", or wants to drive
  a cargo-fuzz run or triage a crash/divergence it found. Covers both
  the parse_roundtrip and differential targets under fuzz/.
version: 0.1.0
---

# Fuzzing

Two `cargo-fuzz` targets live under `fuzz/`: `parse_roundtrip` (parsing
arbitrary bytes must never panic) and `differential` (jsse vs `node` on
the same source). See `docs/adr/0004-fuzz-lib-target-and-subprocess-differential.md`
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
   cargo +nightly fuzz run parse_roundtrip -- -runs=0 fuzz/corpus/parse_roundtrip
   cargo +nightly fuzz run differential -- -runs=0 fuzz/corpus/differential
   ```
   This is exactly what CI's `fuzz-smoke` job runs on every PR. It replays
   the committed seed corpus with no new mutation — if this fails, something
   in the current tree broke a previously-clean input; that's worth
   investigating immediately, unlike a new-mutation finding (see below).
3. **New-mutation fuzzing (can find fresh, previously-unknown bugs at any
   time — this is expected, not a sign something is broken):**
   ```bash
   cargo +nightly fuzz run parse_roundtrip -- -max_total_time=60
   cargo +nightly fuzz run differential -- -max_total_time=60 -timeout=30
   ```
   `parse_roundtrip` runs at libFuzzer's normal in-process speed.
   `differential` is much slower (two subprocess spawns per iteration,
   tens of iterations/sec, not thousands) by design — see the ADR. Budget
   accordingly; CI's nightly `fuzz-deep` job gives each target 5 minutes.
4. **A crash writes an artifact** to `fuzz/artifacts/<target>/crash-<hash>`
   (or `fuzz/artifacts/<target>/oom-<hash>` / `timeout-<hash>`) and prints
   its path. That file is the input to triage below.

## Triaging a finding

**First, tell the two failure shapes apart** — they need different next steps:

- **`parse_roundtrip` crashed** (a real panic/abort — this target has no
  other way to fail): always worth investigating. Go to "Root-cause and
  file" below.
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

For `parse_roundtrip`, also reproduce outside the fuzzer to get a full
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
   (for `parse_roundtrip`), and — if you have a hypothesis — what looks
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

## Corpus hygiene

`fuzz/corpus/<target>/` holds the *committed* seed corpus (copied from
`tests/*.js` and `test262-extra/*.js`, curated small for `differential`
since it pays a subprocess-spawn cost per seed). Running `cargo fuzz run`
without `-runs=0` writes newly-discovered inputs into this same directory
by default — expect `git status` to show new untracked files after a
local new-mutation session. That's normal; nothing here separates "seed"
from "discovered" corpus into different directories. Don't commit
discovered-corpus growth casually — if a specific discovered input is
worth keeping as a regression seed (e.g. after fixing the bug it found),
add it deliberately, not as a side effect of `git add -A`.
