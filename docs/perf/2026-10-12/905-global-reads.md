# #905 — global data fast path for free-variable reads and writes

Measured on a shared 24-core Linux host at load average 10-38 (no idle host
was available), release builds, **interleaved** runs of the two binaries, CPU
time (user+sys) of the child process, 20 runs each; p10/p25/p50 are reported
because the minimum and the mean were both dominated by host contention.
`base` is the parent commit; `after` is this branch. Only ratios taken in one
session are meaningful — absolute seconds are not comparable with the buildbox3
numbers in the issue. Repros are `docs/perf/2026-10-10/repros/`.

## Micro-benchmarks (default, bytecode)

Two sessions, quieter first (load 9-20) then loaded (load 33-38):

| benchmark | session | base p25 / p50 (s) | after p25 / p50 (s) | after/base (p50) |
|---|---|---:|---:|---:|
| `bench_numeric_globalvars` | quiet | 0.79 / 0.83 | 0.35 / 0.37 | 0.44 |
| `bench_numeric_globalvars` | loaded | 1.15 / 1.36 | 0.51 / 0.57 | 0.42 |
| `bench_numeric_typedarray_global` | quiet | 0.54 / 0.55 | 0.40 / 0.41 | 0.74 |
| `bench_numeric_typedarray_global` | loaded | 0.79 / 0.81 | 0.60 / 0.76 | 0.94 |
| `bench_numeric_typedarray_local` (control) | quiet | 0.31 / 0.34 | 0.34 / 0.36 | 1.07 (noise) |
| `bench_calls_global_loop` | loaded | 0.60 / 0.75 | 0.41 / 0.53 | 0.71 |

Acceptance (issue):

- `bench_numeric_globalvars` ≤ 2.3 s of 3.18 s, i.e. ≤ 0.72× base: **met**
  (≈ 0.42-0.44×).
- `bench_numeric_typedarray_global` within 1.15× of `typedarray_local`: the
  ratio is 1.5-1.65× on base and **1.12-1.2× after** depending on host load
  (quiet session 0.41 / 0.36 = 1.14 at p50; loaded session 0.76 / 0.63 = 1.21).
  The remaining gap is the scope walk (a frame probe plus the global
  environment probe) that a `LoadLocal` slot never pays; removing it needs the
  static-hop-count work listed under "Not done".

Generic property traffic did not move (the `PropertyMap` spilled storage change
touches every object with more than four properties), p25 / p50, loaded host:

| benchmark | base (s) | after (s) |
|---|---:|---:|
| `bench_object` | 0.40 / 0.43 | 0.39 / 0.43 |
| `bench_json` | 0.11 / 0.14 | 0.11 / 0.13 |
| `bench_array` | 0.24 / 0.25 | 0.21 / 0.22 |

Startup (`jsse -e 1`) is unchanged within noise (7-10 ms).

## Deterministic counters (perf-counters build, never timed)

`bench_numeric_globalvars` — 26.5 M VM ops:

- `global_prop_fast_hits` 8,000,004 (6.5 M `LoadName` + 1.5 M
  `StoreResolvedName`), `global_prop_fast_misses` 0.
- `atom_intern_calls` 101 and `atom_lookup_calls` 351 for the whole run, all
  from startup.

`bench_numeric_typedarray_global` — 44.0 M VM ops: 3.6 M `LoadName`, 3,600,005
fast hits, 0 misses.

`tests/perf_counters_global_access.rs` asserts that the hit counter scales with
the iteration count, that the fallback counter does not, and that an accessor
global is never counted as a hit.

## What changed, in cost terms

- A read used to do: frame probe, `resolve_identifier` loop with an `Rc` clone
  per hop, one randomised-SipHash probe of the spilled global `PropertyMap`
  and a cloned `PropertyDescriptor`. It now does: one clone-free two-scope
  walk (name filter mask test per scope) and one slot-verified entry read.
- A write used to do: `find_binding_env` walk, `env_check_set_binding` walk +
  `own_has_property`, `proxy_has_property`, a full `proxy_set` [[Set]] and a
  realm scan in `sync_global_object_binding` — roughly six hashes. It now does
  the walk and one slot-verified in-place store, with the GC write barrier.
- `x++` on a global resolves the reference once with the same hint and reuses
  it for the read and the write.

## Not done

- Skipping the intermediate scope walk by a static hop count, which needs a
  "dynamic scope seen" (direct `eval` var injection / `with`) poison flag and a
  static scope-structure argument per function literal.
- A shape-keyed IC (see ADR-2026-10-12-1500 for why it was not used).
