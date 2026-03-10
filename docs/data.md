# JSSE Project Data

Data snapshot for blog post. Generated 2026-03-10.

## Current State

| Metric | Value |
|--------|-------|
| Current commit | `29a72a7` |
| Rust source files | 54 |
| Rust lines of code (total) | 167,910 |
| Rust lines of code (logic) | 133,397 |
| Rust lines of code (generated data tables) | 34,513 |
| Script lines (Python/Shell/JS) | 1,961 |
| Total commits | 527 |
| Active development days | 40 |
| First commit | 2026-01-27 |
| Last commit | 2026-03-10 |
| Calendar span | 42 days |
| test262 pass rate | 91,986 / 91,986 (100.00%) |

## Dependencies

The project rule is: no JS parser or engine crate — everything must be
implemented from scratch. Utility crates for parsing combinators, math,
Unicode data, etc. are allowed. Here are all 14 dependencies and why:

| Crate | Version | Purpose |
|-------|---------|---------|
| `clap` | 4 | CLI argument parsing (`jsse <file>`, `jsse -e "code"`) |
| `num-bigint` | 0.4 | Arbitrary-precision integers for the BigInt type |
| `unicode-ident` | 1.0 | Unicode ID_Start/ID_Continue tables for identifier lexing |
| `regex` | 1 | Backend for simple RegExp patterns (no lookbehind/lookahead) |
| `fancy-regex` | 0.17 | Backend for complex RegExp patterns (lookbehind, backreferences) |
| `chrono` | 0.4 | Date/time arithmetic for the Date built-in |
| `chrono-tz` | 0.10 | IANA timezone database for Date and Temporal |
| `iana-time-zone` | 0.1 | Detect system default timezone |
| `ryu-js` | 1.0.2 | JS-spec-compliant float-to-string (Number.prototype.toString) |
| `icu` | 2.0 | ICU4X: Intl API (collation, segmentation, display names, list format, plural rules) |
| `icu_calendar` | 2.0 | ICU4X calendar systems for Intl.DateTimeFormat and Temporal |
| `icu_normalizer` | 2.1.1 | Unicode NFC/NFD/NFKC/NFKD normalization for String.prototype.normalize |
| `fixed_decimal` | 0.7 | Decimal formatting for Intl.NumberFormat |
| `tinystr` | 0.8 | Small-string locale subtag type required by ICU4X APIs |

**Not dependencies** (implemented from scratch): lexer, parser, AST,
interpreter, garbage collector, generator state machine transform,
all built-in prototypes (Object, Array, String, Number, Boolean, Symbol,
Promise, Map, Set, WeakMap, WeakSet, TypedArray, DataView, ArrayBuffer,
SharedArrayBuffer, Atomics, Proxy, Reflect, Error types, iterators,
async/await, modules, Temporal).

## Claude Code Sessions

| Metric | Value |
|--------|-------|
| Total sessions | 119 |
| Sessions with user prompts | 100 |
| Total user prompts | 45,703 |
| Total transcript lines (JSONL) | 465,463 |
| Avg prompts per active session | ~457 |

Sessions are stored as JSONL transcripts. Each session corresponds to one
`claude` CLI invocation. The 19 sessions without user prompts are likely
sub-agent spawns or interrupted sessions.

## Largest Source Files

| File | Lines |
|------|-------|
| `src/unicode_tables.rs` (generated) | 28,825 |
| `src/interpreter/eval.rs` | 18,978 |
| `src/interpreter/builtins/mod.rs` | 8,851 |
| `src/interpreter/builtins/regexp.rs` | 8,464 |
| `src/interpreter/builtins/typedarray.rs` | 6,498 |
| `src/emoji_strings.rs` (generated) | 5,688 |
| `src/interpreter/builtins/intl/datetimeformat.rs` | 5,116 |
| `src/interpreter/builtins/temporal/mod.rs` | 4,725 |
| `src/interpreter/builtins/intl/numberformat.rs` | 4,441 |
| `src/interpreter/mod.rs` | 4,385 |

## test262 Pass Rate Over Time

Data extracted from README.md updates across 290 commits. Only showing
one entry per significant change (deduplicated, keeping last entry per day
when the count changed).

```
Date        | Passing | Total  | Rate    | Milestone
------------|---------|--------|---------|-----------------------------------
2026-01-27  |   4,566 | 48,257 |   9.46% | Day 1: first test262 run
2026-01-27  |  10,662 | 42,076 |  25.34% | End of day 1
2026-01-28  |  12,036 | 42,076 |  28.61% |
2026-01-29  |  15,614 | 42,076 |  37.11% |
2026-01-30  |  16,736 | 42,076 |  39.78% |
2026-01-31  |  24,404 | 47,456 |  51.42% | Crossed 50%
2026-02-01  |  25,988 | 47,456 |  54.76% |
2026-02-02  |  28,614 | 47,456 |  60.29% | Crossed 60%
2026-02-03  |  30,450 | 47,456 |  64.16% |
2026-02-04  |  31,225 | 48,257 |  64.71% |
2026-02-06  |  33,783 | 48,257 |  70.01% | Crossed 70%
2026-02-07  |  35,691 | 48,257 |  73.96% |
2026-02-08  |  36,797 | 48,257 |  76.25% |
2026-02-09  |  37,307 | 48,257 |  77.31% |
2026-02-10  |  41,759 | 48,257 |  86.44% | Big jump (intl402/Temporal)
2026-02-11  |  42,029 | 48,257 |  86.95% |
2026-02-13  |  81,878 | 92,504 |  88.51% | Scenario counting (dual-mode)
2026-02-14  |  82,278 | 92,504 |  88.95% |
2026-02-17  |  83,024 | 92,496 |  89.76% |
2026-02-19  |  85,521 | 92,624 |  92.33% | Crossed 90%
2026-02-21  |  86,342 | 92,242 |  93.60% |
2026-02-22  |  87,596 | 92,242 |  94.96% |
2026-02-23  |  87,502 | 91,986 |  95.13% | Crossed 95%
2026-02-24  |  88,416 | 92,114 |  95.99% |
2026-02-25  |  89,099 | 92,242 |  96.59% |
2026-02-26  |  89,635 | 91,986 |  97.44% |
2026-02-27  |  89,696 | 91,986 |  97.51% |
2026-02-28  |  89,843 | 91,986 |  97.67% |
2026-03-01  |  89,927 | 91,986 |  97.76% |
2026-03-02  |  90,244 | 91,986 |  98.11% | Crossed 98%
2026-03-03  |  90,506 | 91,986 |  98.39% |
2026-03-04  |  91,001 | 91,986 |  98.93% |
2026-03-05  |  91,572 | 91,986 |  99.55% | Crossed 99%
2026-03-06  |  91,815 | 91,986 |  99.81% |
2026-03-07  |  91,924 | 91,986 |  99.93% |
2026-03-08  |  91,964 | 91,986 |  99.98% |
2026-03-09  |  91,986 | 91,986 | 100.00% | 100% achieved!
```

### Progress Graph (pass rate %)

```
100% |                                                                  *
     |                                                              ****
 95% |                                                         *****
     |                                                     ****
 90% |                                                 ****
     |                                             ****
 85% |                                       ******
     |                                      *
 80% |                                    **
     |                                   *
 75% |                                ***
     |                              **
 70% |                            **
     |                          **
 65% |                        **
     |                      **
 60% |                    **
     |                  **
 55% |                **
     |              **
 50% |            **
     |          **
 45% |         *
     |        *
 40% |       *
     |     **
 35% |    **
     |   **
 30% |  **
     | **
 25% |**
     |*
 20% |
     |*
 15% |*
     |*
 10% |*
     |
  0% +--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+--+
     1/27   1/31   2/4    2/8   2/12   2/16   2/20   2/24   2/28   3/4  3/9
     |<-- Jan -->|<---------- February ----------->|<---- March ---->|
```

### Full Data (CSV)

All 290 README.md snapshots for precise graphing:

```csv
datetime,passing,total,rate
2026-01-27T16:02:06,4566,48257,9.46
2026-01-27T16:38:56,4786,42076,11.37
2026-01-27T16:48:25,4894,42076,11.63
2026-01-27T16:51:42,5586,42076,13.28
2026-01-27T17:53:20,5923,42076,14.08
2026-01-27T18:19:00,6228,42076,14.80
2026-01-27T18:29:08,6909,42076,16.42
2026-01-27T18:38:35,7170,42076,17.04
2026-01-27T18:44:01,7325,42076,17.41
2026-01-27T19:01:07,7419,42076,17.63
2026-01-27T19:20:44,7756,42076,18.43
2026-01-27T20:00:16,9402,42076,22.35
2026-01-27T20:13:51,9588,42076,22.79
2026-01-27T20:26:30,10003,42076,23.77
2026-01-27T20:47:55,10473,42076,24.89
2026-01-27T21:12:07,10515,42076,24.99
2026-01-27T22:30:44,10662,42076,25.34
2026-01-27T23:55:24,10992,42076,26.12
2026-01-28T00:03:22,11019,42076,26.19
2026-01-28T00:11:45,11198,42076,26.61
2026-01-28T00:19:36,11295,42076,26.84
2026-01-28T00:25:28,11378,42076,27.04
2026-01-28T08:34:54,11644,42076,27.67
2026-01-28T08:51:36,11656,42076,27.70
2026-01-28T09:05:01,11681,42076,27.76
2026-01-28T09:12:10,11915,42076,28.32
2026-01-28T09:52:16,12004,42076,28.53
2026-01-28T12:22:31,12036,42076,28.61
2026-01-28T18:20:15,12045,42076,28.63
2026-01-28T19:48:26,12354,42076,29.36
2026-01-29T09:43:20,12482,42076,29.67
2026-01-29T10:28:57,12878,42076,30.61
2026-01-29T13:51:04,13284,42076,31.57
2026-01-29T14:57:04,14219,42076,33.79
2026-01-29T18:59:32,14359,42076,34.13
2026-01-29T20:56:52,14684,42076,34.90
2026-01-29T22:09:32,15614,42076,37.11
2026-01-30T08:43:33,15828,42076,37.62
2026-01-30T09:37:48,15956,42076,37.92
2026-01-30T11:02:15,15999,42076,38.02
2026-01-30T13:41:41,16164,42076,38.42
2026-01-30T14:34:27,16373,42076,38.91
2026-01-30T15:21:03,16500,42076,39.21
2026-01-30T16:04:44,16736,42076,39.78
2026-01-31T00:13:47,16746,42076,39.80
2026-01-31T11:11:32,16828,42076,39.99
2026-01-31T12:23:28,16948,42076,40.28
2026-01-31T12:52:10,19152,42076,45.52
2026-01-31T16:34:54,20045,42076,47.64
2026-01-31T17:35:11,20232,47456,42.63
2026-01-31T18:45:55,21155,47456,44.58
2026-01-31T19:37:08,21271,47456,44.82
2026-01-31T21:59:38,22253,47456,46.89
2026-01-31T22:34:35,24404,47456,51.42
2026-02-01T10:26:14,24585,47456,51.80
2026-02-01T11:15:56,24604,47456,51.84
2026-02-01T12:11:12,24894,47456,52.45
2026-02-01T12:47:57,25334,47456,53.38
2026-02-01T13:49:13,25404,47456,53.53
2026-02-01T15:05:57,25502,47456,53.74
2026-02-01T19:44:39,25617,47456,53.98
2026-02-01T20:26:14,25951,47456,54.68
2026-02-01T21:02:37,25988,47456,54.76
2026-02-02T07:06:14,26019,47456,54.83
2026-02-02T08:41:08,26077,47456,54.95
2026-02-02T10:21:56,26694,47456,56.25
2026-02-02T10:48:24,26833,47456,56.54
2026-02-02T12:03:15,26894,47456,56.67
2026-02-02T13:51:43,27149,47456,57.21
2026-02-02T15:05:22,27198,47456,57.31
2026-02-02T17:24:32,27340,47456,57.61
2026-02-02T18:30:54,27509,47456,57.96
2026-02-02T19:24:42,27668,47456,58.30
2026-02-02T20:06:16,27742,47456,58.46
2026-02-02T21:32:58,27815,47456,58.61
2026-02-02T22:29:30,28614,47456,60.29
2026-02-03T00:25:20,29156,47456,61.44
2026-02-03T00:52:00,29527,47456,62.22
2026-02-03T01:40:35,29602,47456,62.38
2026-02-03T01:51:24,29720,47456,62.62
2026-02-03T01:58:13,29756,47456,62.70
2026-02-03T09:11:45,30450,47456,64.16
2026-02-04T08:48:19,30914,48257,64.06
2026-02-04T08:54:32,30920,48257,64.07
2026-02-04T09:01:59,31133,48257,64.51
2026-02-04T09:57:27,31134,48257,64.52
2026-02-04T10:43:30,31147,48257,64.54
2026-02-04T13:52:55,31195,48257,64.64
2026-02-04T14:15:36,31222,48257,64.70
2026-02-04T15:56:45,31225,48257,64.71
2026-02-05T21:19:46,31281,48257,64.82
2026-02-05T22:24:16,31419,48257,65.11
2026-02-06T07:00:32,31464,48257,65.20
2026-02-06T11:53:29,31852,48257,66.00
2026-02-06T12:41:14,32148,48257,66.62
2026-02-06T16:09:18,32615,48257,67.59
2026-02-06T17:36:06,32760,48257,67.89
2026-02-06T18:39:10,32893,48257,68.16
2026-02-06T19:08:13,32974,48257,68.33
2026-02-06T19:56:42,33239,48257,68.88
2026-02-06T21:29:13,33529,48257,69.48
2026-02-06T22:45:26,33783,48257,70.01
2026-02-07T00:15:55,33920,48257,70.29
2026-02-07T10:39:16,34703,48257,71.91
2026-02-07T12:21:45,34934,48257,72.39
2026-02-07T14:41:19,35220,48257,72.98
2026-02-07T15:45:49,35353,48257,73.26
2026-02-07T22:16:09,35691,48257,73.96
2026-02-08T10:45:11,36054,48257,74.71
2026-02-08T13:53:36,36413,48257,75.46
2026-02-08T17:23:40,36484,48257,75.60
2026-02-08T21:39:49,36629,48257,75.90
2026-02-08T23:49:04,36797,48257,76.25
2026-02-09T11:00:26,36876,48257,76.42
2026-02-09T12:31:05,36985,48257,76.64
2026-02-09T15:42:32,37093,48257,76.87
2026-02-09T16:44:46,37104,48257,76.89
2026-02-09T17:17:48,37307,48257,77.31
2026-02-10T01:39:04,41344,48257,85.66
2026-02-10T03:08:37,41372,48257,85.68
2026-02-10T04:55:50,41410,48257,85.75
2026-02-10T05:35:35,41427,48257,85.79
2026-02-10T06:19:52,41440,48257,85.82
2026-02-10T09:11:59,41469,48257,85.86
2026-02-10T14:18:55,41649,48257,86.23
2026-02-10T17:54:44,41759,48257,86.44
2026-02-11T09:09:29,41787,48257,86.50
2026-02-11T10:45:51,41808,48257,86.53
2026-02-11T11:52:19,41822,48257,86.56
2026-02-11T18:46:25,41840,48257,86.56
2026-02-11T20:04:28,42029,48257,86.95
2026-02-13T11:38:11,42305,48257,87.52
2026-02-13T16:25:21,81581,92658,88.05
2026-02-13T18:32:55,81717,92504,88.34
2026-02-13T21:46:25,81878,92504,88.51
2026-02-14T09:01:26,82162,92632,88.70
2026-02-14T11:16:10,82124,92504,88.78
2026-02-14T23:48:41,82278,92504,88.95
2026-02-17T17:24:29,82610,92632,89.18
2026-02-17T23:10:24,83024,92496,89.76
2026-02-18T14:37:54,83125,92496,89.87
2026-02-18T20:37:22,83427,92496,90.20
2026-02-18T21:19:56,83486,92496,90.26
2026-02-19T10:08:37,83513,92496,90.29
2026-02-19T11:03:02,83814,92496,90.61
2026-02-19T12:05:51,83946,92496,90.76
2026-02-19T13:35:23,84417,92496,91.27
2026-02-19T17:24:10,84827,92496,91.71
2026-02-19T19:16:01,85045,92624,91.82
2026-02-19T19:41:00,85060,92624,91.83
2026-02-19T20:04:37,85157,92624,91.94
2026-02-19T21:53:48,85521,92624,92.33
2026-02-20T20:54:25,85749,92242,92.96
2026-02-21T10:40:44,85623,92111,92.96
2026-02-21T15:11:13,86116,92242,93.36
2026-02-21T21:53:54,86342,92242,93.60
2026-02-22T12:06:16,86538,92242,93.82
2026-02-22T15:39:39,86556,92242,93.84
2026-02-22T17:45:01,87428,92242,94.78
2026-02-22T20:29:16,87480,92242,94.84
2026-02-22T21:59:23,87596,92242,94.96
2026-02-23T16:08:20,87494,91986,95.12
2026-02-23T16:54:24,87500,91986,95.12
2026-02-23T20:51:59,87502,91986,95.13
2026-02-24T06:03:08,87842,91986,95.49
2026-02-24T12:29:40,88004,91986,95.67
2026-02-24T14:51:32,88160,91986,95.84
2026-02-24T17:09:12,88416,92114,95.99
2026-02-24T22:52:15,88474,92114,96.05
2026-02-25T07:51:26,88600,92114,96.19
2026-02-25T12:40:52,88702,92114,96.30
2026-02-25T15:12:40,88722,92114,96.32
2026-02-25T17:13:19,89131,92550,96.31
2026-02-25T18:24:41,89423,92788,96.37
2026-02-25T19:31:22,88843,91986,96.58
2026-02-25T20:17:17,89099,92242,96.59
2026-02-26T02:09:27,89171,92242,96.66
2026-02-26T03:27:10,89250,91988,97.02
2026-02-26T10:43:40,89422,91986,97.21
2026-02-26T11:20:58,89452,91986,97.25
2026-02-26T16:46:16,89635,91986,97.44
2026-02-27T13:34:02,89632,91986,97.44
2026-02-27T16:38:24,89659,91986,97.47
2026-02-27T19:14:11,89696,91986,97.51
2026-02-28T17:55:57,89797,91986,97.62
2026-02-28T22:35:23,89843,91986,97.67
2026-03-01T13:09:41,89903,91986,97.74
2026-03-01T22:01:56,89927,91986,97.76
2026-03-02T00:53:47,89974,91986,97.81
2026-03-02T01:54:42,90006,91986,97.85
2026-03-02T10:30:39,90080,91986,97.93
2026-03-02T12:24:26,90091,91986,97.94
2026-03-02T14:41:09,90101,91986,97.95
2026-03-02T17:24:56,90206,91986,98.06
2026-03-02T18:01:21,90236,91986,98.10
2026-03-02T20:20:50,90244,91986,98.11
2026-03-03T06:37:54,90250,91986,98.11
2026-03-03T07:43:41,90258,91986,98.12
2026-03-03T09:18:00,90327,91986,98.20
2026-03-03T13:44:29,90337,91986,98.21
2026-03-03T15:03:04,90365,91986,98.24
2026-03-03T16:32:20,90374,91986,98.25
2026-03-03T17:29:15,90383,91986,98.26
2026-03-03T21:06:17,90420,91986,98.30
2026-03-03T22:24:37,90458,91986,98.34
2026-03-03T23:21:22,90506,91986,98.39
2026-03-04T00:31:41,90528,91986,98.41
2026-03-04T01:09:07,90542,91986,98.43
2026-03-04T02:46:04,90560,91986,98.45
2026-03-04T04:42:44,90612,91986,98.51
2026-03-04T05:20:38,90698,91986,98.60
2026-03-04T08:23:51,90744,91986,98.65
2026-03-04T09:28:32,90762,91986,98.67
2026-03-04T11:21:40,90797,91986,98.71
2026-03-04T11:49:27,90815,91986,98.73
2026-03-04T12:25:10,90831,91986,98.74
2026-03-04T13:33:45,90843,91986,98.76
2026-03-04T14:12:15,90893,91986,98.81
2026-03-04T14:57:03,90912,91986,98.83
2026-03-04T17:46:22,90994,91986,98.92
2026-03-04T20:07:41,91001,91986,98.93
2026-03-05T06:22:09,91192,91986,99.14
2026-03-05T08:55:05,91265,91986,99.22
2026-03-05T11:38:03,91274,91986,99.23
2026-03-05T14:34:40,91513,91986,99.49
2026-03-05T15:20:58,91541,91986,99.52
2026-03-05T16:52:54,91572,91986,99.55
2026-03-05T20:40:10,91626,91986,99.61
2026-03-06T00:01:08,91658,91986,99.64
2026-03-06T00:48:29,91680,91986,99.67
2026-03-06T02:39:28,91694,91986,99.68
2026-03-06T09:49:55,91815,91986,99.81
2026-03-06T16:46:32,91831,91986,99.83
2026-03-06T23:24:39,91781,91986,99.78
2026-03-07T08:58:10,91843,91986,99.84
2026-03-07T12:11:04,91865,91986,99.87
2026-03-07T13:07:58,91877,91986,99.88
2026-03-07T13:23:47,91879,91986,99.88
2026-03-07T16:08:22,91883,91986,99.89
2026-03-07T17:51:10,91901,91986,99.91
2026-03-07T20:06:45,91924,91986,99.93
2026-03-08T01:07:15,91937,91986,99.95
2026-03-08T04:42:13,91947,91986,99.96
2026-03-08T04:59:59,91951,91986,99.96
2026-03-08T19:42:47,91952,91986,99.96
2026-03-08T22:18:55,91964,91986,99.98
2026-03-09T00:52:39,91966,91986,99.98
2026-03-09T01:24:36,91967,91986,99.98
2026-03-09T08:58:07,91975,91986,99.99
2026-03-09T20:18:29,91978,91986,99.99
2026-03-09T21:58:38,91986,91986,100.00
```

### Notes on the data

- **Scenario count jump on Feb 13**: The test runner was updated to count
  scenarios (sloppy + strict mode runs) instead of raw test files. The pass
  count roughly doubled but the rate stayed consistent.

- **Some rate dips**: The total scenario count fluctuated slightly as the
  test runner was refined (e.g., adding/removing staging tests, fixing
  dual-mode counting). This caused occasional apparent rate drops even as
  the absolute pass count increased.

- **Feb 10 jump (77% -> 86%)**: Large batch of Intl (intl402) and Temporal
  built-in implementations landed overnight.

## 3-Engine test262 Comparison

Benchmark run on 2026-03-10 using `scripts/run-test262.py -j 64 --timeout 120`
on a 128-core machine. All three engines ran the same test262 checkout with the
same harness, timeout, and scenario expansion.

### Versions tested

| Engine | Version | Binary |
|--------|---------|--------|
| JSSE | commit `29a72a7` | `./target/release/jsse` |
| Node | v25.8.0 | `/tmp/node-v25.8.0-linux-x64/bin/node` |
| Boa | v0.21 | `/tmp/boa-v0.21` |

### Pass rates

| Engine | Scenarios | Run | Skip | Pass | Fail | Rate |
|--------|-----------|-----|------|------|------|------|
| **JSSE** | 91,986 | 91,986 | 0 | 91,986 | 0 | **100.00%** |
| **Boa** | 91,986 | 91,986 | 0 | 83,260 | 8,726 | **90.51%** |
| **Node** | 91,986 | 91,187 | 799 | 79,201 | 11,986 | **86.86%** |

### Failure breakdown

| Category | Node v25.8.0 | Boa v0.21 |
|----------|-------------|-----------|
| Temporal | 8,980 (75%) | 0 |
| Atomics/agent | 236 | 0 |
| Dynamic import | 162 | 594 |
| Array.fromAsync | 176 | — |
| Module (skipped) | 799 | — |
| RegExp property escapes | — | 284 |
| Class elements/destructuring | — | 960+ |
| AssignmentTargetType | — | 608 |
| Other | 1,633 | ~6,280 |

### Top 15 failing directories per engine

**Node v25.8.0:**

```
 242  Temporal/Duration/prototype/round
 196  Temporal/ZonedDateTime/prototype/since
 192  Temporal/ZonedDateTime/prototype/until
 188  Temporal/PlainDateTime/prototype/until
 182  Temporal/PlainDateTime/prototype/since
 176  Array/fromAsync
 174  Temporal/ZonedDateTime/from
 166  Temporal/PlainDate/prototype/since
 164  Temporal/PlainDate/prototype/until
 162  language/expressions/dynamic-import/syntax/valid
 160  Temporal/PlainYearMonth/prototype/since
 156  Temporal/PlainYearMonth/prototype/until
 150  Temporal/Duration/prototype/total
 148  Temporal/PlainTime/prototype/until
 148  Temporal/PlainTime/prototype/since
```

**Boa v0.21:**

```
 608  language/expressions/assignmenttargettype
 594  language/expressions/dynamic-import/syntax/invalid
 288  language/statements/class/dstr
 288  language/expressions/class/dstr
 284  built-ins/RegExp/property-escapes
 264  language/literals/regexp
 231  language/identifiers
 196  language/statements/class/elements/syntax/early-errors
 196  language/expressions/class/elements/syntax/early-errors
 194  language/expressions/object/method-definition
 192  language/statements/class/elements/syntax/early-errors/delete
 192  language/expressions/class/elements/syntax/early-errors/delete
 183  language/block-scope/syntax/redeclaration
 178  language/statements/for-await-of
 158  language/statements/class/elements
```

### 10 slowest tests per engine

**JSSE** — Function.prototype.toString (~78s) and RegExp unicode property escapes (~48-54s):

```
78.487s  Function/prototype/toString/built-in-function-object.js
75.065s  Function/prototype/toString/built-in-function-object.js:strict
54.008s  RegExp/property-escapes/generated/Script_-_Tangsa.js:strict
51.985s  RegExp/property-escapes/generated/Script_Extensions_-_SignWriting.js
49.154s  RegExp/property-escapes/generated/Script_Extensions_-_Old_Uyghur.js
48.740s  RegExp/property-escapes/generated/Script_-_Garay.js
48.325s  RegExp/property-escapes/generated/Script_Extensions_-_Cypriot.js
48.084s  RegExp/property-escapes/generated/Script_-_Elymaic.js:strict
47.960s  RegExp/property-escapes/generated/Script_Extensions_-_Duployan.js:strict
47.940s  RegExp/property-escapes/generated/Script_Extensions_-_Katakana.js:strict
```

**Boa v0.21** — decodeURI (~10s) and RGI_Emoji regex (~5s):

```
10.836s  decodeURIComponent/S15.1.3.2_A2.5_T1.js
10.621s  decodeURI/S15.1.3.1_A2.5_T1.js:strict
10.479s  decodeURI/S15.1.3.1_A2.5_T1.js
10.321s  decodeURIComponent/S15.1.3.2_A2.5_T1.js:strict
 5.191s  RegExp/property-escapes/generated/strings/RGI_Emoji.js:strict
 5.085s  RegExp/property-escapes/generated/strings/RGI_Emoji.js
 1.980s  annexB/RegExp/RegExp-trailing-escape-BMP.js
 1.833s  language/literals/regexp/S7.8.5_A2.4_T2.js:strict
 1.829s  RegExp/character-class-escape-non-whitespace.js:strict
 1.821s  RegExp/character-class-escape-non-whitespace.js
```

**Node v25.8.0** — all 10 slowest are Atomics timeouts (120s each); real work is <230ms:

```
120.131s  Atomics/notify/bigint/notify-all-on-loc.js
120.131s  Atomics/notify/notify-one.js
120.130s  Atomics/notify/notify-in-order.js
120.130s  Atomics/notify/notify-in-order.js:strict
120.129s  Atomics/waitAsync/bigint/no-spurious-wakeup-on-or.js:strict
120.128s  Atomics/notify/bigint/notify-all-on-loc.js:strict
120.128s  Atomics/notify/notify-all.js
120.128s  Atomics/wait/nan-for-timeout.js
120.128s  Atomics/waitAsync/bigint/no-spurious-wakeup-on-xor.js
120.128s  Atomics/waitAsync/bigint/no-spurious-wakeup-on-sub.js:strict
```

### Caveats

- **Node**: 75% of its failures are **Temporal** (not shipped in Node 25).
  Module tests (799 scenarios) are skipped — the adapter can't run ES modules
  via Node. Some Atomics agent tests time out due to the basic `$262.agent`
  prelude in `scripts/node-test262-prelude.js`. Node runs under a 4GB
  `RLIMIT_AS` (V8 requires ~2GB to start), while JSSE and Boa use 512MB.
- **Boa v0.21**: Main gaps are parser-level (assignment target validation,
  class destructuring, regexp property escapes). Temporal and Atomics tests
  pass (Boa handles them internally).
- **JSSE**: Full pass across all categories including Temporal, Atomics,
  modules, and staging tests.
- **Timing**: Per-test timing data at `/tmp/timing-{jsse,node,boa}.json`.
  Timing reflects wall-clock per-test subprocess duration, including engine
  startup. Node's V8 startup (~100ms) is amortized differently than JSSE's
  lighter startup.

### Reproducing

```bash
# 1. Build JSSE
cargo build --release

# 2. Download Node.js v25.8.0
curl -sL "https://nodejs.org/dist/v25.8.0/node-v25.8.0-linux-x64.tar.xz" \
  | tar -xJ -C /tmp/

# 3. Download Boa v0.21
curl -sL "https://github.com/boa-dev/boa/releases/download/v0.21/boa-x86_64-unknown-linux-gnu" \
  -o /tmp/boa-v0.21
chmod +x /tmp/boa-v0.21

# 4. Run each engine (sequentially to avoid resource contention)
uv run python scripts/run-test262.py -j 64
uv run python scripts/run-test262.py --engine node \
  --binary /tmp/node-v25.8.0-linux-x64/bin/node -j 64
uv run python scripts/run-test262.py --engine boa \
  --binary /tmp/boa-v0.21 -j 64
```
