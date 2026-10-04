# tweetnacl-js re-check: raising the sampled vector caps (issue #361)

Generated 2026-10-04. Follow-up to
[`docs/perf/2026-09-05/tweetnacl-bytecode-null-result.md`](../2026-09-05/tweetnacl-bytecode-null-result.md),
which found `--bytecode` gave a null result on this workload (1.00-1.07x) and
split the root cause off as #603 (bytecode `new` expressions and compound
member-target assignment, both unsupported at the time). #603 has since
landed. This re-check asks the two questions #361 is actually about: how much
faster is the *default* (non-bytecode) path the harness runs today, and does
that let the sampled corpus's vector caps go up.

## Headline: the default path is real end-to-end, not projected

The 2026-09-05 doc's ~22min "sampled" and ~6h "full upstream" figures were
**both projections** — per-op costs multiplied out, never run end to end (its
own scope note says so explicitly). This time both endpoints are real
measurements:

| run | caps | measured wall time (jsse, cross-checked vs Node) |
| --- | --- | --- |
| today's shipped corpus | 20 / 20 / 20 | **9m25.7s** (5,470 assertions, PASS) |
| raised corpus (this PR) | 256 / 256 / 256 of 1024 | **50m15.2s** (7,362 assertions, PASS, `--clean`) |

The 9m25.7s figure alone already falsifies the old ~22min projection — by
~2.3x — confirming the engine is faster than even the projection assumed,
before any caps changed.

**The 50m15.2s figure includes incidental CPU contention.** This build host
runs several concurrent agent sessions. Two confirmed sources overlapped
this validation run: this session's own `./scripts/lint.sh` (a `cargo clippy`
pass, run concurrently by mistake while checking in on progress) completed
during the `scalarmult.random.js` phase, which measured 18m1s against the
~13m52s (256 × 3.25s/vector) the contention-free per-vector rate predicts —
a ~4min, ~30% overshoot consistent with that interference; `box.random.js`
and `sign.spec.js`, timed after that lint run had finished, matched their
predicted per-vector costs closely. Separately, a `ps aux` snapshot taken
near the end of the run found an unrelated worktree (a different issue's
session) running a full `test262 -j32` pass with 32+ concurrent jsse
processes — present for at least part of the run, though its exact overlap
with specific phases wasn't captured. 50m15.2s is reported as-measured
rather than corrected for either source — it's what actually happened and it
still passed — but it should be read as this workload's wall time on a busy
shared host, not its floor.

## Where the speedup comes from

Two independent things changed since 2026-09-05:

1. **#603 landed** — the bytecode compiler now accepts `new` expressions and
   compound assignment to a member target (`o[i] += x`). Both were previously
   whole-body bails.
2. **General tree-walker throughput improved** — unrelated to `--bytecode`;
   this is what the *default* path (what the harness actually runs) rides on.

Real per-operation costs, read directly off each run's own timestamped TAP
test blocks (not a separate isolated microbench — this is exactly the code
path the harness executes). The pre-change (20-vector) run gives a clean,
uncontended baseline; the raised-corpus (256-vector) run gives a second,
independent sample at 12.8x the vector count, measured under the contention
noted above — included to show it's in the same range, not to re-derive the
per-op cost from it:

| phase | 2026-09-05 (min of 3 reps) | 2026-10-04, n=20 (this PR's baseline run) | 2026-10-04, n=256 (raised-corpus run) | speedup (20-vector) |
| --- | --- | --- | --- | --- |
| `scalarMult.base` (200-iter KAT, fixed cost) | 2.98 s/op | 0.72 s/op | 0.72 s/op (same fixed loop) | **4.1x** |
| `scalarmult.random.js` vectors (scalarMult.base + scalarMult mix) | — | 0.81 s/op | 1.06 s/op (contended, see above) | — |
| `box.random.js` vectors (box + box.open) | — | 0.80 s/op | 0.83 s/op | — |
| `sign.spec.js` vectors (sign + open) | — | 2.15 s/op | 2.44 s/op | — |

The n=256 column is consistently a bit higher across all three — expected,
given the confirmed contention — rather than scaling non-linearly, which is
what would indicate the per-op cost model itself breaking down at a larger N.

## The two #603 gaps are closed — a third, narrower one remains

Re-running the `perf-counters` build on the same `scalarMult.base` phase:

| metric | default | `--bytecode` |
| --- | --- | --- |
| VM opcodes dispatched | 0 | 153,948,180 |
| tree-walker work units | 141,692,053 | 31,770,832 |
| body dispatches — compiled | 0 | 79,779 |
| body dispatches — AST fallback | 118,649 | 38,870 |
| script compiles — ok / bail | 0 / 0 | 36 / 21 |

`--bytecode` now displaces **109,921,221 of 141,692,053 work units — 77.6%**,
versus 3.3% on 2026-09-05. `M` (the GF(2^255-19) multiply, `new
Float64Array(31)` at its top) and `sel25519` (compound-assignment-only) no
longer appear in the bail table at all — both now compile cleanly, which is
exactly what #603 was supposed to buy.

What's left is narrower and more concentrated than before:

| minified | tweetnacl name | work units remaining | share of what's left | bail reason |
| --- | --- | --- | --- | --- |
| `C` | `car25519` — carry propagation | 31,336,617 | **98.63%** | `call callee` |

`car25519`'s loop body is now *all* compound assignment (`o[i] += 65536`,
`o[i] -= c * 65536`) — which #603 fixed — **except** one call in the middle:

```js
function car25519(o) {
  for (var i = 0; i < 16; i++) {
    o[i] += 65536;
    var c = Math.floor(o[i] / 65536);   // <- the one unsupported construct left
    o[(i+1)*(i<15?1:0)] += c - 1 + 37*(c-1)*(i===15?1:0);
    o[i] -= c * 65536;
  }
}
```

`compile_call` (`src/interpreter/bytecode/compiler.rs:466-474`) only accepts a
bare `Expression::Identifier` callee; `Math.floor` is a member expression, so
the whole function still bails — one compiler gap now holding 98.63% of what
used to be split three ways across `M`/`car25519`/`sel25519`. Filed as
[#839](https://github.com/pmatos/jsse/issues/839).

**This doesn't affect #361 today**: the harness never passes `--bytecode`
(confirmed, no occurrence in `scripts/run-library-tests.sh` or
`scripts/libs/*.sh`), so none of the above changes what the harness measures.
It's recorded here because the re-check surfaced it as a precise, isolated
next step, the same way 2026-09-05's doc surfaced #603.

## What this means for #361

- `scalarmult.random.js` and `box.random.js` now run their full upstream
  256-vector counts, unsampled.
- `sign.spec.js` is raised from 20 to 256 of 1024 — its per-vector cost (a
  sign *and* an open/verify, scaling linearly with vector count) still makes
  1024 the long pole: projected ~74 min for that one file alone, which would
  make tweetnacl-js the harness's new long-running outlier well past the
  `esprima` precedent (~65 min) without a clear win to justify it; raising
  `LIB_TIMEOUT` to fit it is out of scope for this PR (see the PR body).
- Measured total jsse wall time at 256/256/256, `--clean`, cold cache:
  **50m15.2s** — above the ~43.4min a contention-free projection gives, for
  the reasons above. Still well inside `LIB_TIMEOUT`, which this PR raises
  from 3600s to 6000s (100min, ~2x margin over the measured figure) rather
  than leave a margin only comfortable on a quiet host.
- #361 stays open: the corpus is raised, not exhaustive. Full upstream counts
  on all three files remain gated on further engine throughput work — closing
  #839 is a plausible next lever (`car25519` is 98.63% of what's left per
  the table above), but what it's worth is unmeasured until it lands, the same
  caveat 2026-09-05's doc gave for #603.

## Method

```sh
cargo build --release                       # default-build timings
cargo build --release --features perf-counters   # counter dumps
# concatenate: a preamble setting `self`, then nacl.min.js, then bench-tweetnacl.js
jsse [--bytecode] bench.js 2>counters.txt >/dev/null
./scripts/run-library-tests.sh tweetnacl-js          # today's caps, real wall time
./scripts/run-library-tests.sh tweetnacl-js --clean  # raised caps, real wall time
```

Per-op costs in the speedup table come directly from each run's own
timestamped TAP `# <description>` headers (tape prints one per `test()`
block) rather than a separate timed invocation — the gap between consecutive
headers' timestamps gives real per-file wall time, which divided by that
file's vector count gives real per-vector cost. Wall times are from a
**default** `cargo build --release` binary; counters are from a separate
`--features perf-counters` build, timed never (per `AGENTS.md`: an
instrumented build is never timed). The counter dumps were captured
concurrently with the raised-corpus harness validation on this shared host —
safe because counts are deterministic and load-independent, unlike wall
times (which is why the n=20 baseline run, not the contended n=256 run, is
this doc's primary per-op source — see the per-op table's provenance above).
