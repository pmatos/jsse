# Fuzzing: a lib crate target, and a subprocess-based differential target

jsse had no fuzz testing (issue #193, following up on the DevX audit in
#192). test262 and the library harnesses are corpus-based regression
suites — they only exercise inputs someone already wrote down. A
coverage-guided fuzzer finds parser panics and lexer/parser divergences
from `node` that no fixed corpus produces, which directly serves the
100% test262 goal. Doing this needed two structural decisions this ADR
records; everything else about the tooling is downstream of them.

## `src/lib.rs`: a crate root, not just a bigger binary

The crate was binary-only (`src/main.rs` with private `mod` declarations),
so there was no library target for `cargo fuzz`'s libFuzzer harness to
link against. `src/lib.rs` now re-exports `ast`/`interpreter`/`lexer`/
`parser`/`types` as `pub mod`; `main.rs` consumes them via `use jsse::{...}`
instead of declaring its own `mod`s. No behavioral change — the modules
moved, not their contents.

One consequence: clippy's public-API lints (`len_without_is_empty`,
`new_without_default`, `should_implement_trait`) only fire once an item
is genuinely reachable from outside the crate, which nothing was before
(a `mod interpreter;` in a binary crate isn't reachable from anywhere,
regardless of the `pub` on individual items inside it). Making the crate
root `pub mod` surfaced nine of these; fixed with `is_empty()`/`Default`
impls where trivial, `#[allow(clippy::should_implement_trait)]` (matching
the existing `#[allow(clippy::wrong_self_convention)]` precedent in this
codebase) on the three `from_str`-named methods, since renaming a public
method has call-site cost with no correctness benefit here.

## `differential` runs compiled binaries as subprocesses, not the interpreter in-process

The obvious design for "compare jsse and node on the same source" is to
link the interpreter into the fuzz target and call it directly, the way
`parse_roundtrip` calls the parser directly. We didn't do that, for one
concrete reason: the interpreter has no step/instruction budget or
interrupt mechanism (grep-confirmed: no `step_budget`, `instruction_count`,
or similar). A fuzzer will eventually produce `while(1){}`, and an
in-process target fed that hangs the entire libFuzzer process — not a
"crash" it can detect and move past, but a wedged process an operator has
to notice and kill by hand.

Running `target/release/jsse` and `node` as subprocesses turns a hang into
a normal, cheap "kill and skip" outcome on either side: each gets its own
wall-clock timeout (raced via `try_wait()` polling, since
`std::process::Command` has no built-in one), and a timeout classifies as
`Verdict::Recorded` rather than gating anything. It also means the
differential target never needs interpreter internals exposed — no
`CALL_DEPTH_*`/GC internals need to be `pub` for it, unlike `parse_roundtrip`
which already needed `run_on_engine_stack` and the parser to be reachable.

The trade-off, stated plainly rather than glossed over: throughput is
bounded by two process spawns per iteration (tens of ms each), so this
target runs at tens of iterations/sec, not the thousands/sec libFuzzer
normally achieves fuzzing in-process. That's why it's a `workflow_dispatch`/
`schedule` (nightly) target only (`fuzz-deep` in `.github/workflows/fuzz.yml`),
never the `pull_request` gate.

### Resource limits mirror `scripts/run-test262.py`, but asymmetrically

Both subprocesses run under `prlimit --as=<limit> --` rather than
reimplementing `setrlimit` via `pre_exec` from Rust (no new dependency
needed; `prlimit(1)` is already on the CI image and any Linux dev box).
The limits themselves are **not** the same for both engines, and this was
verified empirically, not assumed: `prlimit --as=536870912 node -e 1`
(jsse's 512 MiB test262 cap) fatally OOMs node in V8's `NewIsolate` before
running anything — V8 reserves a large virtual address range for
`CodeRange` at startup regardless of how much the script actually uses.
`scripts/run-test262.py`'s `NodeAdapter` already uses 4 GiB for exactly
this reason; `fuzz/src/lib.rs` reuses the same two constants
(`JSSE_AS_LIMIT` = 512 MiB, `NODE_AS_LIMIT` = 4 GiB) rather than picking
new ones.

### Divergence tiers

Not every difference between jsse and node's output is a finding — most of
the space is "jsse hasn't implemented some feature yet," which is
expected and not what fuzzing is for here. `classify()` sorts every run
into three tiers (also documented in `CONTEXT.md`):

- **Tier 1** (`panic!`, a libFuzzer finding): jsse crashed — killed by a
  signal, or exited with the interpreter-panic code 101 — while node did
  not crash the same way. An engine bug by definition, independent of
  what node does.
- **Tier 2** (`panic!`): a parse accept/reject mismatch — one side treats
  the source as a `SyntaxError` and the other parses it successfully.
  Surfaces real syntax coverage gaps.
- **Tier 3** (recorded, not a finding): both sides threw (possibly a
  different error class), or both timed out. Dominated by unimplemented
  features and host-global differences (`print`, `gc`, `$262`); gating on
  this would drown real findings in noise. The fuzzing skill's triage step
  is where a human or agent decides whether a specific Tier 3 sample is
  worth filing anyway.

Error class is extracted from stderr by scanning every line (not just the
first) for a `SomeError:`-shaped prefix: node prints the failing source
line and a `^` caret *before* the actual `Error: message` line, so "first
line of stderr" is not the error class.

### Triage discipline: the spec decides, never node

Any bug either target surfaces is a **lead**, not a fix to make inside
this tooling. It gets filed as its own issue and, if fixed, fixed against
`spec/`/test262 — never by matching whatever `node` happened to do, and
never inside the PR that only builds the tooling. The `differential`
target's own name is a reminder that node is a comparison oracle for
*finding* divergences, not an authority on what the *correct* behavior
is (see `AGENTS.md`'s authority order: spec, then test262, then node).
This surfaced in practice, not hypothetically: a 20-second local
new-mutation run against `parse_roundtrip` — no sanitizer, no special
seed — found a real integer-underflow panic in the parser (filed as
issue #597) while this tooling was being built. It was filed and left
unfixed here, and its existence is also why `fuzz-smoke`'s PR gate is
corpus-replay-only (`-runs=0`) rather than running any new-mutation pass:
a mutation pass can hit this exact bug (or another latent one) on any
future PR, at random, which would make the fuzz gate itself flaky for
reasons that have nothing to do with the PR under test.

## Deliberately out of scope

- **Module-source (`.mjs`) fuzzing.** The seed corpus and `parse_roundtrip`
  cover scripts only; a fuzzed module can `import` an arbitrary
  nonexistent path, which adds resolution complexity this round doesn't
  need to solve.
- **A true AST-print-and-reparse roundtrip.** `parse_roundtrip` is
  parse-must-not-panic only; there's no `Display`/`to_source` for
  `ast::Program` to roundtrip through yet (grep-confirmed).
- **Wiring `fuzz/` into `scripts/lint.sh`.** The fuzz crate's `#![no_main]`/
  `libfuzzer-sys` idioms don't necessarily suit the same lint config as
  the main crate; `cd fuzz && cargo fmt --check && cargo clippy --all-targets
  -- -D warnings` was run manually during implementation and is clean, but
  isn't wired into the automated gate.
- **`ASan`/other sanitizers in `fuzz-smoke`.** `--sanitizer none`: `unsafe`
  usage is small (29 hits, grep-confirmed) and ASan roughly doubles build
  time on an already `icu`-heavy dependency tree. `fuzz-deep` could opt in
  later if UB detection beyond debug-assertions/overflow-checks turns out
  to matter.
- **An in-process differential target with an interpreter step budget.**
  Would let `differential` run at libFuzzer speed instead of subprocess
  speed, but needs a real engine feature (an instruction/step budget) with
  its own spec/perf implications, well beyond this issue.
- **A committed `fuzz/corpus/<target>/`-is-pristine guarantee.** `cargo
  fuzz run <target>` without `-runs=0` writes newly-discovered inputs into
  whatever corpus directory it's given, which defaults to
  `fuzz/corpus/<target>/` — the same directory the seed files are
  committed in. A local fuzzing session growing that directory is expected
  and shows up as untracked files in `git status`; nothing here tries to
  separate "seed" from "discovered" corpus into different directories.
