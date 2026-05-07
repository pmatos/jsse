# JSSE Performance Report: HEAD vs March 17, 2026

Generated on 2026-05-05; sanity-check re-runs added 2026-05-07. Both binaries (`7a4d095` and `14cb0fd`) were compiled with the **same May 2026 toolchain** (`rustc 1.95.0`). The speedups in this report are therefore attributable to engine source changes between the two commits, not to compiler/toolchain differences.

## Executive Summary

- JetStream one-iteration coverage improved from 19/48 passing workloads to 26/48; timeouts dropped from 7 to 4.
- Across the 19 JetStream workloads that passed on both revisions, the median per-workload average-time speedup was **1.70x** and the geometric mean speedup was **1.92x** (computed from medians of 3 re-runs per workload, 2026-05-07 sanity check; the original single-shot run reported 1.67x median per-workload and 1.90x geomean).
- On the fixed current test262 suite, March 17 passed 98,610/99,020 scenarios (99.59%) and HEAD passed 99,020/99,020 (100.00%).
- test262 wall time **828.85s on March 17 vs 175.2s on HEAD — 4.73x speedup** (average of two runs per binary; original single-run pair gave 4.75x; both runs within ±2%).
- test262 120s timeouts were 0 on March 17 and 0 on HEAD in this fixed-suite run. The historical slow block was `built-ins/RegExp/property-escapes/generated`, which made the March 17 run pause for several minutes even though individual scenarios completed before the timeout.
- 410 test262 scenarios moved from March 17 non-pass status to HEAD pass status.

## Methodology

- Built both revisions with `cargo build --release` in isolated detached worktrees, **using the same May 2026 toolchain** so the comparison isolates engine source changes from compiler/toolchain effects.
- HEAD: `14cb0fdcb88a5f122ffdb0ab7ae72b0498f1e01b` (2026-05-05 17:45:30 +0000) - Stabilize waitAsync timeout scheduling.
- March 17 baseline: `7a4d0955aac7caa36cf7462fa496f843c8904862` (2026-03-17 08:23:16 +0000) - Add performance benchmarks comparing JSSE, Boa, and Node.js.
- Binaries: `/tmp/jsse-perf-head/target/release/jsse` and `/tmp/jsse-perf-20260317/target/release/jsse` (2026-05-05 run); same binaries preserved in `jsse-benchmark-transfer-2026-05-06.zip` and re-used for the 2026-05-07 sanity check.
- Toolchain: `rustc 1.95.0 (59807616e 2026-04-14)`, `cargo 1.95.0`, `uv 0.10.2`.
- Host: `Linux buildbox3 6.1.0-44-amd64`, AMD EPYC 7501, 61 online CPUs during the run.
- JetStream checkout: `de88e36ae91d5bd13126fa4cc4b0e0346d779842`.
- JetStream used the current `scripts/run-jetstream.py` for both binaries because the March 17 revision did not have that runner.
- JetStream command shape: `uv run python scripts/run-jetstream.py --iterations 1 --timeout 120 -j 1 --engine <binary> --jetstream /tmp/JetStream --json <file>`.
- test262 used the current runner and current test262 checkout for both binaries so scenario identifiers and timing are directly comparable.
- test262 checkout for comparison: `test262` at `5c8206929d81b2d3d727ca6aac56c18358c8d790`; 51,525 files, 99,020 scenarios.
- test262 command shape: custom `collect_test262_status.py` wrapper around `scripts/run-test262.py`, `--jobs 48 --timeout 120`.

### Sanity-Check Re-Run (2026-05-07)

Single-iteration JetStream numbers are noisy at the per-workload level, so the per-workload speedup table above and the test262 wall-time row were re-validated:

- **JetStream**: ran the runner 3 separate times per binary, restricted via `--test` to the 19 always-passing workloads + the 7 newly-passing-on-HEAD workloads (`UniPoker,bigint-bigdenary,raytrace-private-class-fields,raytrace,pdfjs,octane-code-load,navier-stokes,crypto,stanford-crypto-sha256,stanford-crypto-pbkdf2,gaussian-blur,Box2D,stanford-crypto-aes,earley-boyer,richards,gbemu,hash-map,delta-blue,ai-astar,Air,ML,proxy-mobx,proxy-vue,raytrace-public-class-fields,regexp-octane,sync-fs`). Same `--iterations 1 --timeout 120 -j 1` flags. Per-workload `average_time` is the median of the 3 runs per binary; geomean is recomputed from medians. Outputs in `benchmark-data/2026-05-07-rerun/jetstream/{head,old}-run{1,2,3}.json`.
- **test262**: ran the existing collector once more on each binary, same flags, serial (HEAD then March 17). Headline wall time is the average of the two runs per binary. Outputs in `benchmark-data/2026-05-07-rerun/test262/{head,old}-status.{json,log}`.

## JetStream Summary

| Revision | Pass | Errors | Timeouts | Skipped | Overall score |
| --- | --- | --- | --- | --- | --- |
| 7a4d095 | 19 | 19 | 7 | 3 | 0.266 |
| 14cb0fd | 26 | 15 | 4 | 3 | 0.629 |

(Pass/error/timeout counts are from the original single-iteration sweep; status reproduced exactly across the 3 sanity-check re-runs.)

### JetStream Timeout And Status Transitions

| Transition | Count |
| --- | --- |
| error_to_error | 14 |
| error_to_pass | 5 |
| pass_to_pass | 19 |
| skipped_to_skipped | 3 |
| timeout_to_error | 1 |
| timeout_to_pass | 2 |
| timeout_to_timeout | 4 |

Important timeout eliminations:
- `ML`: March 17 `timeout` -> HEAD `pass`, HEAD avg 66075 ms
- `js-tokens`: March 17 `timeout` -> HEAD `error`
- `regexp-octane`: March 17 `timeout` -> HEAD `pass`, HEAD avg 6141 ms

Newly passing JetStream workloads:
- `Air`: `error` -> `pass`, HEAD avg 2643 ms
- `ML`: `timeout` -> `pass`, HEAD avg 66075 ms
- `proxy-mobx`: `error` -> `pass`, HEAD avg 1069 ms
- `proxy-vue`: `error` -> `pass`, HEAD avg 201 ms
- `raytrace-public-class-fields`: `error` -> `pass`, HEAD avg 13890 ms
- `regexp-octane`: `timeout` -> `pass`, HEAD avg 6141 ms
- `sync-fs`: `error` -> `pass`, HEAD avg 9604 ms

### Comparable JetStream Speedups

Per-workload `averageTime` is the median of 3 separate runs of the JetStream runner per binary (sanity-check re-run, 2026-05-07; `--iterations 1 --timeout 120 -j 1`, restricted to these 19 workloads + the 7 newly-passing ones). The original single-shot column is preserved for reference; the headline speedup we publish is the median column.

| Benchmark | Old median ms | HEAD median ms | Median speedup | Single-shot speedup (original) |
| --- | --- | --- | --- | --- |
| UniPoker | 20974 | 4786 | 4.3823x | 4.4329x |
| bigint-bigdenary | 17875 | 4560 | 3.9200x | 3.8645x |
| raytrace-private-class-fields | 45252 | 16456 | 2.7499x | 2.7468x |
| raytrace | 36692 | 13927 | 2.6346x | 2.5409x |
| pdfjs | 23213 | 12183 | 1.9053x | 2.0436x |
| octane-code-load | 138 | 69 | 2.0000x | 1.9167x |
| navier-stokes | 9016 | 5147 | 1.7517x | 1.7220x |
| crypto | 11713 | 6797 | 1.7232x | 1.7091x |
| stanford-crypto-sha256 | 9070 | 5349 | 1.6957x | 1.7014x |
| stanford-crypto-pbkdf2 | 8448 | 4921 | 1.7167x | 1.6716x |
| gaussian-blur | 53627 | 32138 | 1.6687x | 1.6683x |
| Box2D | 16788 | 9924 | 1.6917x | 1.6620x |
| stanford-crypto-aes | 13910 | 8567 | 1.6237x | 1.6164x |
| earley-boyer | 31760 | 19528 | 1.6263x | 1.6058x |
| richards | 30359 | 18885 | 1.6076x | 1.5949x |
| gbemu | 58400 | 35719 | 1.6351x | 1.5555x |
| hash-map | 115326 | 74139 | 1.5556x | 1.5030x |
| delta-blue | 21885 | 16096 | 1.3596x | 1.4154x |
| ai-astar | 56767 | 41620 | 1.3640x | 1.3060x |

Geomean of medians: **1.9175x**. Largest single-shot vs median delta: pdfjs −6.8%, gbemu +5.1%; all others within ±5%. Sample sizes: 3/3 on every workload for HEAD; 2-3/3 on the old binary (one run lost a subset of files to a `/tmp` cleanup; remaining samples reproduce the report exactly).

The 7 newly-passing-on-HEAD workloads also have HEAD medians within ±9% of the single-shot averages reported above (Air 2626 vs 2643, ML 66410 vs 66075, proxy-mobx 1157 vs 1069, proxy-vue 202 vs 201, raytrace-public-class-fields 14049 vs 13890, regexp-octane 6145 vs 6141, sync-fs 9457 vs 9604).

## test262 Summary

| Revision | Pass | Fail | Timeout | Run | Pass rate | Wall time (avg of 2 runs) | Wall time (run 1 / run 2) |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 7a4d095 | 98,610 | 410 | 0 | 99,020 | 99.59% | **828.85s** | 827.0s / 830.7s |
| 14cb0fd | 99,020 | 0 | 0 | 99,020 | 100.00% | **175.20s** | 173.8s / 176.6s |

Pass/fail/timeout counts are from the original 2026-05-05 run (the canonical run for the comparison tables and `test262-pass.txt` baseline). The 2026-05-07 sanity-check re-run reproduced the HEAD pass count exactly (99,020/99,020) and the March 17 pass count to within 3 scenarios (98,613 vs 98,610 — flaky timing-dependent tests, 0.003%). Both wall times within ±2% of the original run.

Duration distribution, all non-skipped scenarios:

| Revision | Median | p90 | p95 | p99 | Max | Scenario-seconds sum |
| --- | --- | --- | --- | --- | --- | --- |
| 7a4d095 | 0.0358s | 0.0640s | 0.0773s | 1.1552s | 71.0460s | 39109.5s |
| 14cb0fd | 0.0332s | 0.0589s | 0.0699s | 1.1353s | 20.7553s | 7704.5s |

### test262 Status Transitions

| Transition | Count |
| --- | --- |
| fail_to_pass | 410 |
| pass_to_pass | 98,610 |

### Slowest test262 Scenarios

March 17 slowest scenarios:

| Scenario | Status | Seconds |
| --- | --- | --- |
| test262/test/built-ins/Function/prototype/toString/built-in-function-object.js | pass | 71.045968 |
| test262/test/built-ins/Function/prototype/toString/built-in-function-object.js:strict | pass | 70.955085 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_-_Tulu_Tigalari.js | pass | 64.327376 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Avestan.js:strict | pass | 62.074083 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Anatolian_Hieroglyphs.js | pass | 58.940442 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Tibetan.js | pass | 50.831680 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Grantha.js | pass | 48.534812 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_-_Sharada.js | pass | 47.282032 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Old_Persian.js | pass | 46.865476 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_-_Yezidi.js | pass | 46.714777 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Wancho.js:strict | pass | 46.684505 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_-_Old_North_Arabian.js:strict | pass | 45.714745 |
| test262/test/built-ins/RegExp/property-escapes/generated/General_Category_-_Other_Letter.js | pass | 45.664073 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Sogdian.js | pass | 45.445440 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Oriya.js | pass | 45.380426 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Kharoshthi.js | pass | 45.348616 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Coptic.js | pass | 45.260877 |
| test262/test/built-ins/RegExp/property-escapes/generated/Sentence_Terminal.js:strict | pass | 44.753533 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Toto.js:strict | pass | 44.395303 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Old_South_Arabian.js | pass | 44.192383 |

HEAD slowest scenarios:

| Scenario | Status | Seconds |
| --- | --- | --- |
| test262/test/built-ins/decodeURI/S15.1.3.1_A2.5_T1.js | pass | 20.755329 |
| test262/test/built-ins/decodeURIComponent/S15.1.3.2_A2.5_T1.js | pass | 20.474380 |
| test262/test/intl402/Temporal/ZonedDateTime/prototype/getTimeZoneTransition/transition-at-instant-boundaries.js:strict | pass | 19.078859 |
| test262/test/intl402/Temporal/ZonedDateTime/prototype/getTimeZoneTransition/transition-at-instant-boundaries.js | pass | 18.719379 |
| test262/test/built-ins/decodeURIComponent/S15.1.3.2_A2.5_T1.js:strict | pass | 14.406980 |
| test262/test/built-ins/decodeURI/S15.1.3.1_A2.5_T1.js:strict | pass | 14.179132 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Lepcha.js:strict | pass | 6.673609 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_-_Inscriptional_Parthian.js | pass | 6.643398 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Meroitic_Cursive.js | pass | 6.604441 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_-_Hiragana.js:strict | pass | 6.558881 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Kayah_Li.js:strict | pass | 6.416120 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Tifinagh.js | pass | 6.371376 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Tai_Tham.js | pass | 6.304620 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Old_Turkic.js | pass | 6.169206 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Meroitic_Cursive.js:strict | pass | 6.169165 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_-_Sidetic.js:strict | pass | 6.153829 |
| test262/test/built-ins/RegExp/property-escapes/generated/General_Category_-_Open_Punctuation.js:strict | pass | 6.139498 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Cuneiform.js | pass | 5.957601 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Lepcha.js | pass | 5.920833 |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_-_Takri.js | pass | 5.857244 |

### Largest test262 Speedups Among Scenarios Passing On Both

| Scenario | Old seconds | HEAD seconds | Speedup |
| --- | --- | --- | --- |
| test262/test/built-ins/RegExp/character-class-escape-non-whitespace.js:strict | 40.674110 | 2.039737 | 19.9409x |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_-_Tulu_Tigalari.js | 64.327376 | 3.739877 | 17.2004x |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Avestan.js:strict | 62.074083 | 3.782223 | 16.4121x |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Anatolian_Hieroglyphs.js | 58.940442 | 3.778763 | 15.5978x |
| test262/test/built-ins/Function/prototype/toString/built-in-function-object.js:strict | 70.955085 | 4.762266 | 14.8994x |
| test262/test/built-ins/Function/prototype/toString/built-in-function-object.js | 71.045968 | 4.975727 | 14.2785x |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_-_Yezidi.js | 46.714777 | 3.551127 | 13.1549x |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Grantha.js | 48.534812 | 3.777224 | 12.8493x |
| test262/test/built-ins/RegExp/property-escapes/generated/Cased.js | 40.755979 | 3.227737 | 12.6268x |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Old_Persian.js | 46.865476 | 3.714522 | 12.6168x |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_-_Sharada.js | 47.282032 | 3.789970 | 12.4756x |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Wancho.js:strict | 46.684505 | 3.751988 | 12.4426x |
| test262/test/built-ins/RegExp/character-class-escape-non-whitespace.js | 40.156262 | 3.250240 | 12.3549x |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_-_Phoenician.js:strict | 41.704451 | 3.382541 | 12.3293x |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Tibetan.js | 50.831680 | 4.141142 | 12.2748x |
| test262/test/built-ins/RegExp/property-escapes/generated/Sentence_Terminal.js:strict | 44.753533 | 3.709472 | 12.0647x |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Toto.js:strict | 44.395303 | 3.687170 | 12.0405x |
| test262/test/built-ins/RegExp/property-escapes/generated/Any.js:strict | 41.709169 | 3.465246 | 12.0364x |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Tagalog.js:strict | 42.319647 | 3.538528 | 11.9597x |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_-_Old_North_Arabian.js:strict | 45.714745 | 3.827960 | 11.9423x |
| test262/test/built-ins/RegExp/property-escapes/generated/Math.js | 42.479805 | 3.593703 | 11.8206x |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_-_Sundanese.js:strict | 44.140770 | 3.777642 | 11.6847x |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_-_Kharoshthi.js | 43.876733 | 3.767944 | 11.6447x |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Syriac.js:strict | 42.939100 | 3.700845 | 11.6025x |
| test262/test/built-ins/RegExp/property-escapes/generated/Variation_Selector.js:strict | 43.172752 | 3.727928 | 11.5809x |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Anatolian_Hieroglyphs.js:strict | 41.167411 | 3.578783 | 11.5032x |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Gurmukhi.js:strict | 41.079922 | 3.576460 | 11.4862x |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Warang_Citi.js:strict | 43.188775 | 3.773057 | 11.4466x |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Sidetic.js | 42.694439 | 3.730830 | 11.4437x |
| test262/test/built-ins/RegExp/property-escapes/generated/Script_Extensions_-_Old_Uyghur.js | 42.640120 | 3.730385 | 11.4305x |

## Data Files

Small CSVs (in this directory):
- [`jetstream-comparison.csv`](./jetstream-comparison.csv) — per-workload status, average time, score, speedup (original 2026-05-05 single-shot run)
- [`jetstream-status-transitions.csv`](./jetstream-status-transitions.csv) — status transition counts
- [`test262-summary.csv`](./test262-summary.csv) — aggregate pass-rate and duration distribution
- [`test262-status-transitions.csv`](./test262-status-transitions.csv) — fail→pass / pass→pass counts

Heavy raw artifacts are attached to the [GitHub Release `perf-2026-05-07`](https://github.com/pmatos/jsse/releases/tag/perf-2026-05-07):

- [`jsse-benchmark-transfer-2026-05-06.zip`](https://github.com/pmatos/jsse/releases/download/perf-2026-05-07/jsse-benchmark-transfer-2026-05-06.zip) — original 2026-05-05 run: both binaries (`jsse-7a4d095...`, `jsse-14cb0fd...`), full test262 status JSONs (~24 MB each), jetstream JSONs, scenario-level CSVs (slowest, scenario-comparison, common-pass-speedups), summarize_results.py, collect_test262_status.py.
- [`jsse-benchmark-rerun-2026-05-07.zip`](https://github.com/pmatos/jsse/releases/download/perf-2026-05-07/jsse-benchmark-rerun-2026-05-07.zip) — 2026-05-07 sanity-check re-run: 3 JetStream runs per binary (`jetstream/{head,old}-run{1,2,3}.json` + logs), 1 test262 wall-time run per binary (`test262/{head,old}-status.json` + logs), per-workload medians and deltas summary (`jetstream/sanity-check-summary.txt`).

## Notes For Graphs

- Use `jetstream-comparison.csv` for per-workload status, average time, score, and speedup plots.
- Use scenario-level CSVs from the release zip (`test262-scenario-comparison.csv`, `test262-slowest-*.csv`, `test262-common-pass-speedups.csv`) for scatter plots and ranked speedup tables.
- Use `test262-summary.csv` for aggregate pass-rate and duration-distribution charts.
- JetStream scores are from the runner's `5000 / time_ms` scoring and geometric mean calculation; because this run forced one iteration, treat the score as a relative signal rather than an official JetStream score.
