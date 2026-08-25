# Benchmark measurement protocol design

## Goal

Make JetStream results comparable across runs by rejecting measurements made
under background load, avoiding heterogeneous slow-core placement, reporting
repeat variability, and recording enough host context to audit later results.

## Constraints

- Preserve the existing `scripts/run-jetstream.py` benchmark selection,
  JetStream-internal iteration counts, score calculation, comparison format,
  and default sequential execution.
- Keep diagnostic parallel execution available, while making it explicit when
  the idle-window guarantee is disabled.
- Work on Linux hosts with heterogeneous cpufreq data, uniform-frequency hosts,
  and hosts where sysfs, `taskset`, `lscpu`, or `nproc` are unavailable.
- Do not change ECMAScript behavior, `spec/`, `test262/`, or the test262 pass
  baseline.

## Approaches considered

1. Add a shell wrapper around the unchanged runner. This is superficially
   small, but repeating a whole suite prevents per-benchmark idle checks and
   requires parsing human output to aggregate results.
2. Add every probe and aggregation helper directly to `run-jetstream.py`. This
   keeps one file, but makes the already large runner harder to test and reuse.
3. Put host probing, idle checks, affinity selection, and repeat statistics in
   a focused Python module, with JetStream orchestration remaining in the
   existing runner. This is the selected approach because the operating-system
   policy is independently testable without a JetStream checkout.

## Design

`scripts/benchmark_protocol.py` owns four small responsibilities:

- read `/proc/loadavg` and reject a measurement when loadavg1 is greater than
  the configured threshold;
- inspect every CPU available to the current process under
  `/sys/devices/system/cpu/cpu*/cpufreq/cpuinfo_max_freq`, distinguish uniform
  and heterogeneous frequency sets, and select the complete maximum-frequency
  cluster only when the topology is complete and `taskset` is available;
- collect the CPU model reported by `lscpu`, the logical CPU count reported by
  `nproc`, starting load averages, process affinity, and pinning decision for
  JSON output;
- summarize positive repeat values with N, median, minimum, maximum, relative
  range, and an unstable flag when `max / min - 1` exceeds 5%.

`scripts/run-jetstream.py` gains `--repeats` (default and minimum 3),
`--idle-threshold` (default 1.5), and `--no-idle-gate`. A benchmark's complete
JetStream harness invocation is one outer measurement. Before each outer
measurement, including the first, the runner re-reads loadavg1. The engine
command is prefixed with `taskset --cpu-list <fast CPUs>` when pinning is
available. JetStream's internal iteration loop is unchanged.

Reliable measurements remain sequential. `-j` greater than one requires
`--no-idle-gate`, since concurrent benchmark workers intentionally create load
and cannot satisfy the protocol. The disabled gate is printed prominently and
recorded in JSON so diagnostic runs cannot be mistaken for controlled results.

For passing benchmarks, the existing `scores` object remains the comparison
interface, with each numeric field set to the median across successful outer
measurements. The result also stores each measurement, plus a `repeat_summary`
for overall score. Console output shows median score, N, min-max score, and an
`UNSTABLE` marker above the 5% limit. A busy check returns a distinct result,
stops further sequential benchmarks, writes partial JSON, and exits nonzero.

The top-level JSON adds a `measurement_protocol` object and a `host` object.
The host object includes CPU model, logical CPU count, start load averages,
available CPU affinity, per-CPU maximum frequencies, selected fast cores, and
whether pinning was applied. Uniform or unreadable topology is explicitly
reported as unpinned with a reason rather than guessed.

## Error handling

- Invalid repeat counts, thresholds, and incompatible parallel/idle-gate
  options fail during argument validation.
- Missing or malformed loadavg data fails closed while the gate is enabled; it
  is recorded as unavailable when the gate is explicitly disabled.
- Incomplete frequency data, uniform frequencies, or missing `taskset` fall
  back to an unpinned run with a prominent topology message.
- An error, skip, or timeout in any outer measurement preserves the existing
  benchmark status and stops repeats for that benchmark; no partial set is
  presented as a valid median.

## Verification

- Unit-test load threshold boundaries and unreadable load data.
- Unit-test heterogeneous, uniform, incomplete, and affinity-restricted CPU
  topology detection using temporary sysfs trees.
- Unit-test repeat medians, ranges, and the 5% instability boundary.
- Use a temporary fake JetStream checkout and engine to verify CLI repeat
  aggregation, affinity command construction, JSON host metadata, and busy
  refusal without running the full benchmark suite.
- Run the documented synthetic burner check and the repository quality gate.

This is benchmark infrastructure only. There is no relevant ECMAScript section
or targeted test262 directory, and the test262 pass count must remain unchanged.
