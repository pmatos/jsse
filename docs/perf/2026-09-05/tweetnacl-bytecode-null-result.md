# Why `--bytecode` does nothing for tweetnacl's curve arithmetic (issue #361)

Generated 2026-09-05. Answers the benchmark issue #361's triage decision asked
for — whether the bytecode VM closes the perf gap enough to run tweetnacl-js's
full upstream vector counts — and names the two compiler gaps that stop it.

## Headline

**The VM does not move this workload, and the reason is coverage, not speed.**
Measured against `nacl.min.js` 1.0.3 (the build the harness bundles), min of 3
reps, deterministic inputs:

| operation | tree-walker | `--bytecode` | speedup | Node 25 | jsse/Node |
| --- | --- | --- | --- | --- | --- |
| `scalarMult.base` | 2982.20 ms | 2973.40 ms | 1.003x | 14.00 ms | 213x |
| `scalarMult` | 3010.20 | 2977.80 | 1.011x | 10.75 | 280x |
| `sign.detached` | 5184.67 | 5190.67 | 0.999x | 37.15 | 140x |
| `sign.detached.verify` | 10834.00 | 10112.67 | 1.071x | 69.75 | 155x |
| `secretbox.open` (control) | 34.09 | 33.59 | 1.015x | 0.14 | 244x |
| `hash` (control) | 70.15 | 67.41 | 1.041x | 0.18 | 390x |

Reps spread ±15% on this shared host (load average 17-80 during the run) and
several `--bytecode` reps came in slower than the default, so these are null
results, not small wins. Raw per-rep data in `timings.tsv`.

## Where the work goes

Counter dumps in `counters-default.txt` and `counters-bytecode.txt`
(`scalarMult.base`, 2 iterations). These counts are deterministic, so unlike the
wall times they are immune to the host load.

| metric | default | `--bytecode` |
| --- | --- | --- |
| VM opcodes dispatched | 0 | 5,584,177 |
| tree-walker work units | 141,692,052 | 136,962,403 |
| body dispatches — compiled | 0 | 34,081 (29%) |
| body dispatches — AST fallback | 118,649 | 84,568 (71%) |
| script compiles — ok / bail | 0 / 0 | 18 / 39 |

Enabling `--bytecode` displaces 4.73 M of 141.69 M tree-walker work units —
**3.3%** — while compiling 29% of function *invocations*. That gap between
invocation share and work share is the same trap #526 documented for mandreel
(`../2026-08-26/mandreel-bytecode-work-share.md`); here it is wider still.

## The three functions that hold everything

`BODY` rows, minified names as they appear in `nacl.min.js`:

| minified | tweetnacl | work units | share of tree-walker | share left under `--bytecode` | bail |
| --- | --- | --- | --- | --- | --- |
| `X` | `M` — GF(2^255-19) multiply | 101,449,340 | 71.60% | 74.07% | `expression:New` |
| `C` | `car25519` | 31,336,617 | 22.12% | 22.88% | `assign target` |
| `F` | `sel25519` | 2,817,810 | 1.99% | 2.06% | `assign target` |

The work-unit counts are **identical in both columns**. That is the direct
evidence the VM never reaches these functions: they do the same work whether or
not `--bytecode` is on, and the percentage only moves because the denominator
shrinks as *other* functions get compiled. Together they are 95.70% of the
tree-walker's total work, and 99.01% of what is left under `--bytecode`.

## The two gaps

**1. `new` expressions are not compiled at all.** `Expression::New` appears in
`src/interpreter/bytecode/compiler.rs` only in the bail-name table; it never
reaches a `compile_expr` arm. `M` opens with `var t = new Float64Array(31)`, and
allocating a scratch field element is the first statement of essentially every
tweetnacl field operation.

**2. Compound assignment to a member target is not compiled.** `compiler.rs`
accepts an `Expression::Member` assignment target only under
`AssignOp::Assign`; every other operator falls through to
`Unsupported("assign target")`. So `o[i] = x` compiles but `o[i] += x`,
`o[i] -= x` and `o[i] ^= t` do not — which is the whole body of `car25519` and
`sel25519`, and the accumulation loop of `M`:

```js
function car25519(o) {                       // every statement is a compound
  for (var i = 0; i < 16; i++) {             // member assignment
    o[i] += 65536;
    var c = Math.floor(o[i] / 65536);
    o[(i+1)*(i<15?1:0)] += c - 1 + 37*(c-1)*(i===15?1:0);
    o[i] -= c * 65536;
  }
}
```

Both gaps are tracked in issue #603. They are not tweetnacl-specific: any
Float64Array/Uint32Array kernel bails for the same two reasons.

The non-curve control shows the same shape rather than a curve-specific quirk —
the SHA-512 phase compiles 46% of invocations and still displaces only 4.5% of
the work.

## What this means for #361

At the measured per-op costs, and each test file's real per-vector operation
mix, the projected harness runtimes are:

| | sampled (20/20/20, today) | full upstream (256/256/1024) |
| --- | --- | --- |
| `05-scalarmult.js` fixed KAT | 9.9 min | 9.9 min |
| `05-scalarmult.js` vectors | 4.0 min | 51.1 min |
| `06-box.js` | 2.0 min | 25.7 min |
| `08-sign.js` | 5.6 min | 4.6 h |
| **total** | **~22 min** | **~6.05 h** |

So raising the caps needs roughly **17x** to hold today's runtime, or ~6x to
merely stay inside the harness's 1 h `LIB_TIMEOUT` — against the 1.00-1.07x on
offer. The sampled corpus therefore stays, as a perf-gated deferral.

Closing #603 is a **precondition, not a demonstration**: it would let the VM
reach the field arithmetic, but what the VM then delivers on typed-array numeric
code is unmeasured here, because no phase in this run had hot numeric code
executing in the VM at all. The next checkpoint is to re-run this benchmark
after #603 lands.

## Method

```sh
cargo build --release --features perf-counters
git clone --depth 1 --branch 1.0.3 https://github.com/dchest/tweetnacl-js.git
# concatenate: a preamble setting `self`, then nacl.min.js, then bench-tweetnacl.js
jsse [--bytecode] bench.js 2>counters.txt >/dev/null
```

`bench-tweetnacl.js` here is the benchmark body; it expects `nacl.min.js` to
have been concatenated ahead of it and picks a phase via the `PHASE` constant.
Inputs are fixed byte patterns rather than `nacl.randomBytes`, so both engines
do identical work and no PRNG shim is needed.

Wall times were taken from a **default** `cargo build --release` binary and the
counters from a separate `--features perf-counters` build; per `CLAUDE.md`, an
instrumented build is never timed.

**The projection is unvalidated, and one apparent check is not one.** The
harness was not re-run end to end for this work, so the ~22 min sampled figure
and the ~6 h full figure are both model output: per-op costs multiplied by each
test file's operation counts. It is tempting to check the model against the
`~11min` the harness config records for the fixed 200-iteration
`scalarMult.base` KAT, which the model puts at 9.9 min — but that `~11min`
entered the config in the same commit as the `≈3.4s` per-op figure this
measurement supersedes, and 200 × 3.4 s = 11.3 min. It is the old per-op number
restated, not an independent observation, so agreeing with it would only mean
agreeing with the measurement it came from. (The config comment is updated to
~10 min alongside this report, for the same reason.)

The one real bound is weak: the sampled corpus passes inside the harness's 1 h
`LIB_TIMEOUT`, so its true runtime is under 60 min, and ~22 min is consistent
with that. Anyone wanting a firm number should run
`./scripts/run-library-tests.sh tweetnacl-js` and time it.
