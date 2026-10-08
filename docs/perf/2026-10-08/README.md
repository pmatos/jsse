# Mandreel retest — 2026-10-08

Raw artifacts for the [issue #54 retest at `7f2abfc`](https://github.com/pmatos/jsse/issues/54#issuecomment-6055085418) and the hotspot analysis in [#873](https://github.com/pmatos/jsse/issues/873).

| file | what |
|---|---|
| `mandreel-bbox3-bytecode-120s-timeout.json` | Decision run on buildbox3 (EPYC 7501, `--bytecode`, runner `--timeout 120`): aborted on repeat 1, censored — the issue's pass/fail criterion still fails. |
| `mandreel-bbox3-bytecode.json` | Uncensored true times, buildbox3, `--bytecode`, N=3: benchmark-internal 109.1-112.9 s, process wall 191.1-196.4 s. |
| `mandreel-bbox3-default.json` | Same, default tree-walker: benchmark-internal 115.8-118.8 s, process wall 204.6-211.1 s. |
| `mandreel-local-bytecode.json` | Audit box (Ryzen AI 9 HX 370, busy loadavg ~7.5, runner-pinned), `--bytecode`, N=3: benchmark-internal ~25.9 s, process wall ~45.2 s. |
| `counters-bytecode.txt` | Full-run perf-counters dump, audit box, `--bytecode` (includes the `time` of the run at the end). VM: 417.4M ops, 32.0 ops/compiled body; AST: 1,171.2M units; BAIL led by `statement:Labeled` (56 bodies); BODY table: `sortMinDown` 43.96% + `sortMaxDown` 36.72% of AST work units. |
| `counters-default.txt` | Full-run counters, audit box, default mode: 1,548.8M AST units, no compile attempts. |
| `counters-toplevel-default.txt` | Module-load isolation (polyfill preamble + mandreel.js, no harness), default mode: 660.6M AST units, 6.3M user calls in 21.8 s — `setupMandreel();` at mandreel.js top level; parse itself is negligible (<1 s). |
| `counters-toplevel-bytecode.txt` | Same with `--bytecode`: 506.9M AST units + 171.4M vm_ops, 84 chunks compiled / 38 bailed (25 `statement:Labeled`); wall ~21.5 s ≈ default — the #531 top-level compile is net-zero while heavy module-load bodies bail. |
| `mandreel-bbox3-bytecode-numa-pinned.txt` | NUMA probe on buildbox3: `taskset`-pinned to one node (cpus 0-7,64-71), 1 repeat — 222.1 s process wall, benchmark-internal 125.8 s (`{"results":[125814]}`), ~15% *worse* than unpinned. The unpinned protocol numbers above are representative; no NUMA artifact hides below 120 s. |
| `mandreel-bbox3-before-876.json` | Reference-box retest at `37339f3`, immediately before #876; bytecode default after #875, N=3. |
| `mandreel-bbox3-after-876.json` | Same retest at the #876 merge commit `7f8d4e2`, N=3. |

Hosts: buildbox3 = EPYC 7501 @ 2.0 GHz (2 sockets, 8 NUMA nodes, runner pinning disabled — uniform max frequency); audit box = Ryzen AI 9 HX 370 (heterogeneous, runner pins to the 5 GHz cores). Protocol: `run-jetstream.py --test mandreel --iterations 1 --no-idle-gate`, N=3 unless noted.

## #876 reference-box impact

On buildbox3, detached worktrees at the adjacent `37339f3` and `7f8d4e2` commits were built with `cargo build --release`. The two release binaries had distinct SHA-256 hashes. Both runs used the same runner from `37339f3`, one iteration, three sequential repeats, a 300-second per-measurement cap, and no idle gate. CPU pinning was disabled by the runner because all available CPUs report the same 2.0 GHz maximum. Bytecode was enabled by default in both builds by #875, so the wrappers passed no engine flags.

The host's `/tmp/JetStream` checkout was incomplete at retest time. The runner used a separate directory containing `JetStreamDriver.js` and `Octane/mandreel.js` copied from JetStream commit `c603c04db8505477867974a69789309ded2cc948`. The Mandreel source SHA-256 was `8f05c6eee5f307e84ec704075210086e4d7f7bf022df9865807351681844883f` for both runs. Starting one-minute load averages were 1.29 before and 1.06 after.

| commit | process wall, 3 repeats (s) | median (s) | benchmark-internal, 3 repeats (s) | median (s) |
|---|---:|---:|---:|---:|
| Before #876 (`37339f3`) | 194.855, 193.771, 194.042 | **194.042** | 111.538, 110.817, 111.038 | **111.038** |
| After #876 (`7f8d4e2`) | 156.759, 158.609, 157.596 | **157.596** | 91.907, 92.724, 92.128 | **92.128** |

#876 saved **36.446 seconds (18.78%)** of median process wall and **18.910 seconds (17.03%)** of median benchmark-internal time. The baseline median reproduces the earlier 193.8-second reference result. The fastest post-#876 process wall was 156.759 seconds, so [#54](https://github.com/pmatos/jsse/issues/54) remains open against its 120-second target.
