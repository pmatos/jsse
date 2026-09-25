# Array result creation after issue #756

Measured in this worktree with release builds immediately before and after
the change. Each number is the median of five separate process invocations,
including engine startup. Both binaries used the release profile on the same
host, with the same input and Python `time.perf_counter()` harness. The full
benchmark is `benchmarks/scripts/bench_array.js`; the curves first fill an array with
`push` and then call the named method. `map` doubles each value, `filter`
keeps even values, and `slice` copies from index zero.

| Workload | Elements | Before (s) | After (s) | Speedup |
| --- | ---: | ---: | ---: | ---: |
| `map` | 10k | 0.110 | 0.021 | 5.3x |
| `map` | 20k | 0.308 | 0.034 | 9.0x |
| `map` | 40k | 0.863 | 0.065 | 13.3x |
| `filter` | 10k | 0.033 | 0.020 | 1.6x |
| `filter` | 20k | 0.124 | 0.036 | 3.5x |
| `filter` | 40k | 0.298 | 0.065 | 4.6x |
| `slice` | 10k | 0.103 | 0.017 | 5.9x |
| `slice` | 20k | 0.285 | 0.021 | 13.7x |
| `slice` | 40k | 0.939 | 0.045 | 20.7x |
| `bench_array.js` | 100k | 7.065 | 0.200 | 35.4x |

The 40k/20k ratios fell from 2.8 to 1.9 for `map`, 2.4 to 1.8 for
`filter`, and 3.3 to 2.2 for `slice`. These include linear input setup and
process startup, so the method costs alone are not isolated.

The host's one minute load average was 7.1–8.7 during these measurements,
above the 1.5 idle gate in the engine comparison protocol. The five run
medians show the size of the improvement and the change in scaling, but these
are diagnostic measurements rather than controlled throughput results.
