# JSSE HEAD versus Boa HEAD: benchmark audit

JSSE `2ea6417535037ca58d78a433a72a2a0e2dfb4d77`; Boa `f0789a20d87ad52eef6460953929c2b3cdbf8716`; JetStream `c603c04db8505477867974a69789309ded2cc948`.

Both engines were built with their default fat-LTO release profiles. Boa includes its default bundled Intl and fast allocator. The unchanged JSSE release binary is the timing target; a separate release-nolto perf-counters build is used only for execution counts. Engine binaries were snapshotted before measurements.

All measurements are sequential, pinned to CPU 0, with alternating JSSE/Boa order, three fresh CLI invocations per comparable successful workload (failed counterparts stop further timing repetitions), a 180-second timeout and an 8 GiB address-space cap. Node supplies the output/validation reference. Failed workloads are retained and excluded from ratios. Background load was present; these are diagnostic comparisons, not controlled benchmark publication results. Every sample retains load averages, exit status and output.

The JetStream audit executes one complete workload iteration in each fresh process, including init, preparation and validation. It does not run the official 4–160-iteration schedules or publish an official JetStream score. Both process wall time and inner iteration time are retained. Boa V8 workloads adapt their work to a time window: their primary ratio is inverse reported throughput, not process wall time. Tiny Boa engine scripts mostly measure CLI startup; no runtime-performance conclusions should be drawn from those rows.

The inventory covers the JavaScript benchmark scripts from both source trees, 146 retained diagnostics, and every JavaScript leaf in the pinned upstream JetStream registry: 88 attempted out of 109 total leaves. It includes the otherwise omitted startup/scene, Intl, SunSpider, WebTooling and WSL workloads, with equivalent in-memory text/binary preload adapters. The 21 excluded leaves (19 WebAssembly, two Workers) and their upstream tags are enumerated in `exclusions.json`; optional/disabled JavaScript leaves are included. The JavaScript source-map WebTooling leaf was also attempted, but JSSE cannot run its WebAssembly dependency. Rust-only parser/GC/microarchitecture benchmarks do not provide a cross-engine JavaScript workload. The extra 23 Boa expression checks make otherwise unprinted results observable; 17 paired controls isolate arithmetic, lexical loops, function calls and parameter binding.

## Coverage

| Group | Cases | JSSE success | Boa success | Both succeed |
|---|---:|---:|---:|---:|
| jsse | 11 | 11 | 11 | 11 |
| boa-scripts | 35 | 35 | 34 | 34 |
| boa-engine | 23 | 23 | 23 | 23 |
| jetstream | 48 | 48 | 44 | 44 |
| diagnostic-1010 | 130 | 130 | 130 | 130 |
| diagnostic-1013 | 16 | 16 | 16 | 16 |
| jetstream-extra | 9 | 9 | 9 | 9 |
| boa-validation | 23 | 23 | 23 | 23 |
| audit-controls | 17 | 17 | 17 | 17 |
| jetstream-registry | 31 | 28 | 23 | 22 |

343 cases, 2359 process measurements. 110 cases have sampling profiles and deterministic execution counters.

For output checks, the harness removes timing lines and Boa’s display of a script’s final undefined/Promise value. The original Boa engine scripts do not print their expression results; use the separate `boa-validation` rows for those correctness checks. Intl benchmarks returning undefined only establish successful execution, not locale-output equivalence.

## Findings

[GitHub audit tracker](https://github.com/pmatos/jsse/issues/1034) links the findings and summarizes scope, controls and exclusions.

| Finding | Issue |
|---|---|
| Console is disconnected from the global object | https://github.com/pmatos/jsse/issues/1021 |
| perf(bytecode): verified slots miss function lexicals and captured/global lexical bindings | https://github.com/pmatos/jsse/issues/1022 |
| perf(function): compiled leaf calls still spend substantial time building and tearing down frames | https://github.com/pmatos/jsse/issues/1023 |
| perf(function): default parameters re-intern names and allocate a body environment on every call | https://github.com/pmatos/jsse/issues/1024 |
| perf(arguments): arguments-only calls still materialize property maps and re-intern the arguments binding | https://github.com/pmatos/jsse/issues/1025 |
| perf(bigint): VM BigInt operators still pay generic dispatch and ownership costs in fully compiled loops | https://github.com/pmatos/jsse/issues/1026 |
| perf(gc): nursery collections repeatedly trace large environment/object graphs in Three.js and deep promise queues | https://github.com/pmatos/jsse/issues/1027 |
| perf(runtime): closure and small-array construction still eagerly allocate metadata and property storage | https://github.com/pmatos/jsse/issues/1028 |
| perf(property): bytecode named property access bypasses property ICs and repeatedly enters generic MOP/key paths | https://github.com/pmatos/jsse/issues/1029 |
| perf(vm): fully native arithmetic loops still trail Boa after removing fallback, hashing and GC costs | https://github.com/pmatos/jsse/issues/1030 |
| perf(generator): state payload execution and resume bookkeeping retain a 2x gap on lazy/manual iteration | https://github.com/pmatos/jsse/issues/1031 |
| Function declarations lost across state fragments | https://github.com/pmatos/jsse/issues/1032 |
| perf(intl): cache ICU plural rules instead of rebuilding locale data on every select | https://github.com/pmatos/jsse/issues/1033 |

Two confirmed bugs and eleven performance findings were opened; each issue includes a reproduction, measurements, profile/counter evidence, a potential cause and semantic constraints. All 107 observed process/throughput slowdowns map to an investigated issue. Tiny or overlapping observations are attached as evidence without asserting a demonstrated defect.

## Measured JSSE slowdowns

Ratios greater than 1 mean JSSE is slower. “Separated” means the three observed ranges do not overlap and the workload exceeds the 30 ms startup floor; it is not a statistical confidence interval. Small or overlapping differences remain observations, not established performance defects. V8 rows use reported scores and their score ranges. Every observed slowdown is profiled, including differences that remain within noise.

| Workload | Metric | JSSE median | Boa median | Slowdown | Separated | Finding | Profile |
|---|---|---:|---:|---:|:---:|---|---|
| diagnostic-1010/generators/bench_generators_manual_next | seconds | 0.334718 | 0.142985 | 2.341 | yes | https://github.com/pmatos/jsse/issues/1031 | [profile](profiles/diagnostic-1010__generators__bench_generators_manual_next.self.txt) |
| audit-controls/params-default-omitted | seconds | 0.306154 | 0.140527 | 2.179 | yes | https://github.com/pmatos/jsse/issues/1024 | [profile](profiles/audit-controls__params-default-omitted.self.txt) |
| diagnostic-1010/bigint/bench_bigint_mulmod256 | seconds | 0.345339 | 0.164740 | 2.096 | yes | https://github.com/pmatos/jsse/issues/1024 | [profile](profiles/diagnostic-1010__bigint__bench_bigint_mulmod256.self.txt) |
| diagnostic-1010/generators/bench_generators_lazy_collections | seconds | 0.624972 | 0.304440 | 2.053 | yes | https://github.com/pmatos/jsse/issues/1031 | [profile](profiles/diagnostic-1010__generators__bench_generators_lazy_collections.self.txt) |
| audit-controls/params-default-supplied | seconds | 0.291043 | 0.143152 | 2.033 | yes | https://github.com/pmatos/jsse/issues/1024 | [profile](profiles/audit-controls__params-default-supplied.self.txt) |
| jetstream/lazy-collections | seconds | 0.629330 | 0.312756 | 2.012 | yes | https://github.com/pmatos/jsse/issues/1031 | [profile](profiles/jetstream__lazy-collections.self.txt) |
| diagnostic-1010/bigint/bench_bigint_invert | seconds | 0.166565 | 0.083645 | 1.991 | yes | https://github.com/pmatos/jsse/issues/1026 | [profile](profiles/diagnostic-1010__bigint__bench_bigint_invert.self.txt) |
| boa-scripts/closures/create | seconds | 0.070093 | 0.037429 | 1.873 | yes | https://github.com/pmatos/jsse/issues/1028 | [profile](profiles/boa-scripts__closures__create.self.txt) |
| diagnostic-1010/calls/bench_calls_args | seconds | 0.362549 | 0.196379 | 1.846 | yes | https://github.com/pmatos/jsse/issues/1025 | [profile](profiles/diagnostic-1010__calls__bench_calls_args.self.txt) |
| diagnostic-1010/bigint/bench_bigint_compare_numeric_control | seconds | 0.345366 | 0.187424 | 1.843 | yes | https://github.com/pmatos/jsse/issues/1022 | [profile](profiles/diagnostic-1010__bigint__bench_bigint_compare_numeric_control.self.txt) |
| diagnostic-1010/proxy-classes/bench_proxy-classes_getter_setter | seconds | 0.312133 | 0.175148 | 1.782 | yes | https://github.com/pmatos/jsse/issues/1029 | [profile](profiles/diagnostic-1010__proxy-classes__bench_proxy-classes_getter_setter.self.txt) |
| diagnostic-1013/bench_block_let | seconds | 0.243509 | 0.137680 | 1.769 | yes | https://github.com/pmatos/jsse/issues/1030 | [profile](profiles/diagnostic-1013__bench_block_let.self.txt) |
| diagnostic-1013/bench_script_let_loop | seconds | 0.237002 | 0.134013 | 1.769 | yes | https://github.com/pmatos/jsse/issues/1022 | [profile](profiles/diagnostic-1013__bench_script_let_loop.self.txt) |
| diagnostic-1010/bigint/bench_bigint_ops_vm_add | seconds | 0.393799 | 0.228145 | 1.726 | yes | https://github.com/pmatos/jsse/issues/1026 | [profile](profiles/diagnostic-1010__bigint__bench_bigint_ops_vm_add.self.txt) |
| boa-scripts/v8-benches/richards | V8 score | 225.000000 | 381.000000 | 1.693 | yes | https://github.com/pmatos/jsse/issues/1029 | [profile](profiles/boa-scripts__v8-benches__richards.self.txt) |
| diagnostic-1010/bigint/bench_bigint_small | seconds | 0.164108 | 0.097561 | 1.682 | yes | https://github.com/pmatos/jsse/issues/1026 | [profile](profiles/diagnostic-1010__bigint__bench_bigint_small.self.txt) |
| diagnostic-1010/bigint/bench_bigint_compare | seconds | 0.453389 | 0.276218 | 1.641 | yes | https://github.com/pmatos/jsse/issues/1022 | [profile](profiles/diagnostic-1010__bigint__bench_bigint_compare.self.txt) |
| boa-scripts/basic/nested-loop | seconds | 0.030413 | 0.018712 | 1.625 | yes | https://github.com/pmatos/jsse/issues/1022 | [profile](profiles/boa-scripts__basic__nested-loop.self.txt) |
| diagnostic-1013/bench_switch_let | seconds | 0.213100 | 0.132526 | 1.608 | yes | https://github.com/pmatos/jsse/issues/1030 | [profile](profiles/diagnostic-1013__bench_switch_let.self.txt) |
| diagnostic-1010/generators/bench_generators_pipeline | seconds | 0.545202 | 0.341929 | 1.594 | yes | https://github.com/pmatos/jsse/issues/1031 | [profile](profiles/diagnostic-1010__generators__bench_generators_pipeline.self.txt) |
| audit-controls/args-object | seconds | 0.851030 | 0.533831 | 1.594 | yes | https://github.com/pmatos/jsse/issues/1025 | [profile](profiles/audit-controls__args-object.self.txt) |
| jetstream/richards | seconds | 0.754779 | 0.482655 | 1.564 | yes | https://github.com/pmatos/jsse/issues/1029 | [profile](profiles/jetstream__richards.self.txt) |
| audit-controls/loop-let | seconds | 0.231108 | 0.150030 | 1.540 | yes | https://github.com/pmatos/jsse/issues/1022 | [profile](profiles/audit-controls__loop-let.self.txt) |
| diagnostic-1013/bench_closure_literal | seconds | 0.334763 | 0.219326 | 1.526 | yes | https://github.com/pmatos/jsse/issues/1028 | [profile](profiles/diagnostic-1013__bench_closure_literal.self.txt) |
| diagnostic-1010/bigint/bench_bigint_literal | seconds | 0.159740 | 0.106963 | 1.493 | yes | https://github.com/pmatos/jsse/issues/1026 | [profile](profiles/diagnostic-1010__bigint__bench_bigint_literal.self.txt) |
| diagnostic-1010/proxy-classes/bench_proxy-classes_control_trap_direct | seconds | 0.474010 | 0.322005 | 1.472 | yes | https://github.com/pmatos/jsse/issues/1029 | [profile](profiles/diagnostic-1010__proxy-classes__bench_proxy-classes_control_trap_direct.self.txt) |
| jetstream/hash-map | seconds | 1.207650 | 0.840744 | 1.436 | yes | https://github.com/pmatos/jsse/issues/1029 | [profile](profiles/jetstream__hash-map.self.txt) |
| jetstream/bigint-noble-ed25519 | seconds | 0.516195 | 0.368121 | 1.402 | yes | https://github.com/pmatos/jsse/issues/1026 | [profile](profiles/jetstream__bigint-noble-ed25519.self.txt) |
| diagnostic-1010/calls/bench_calls_closure_create | seconds | 0.190014 | 0.135870 | 1.399 | yes | https://github.com/pmatos/jsse/issues/1028 | [profile](profiles/diagnostic-1010__calls__bench_calls_closure_create.self.txt) |
| diagnostic-1010/bigint/bench_bigint_mulmod256_vm | seconds | 0.209128 | 0.149589 | 1.398 | yes | https://github.com/pmatos/jsse/issues/1026 | [profile](profiles/diagnostic-1010__bigint__bench_bigint_mulmod256_vm.self.txt) |
| jsse/bench_closure_create | seconds | 0.189865 | 0.136955 | 1.386 | yes | https://github.com/pmatos/jsse/issues/1028 | [profile](profiles/jsse__bench_closure_create.self.txt) |
| diagnostic-1010/numeric/bench_numeric_intarith | seconds | 0.222893 | 0.161888 | 1.377 | yes | https://github.com/pmatos/jsse/issues/1030 | [profile](profiles/diagnostic-1010__numeric__bench_numeric_intarith.self.txt) |
| diagnostic-1010/calls/bench_calls_closure_bigbody | seconds | 0.170301 | 0.124055 | 1.373 | yes | https://github.com/pmatos/jsse/issues/1028 | [profile](profiles/diagnostic-1010__calls__bench_calls_closure_bigbody.self.txt) |
| diagnostic-1010/bigint/bench_bigint_ops_vm_numadd | seconds | 0.116855 | 0.085420 | 1.368 | yes | https://github.com/pmatos/jsse/issues/1030 | [profile](profiles/diagnostic-1010__bigint__bench_bigint_ops_vm_numadd.self.txt) |
| jetstream/bigint-noble-secp256k1 | seconds | 0.446441 | 0.329564 | 1.355 | yes | https://github.com/pmatos/jsse/issues/1026 | [profile](profiles/jetstream__bigint-noble-secp256k1.self.txt) |
| diagnostic-1010/proxy-classes/bench_proxy-classes_proxy_vue_reactive | seconds | 0.489910 | 0.371364 | 1.319 | yes | https://github.com/pmatos/jsse/issues/1029 | [profile](profiles/diagnostic-1010__proxy-classes__bench_proxy-classes_proxy_vue_reactive.self.txt) |
| audit-controls/params-simple | seconds | 0.185810 | 0.141999 | 1.309 | yes | https://github.com/pmatos/jsse/issues/1023 | [profile](profiles/audit-controls__params-simple.self.txt) |
| diagnostic-1010/calls/bench_calls_leaf | seconds | 0.339913 | 0.260312 | 1.306 | yes | https://github.com/pmatos/jsse/issues/1023 | [profile](profiles/diagnostic-1010__calls__bench_calls_leaf.self.txt) |
| diagnostic-1010/proxy-classes/bench_proxy-classes_field_arrow_fn | seconds | 0.232317 | 0.178788 | 1.299 | yes | https://github.com/pmatos/jsse/issues/1028 | [profile](profiles/diagnostic-1010__proxy-classes__bench_proxy-classes_field_arrow_fn.self.txt) |
| diagnostic-1010/alloc-gc/bench_alloc-gc_hashmap_js | seconds | 0.533490 | 0.411630 | 1.296 | yes | https://github.com/pmatos/jsse/issues/1030 | [profile](profiles/diagnostic-1010__alloc-gc__bench_alloc-gc_hashmap_js.self.txt) |
| audit-controls/opmix-arith | seconds | 1.048314 | 0.817411 | 1.282 | yes | https://github.com/pmatos/jsse/issues/1030 | [profile](profiles/audit-controls__opmix-arith.self.txt) |
| diagnostic-1010/calls/bench_calls_fib | seconds | 0.378579 | 0.298703 | 1.267 | yes | https://github.com/pmatos/jsse/issues/1023 | [profile](profiles/diagnostic-1010__calls__bench_calls_fib.self.txt) |
| boa-scripts/basic/closure | seconds | 1.398179 | 1.106146 | 1.264 | yes | https://github.com/pmatos/jsse/issues/1022 | [profile](profiles/boa-scripts__basic__closure.self.txt) |
| diagnostic-1010/proxy-classes/bench_proxy-classes_proxy_ownkeys_has | seconds | 0.368229 | 0.294789 | 1.249 | yes | https://github.com/pmatos/jsse/issues/1029 | [profile](profiles/diagnostic-1010__proxy-classes__bench_proxy-classes_proxy_ownkeys_has.self.txt) |
| jetstream-registry/crypto-sha1-SP | seconds | 0.642218 | 0.515602 | 1.246 | yes | https://github.com/pmatos/jsse/issues/1023 | [profile](profiles/jetstream-registry__crypto-sha1-SP.self.txt) |
| diagnostic-1010/proxy-classes/bench_proxy-classes_proxy_set_plain | seconds | 0.358384 | 0.288170 | 1.244 | no | https://github.com/pmatos/jsse/issues/1029 | [profile](profiles/diagnostic-1010__proxy-classes__bench_proxy-classes_proxy_set_plain.self.txt) |
| audit-controls/args-parameters | seconds | 0.194627 | 0.157222 | 1.238 | yes | https://github.com/pmatos/jsse/issues/1023 | [profile](profiles/audit-controls__args-parameters.self.txt) |
| diagnostic-1010/calls/bench_calls_closure_call | seconds | 0.314382 | 0.254026 | 1.238 | yes | https://github.com/pmatos/jsse/issues/1023 | [profile](profiles/diagnostic-1010__calls__bench_calls_closure_call.self.txt) |
| diagnostic-1010/strings-arrays/fn-scope/bench_strings-arrays_arr_literals | seconds | 0.235562 | 0.190370 | 1.237 | yes | https://github.com/pmatos/jsse/issues/1028 | [profile](profiles/diagnostic-1010__strings-arrays__fn-scope__bench_strings-arrays_arr_literals.self.txt) |
| jsse/bench_fib | seconds | 0.373312 | 0.301767 | 1.237 | yes | https://github.com/pmatos/jsse/issues/1023 | [profile](profiles/jsse__bench_fib.self.txt) |
| audit-controls/call-sloppy-captured | seconds | 0.506761 | 0.411351 | 1.232 | yes | https://github.com/pmatos/jsse/issues/1023 | [profile](profiles/audit-controls__call-sloppy-captured.self.txt) |
| audit-controls/opmix-called | seconds | 1.750740 | 1.426298 | 1.227 | yes | https://github.com/pmatos/jsse/issues/1023 | [profile](profiles/audit-controls__opmix-called.self.txt) |
| jetstream/delta-blue | seconds | 0.496181 | 0.404280 | 1.227 | yes | https://github.com/pmatos/jsse/issues/1029 | [profile](profiles/jetstream__delta-blue.self.txt) |
| diagnostic-1010/generators/bench_generators_simple_yield | seconds | 0.382595 | 0.312079 | 1.226 | yes | https://github.com/pmatos/jsse/issues/1031 | [profile](profiles/diagnostic-1010__generators__bench_generators_simple_yield.self.txt) |
| boa-scripts/v8-benches/deltablue | V8 score | 281.000000 | 343.000000 | 1.221 | yes | https://github.com/pmatos/jsse/issues/1029 | [profile](profiles/boa-scripts__v8-benches__deltablue.self.txt) |
| diagnostic-1010/strings-arrays/fn-scope/bench_strings-arrays_ctl_scalar_fn | seconds | 0.185346 | 0.152431 | 1.216 | yes | https://github.com/pmatos/jsse/issues/1030 | [profile](profiles/diagnostic-1010__strings-arrays__fn-scope__bench_strings-arrays_ctl_scalar_fn.self.txt) |
| jetstream-registry/crypto-md5-SP | seconds | 0.570709 | 0.469524 | 1.216 | yes | https://github.com/pmatos/jsse/issues/1023 | [profile](profiles/jetstream-registry__crypto-md5-SP.self.txt) |
| diagnostic-1010/proxy-classes/bench_proxy-classes_public_access | seconds | 0.125291 | 0.103556 | 1.210 | yes | https://github.com/pmatos/jsse/issues/1029 | [profile](profiles/diagnostic-1010__proxy-classes__bench_proxy-classes_public_access.self.txt) |
| diagnostic-1010/bigint/bench_bigint_bigdenary_real | seconds | 0.402333 | 0.333097 | 1.208 | yes | https://github.com/pmatos/jsse/issues/1023 | [profile](profiles/diagnostic-1010__bigint__bench_bigint_bigdenary_real.self.txt) |
| audit-controls/call-sloppy-local | seconds | 0.451864 | 0.374231 | 1.207 | yes | https://github.com/pmatos/jsse/issues/1023 | [profile](profiles/audit-controls__call-sloppy-local.self.txt) |
| diagnostic-1010/proxy-classes/bench_proxy-classes_proxy_get | seconds | 0.799887 | 0.662584 | 1.207 | yes | https://github.com/pmatos/jsse/issues/1029 | [profile](profiles/diagnostic-1010__proxy-classes__bench_proxy-classes_proxy_get.self.txt) |
| boa-scripts/closures/invoke | seconds | 0.066523 | 0.055132 | 1.207 | no | https://github.com/pmatos/jsse/issues/1023 | [profile](profiles/boa-scripts__closures__invoke.self.txt) |
| diagnostic-1010/bigint/bench_bigint_bigdenary_numeric_control | seconds | 0.360534 | 0.298941 | 1.206 | yes | https://github.com/pmatos/jsse/issues/1023 | [profile](profiles/diagnostic-1010__bigint__bench_bigint_bigdenary_numeric_control.self.txt) |
| jetstream-registry/PluralRules-intl | seconds | 0.073895 | 0.061301 | 1.205 | yes | https://github.com/pmatos/jsse/issues/1033 | [profile](profiles/jetstream-registry__PluralRules-intl.self.txt) |
| jetstream/splay | seconds | 0.943974 | 0.785845 | 1.201 | yes | https://github.com/pmatos/jsse/issues/1027 | [profile](profiles/jetstream__splay.self.txt) |
| diagnostic-1013/bench_array_literal | seconds | 0.321162 | 0.267709 | 1.200 | yes | https://github.com/pmatos/jsse/issues/1028 | [profile](profiles/diagnostic-1013__bench_array_literal.self.txt) |
| jetstream/bigint-bigdenary | seconds | 0.400575 | 0.334057 | 1.199 | yes | https://github.com/pmatos/jsse/issues/1023 | [profile](profiles/jetstream__bigint-bigdenary.self.txt) |
| audit-controls/call-strict-captured | seconds | 0.465468 | 0.391130 | 1.190 | yes | https://github.com/pmatos/jsse/issues/1023 | [profile](profiles/audit-controls__call-strict-captured.self.txt) |
| diagnostic-1010/alloc-gc/bench_alloc-gc_splay_mini | seconds | 0.191899 | 0.161936 | 1.185 | yes | https://github.com/pmatos/jsse/issues/1027 | [profile](profiles/diagnostic-1010__alloc-gc__bench_alloc-gc_splay_mini.self.txt) |
| boa-scripts/properties/access | seconds | 0.045700 | 0.038919 | 1.174 | yes | https://github.com/pmatos/jsse/issues/1029 | [profile](profiles/boa-scripts__properties__access.self.txt) |
| jetstream-extra/threejs | seconds | 30.167191 | 25.789649 | 1.170 | yes | https://github.com/pmatos/jsse/issues/1027 | [profile](profiles/jetstream-extra__threejs.self.txt) |
| audit-controls/call-strict-local | seconds | 0.408157 | 0.349320 | 1.168 | yes | https://github.com/pmatos/jsse/issues/1023 | [profile](profiles/audit-controls__call-strict-local.self.txt) |
| diagnostic-1010/strings-arrays/no-gap/fn-scope/bench_strings-arrays_str_keys_control | seconds | 0.216183 | 0.186289 | 1.160 | yes | https://github.com/pmatos/jsse/issues/1030 | [profile](profiles/diagnostic-1010__strings-arrays__no-gap__fn-scope__bench_strings-arrays_str_keys_control.self.txt) |
| jetstream-registry/date-format-tofte-SP | seconds | 0.137388 | 0.119070 | 1.154 | yes | https://github.com/pmatos/jsse/issues/1028 | [profile](profiles/jetstream-registry__date-format-tofte-SP.self.txt) |
| jetstream-registry/n-body-SP | seconds | 0.412315 | 0.359337 | 1.147 | yes | https://github.com/pmatos/jsse/issues/1029 | [profile](profiles/jetstream-registry__n-body-SP.self.txt) |
| diagnostic-1010/proxy-classes/bench_proxy-classes_map_lookup | seconds | 0.055216 | 0.048132 | 1.147 | yes | https://github.com/pmatos/jsse/issues/1022 | [profile](profiles/diagnostic-1010__proxy-classes__bench_proxy-classes_map_lookup.self.txt) |
| boa-scripts/intl/datetimeformat-format | seconds | 0.009047 | 0.007888 | 1.147 | no | https://github.com/pmatos/jsse/issues/1033 | [profile](profiles/boa-scripts__intl__datetimeformat-format.self.txt) |
| diagnostic-1010/bigint/bench_bigint_shift_bitops | seconds | 0.088346 | 0.078016 | 1.132 | yes | https://github.com/pmatos/jsse/issues/1026 | [profile](profiles/diagnostic-1010__bigint__bench_bigint_shift_bitops.self.txt) |
| diagnostic-1010/strings-arrays/fn-scope/bench_strings-arrays_arr_sort_cmp | seconds | 0.511916 | 0.453868 | 1.128 | yes | https://github.com/pmatos/jsse/issues/1023 | [profile](profiles/diagnostic-1010__strings-arrays__fn-scope__bench_strings-arrays_arr_sort_cmp.self.txt) |
| jetstream/Box2D | seconds | 0.454732 | 0.406448 | 1.119 | yes | https://github.com/pmatos/jsse/issues/1029 | [profile](profiles/jetstream__Box2D.self.txt) |
| boa-scripts/basic/call-loop | seconds | 0.016670 | 0.014918 | 1.117 | no | https://github.com/pmatos/jsse/issues/1023 | [profile](profiles/boa-scripts__basic__call-loop.self.txt) |
| diagnostic-1010/proxy-classes/bench_proxy-classes_proxy_set | seconds | 0.575787 | 0.516099 | 1.116 | yes | https://github.com/pmatos/jsse/issues/1029 | [profile](profiles/diagnostic-1010__proxy-classes__bench_proxy-classes_proxy_set.self.txt) |
| diagnostic-1010/proxy-classes/bench_proxy-classes_proxy_notrap | seconds | 0.395475 | 0.355963 | 1.111 | yes | https://github.com/pmatos/jsse/issues/1029 | [profile](profiles/diagnostic-1010__proxy-classes__bench_proxy-classes_proxy_notrap.self.txt) |
| boa-scripts/v8-benches/raytrace | V8 score | 603.000000 | 665.000000 | 1.103 | yes | https://github.com/pmatos/jsse/issues/1029 | [profile](profiles/boa-scripts__v8-benches__raytrace.self.txt) |
| diagnostic-1010/promises/bench_promises_queue_deep | seconds | 0.808132 | 0.732927 | 1.103 | yes | https://github.com/pmatos/jsse/issues/1027 | [profile](profiles/diagnostic-1010__promises__bench_promises_queue_deep.self.txt) |
| diagnostic-1010/bigint/bench_bigint_modpow | seconds | 0.359985 | 0.327006 | 1.101 | yes | https://github.com/pmatos/jsse/issues/1026 | [profile](profiles/diagnostic-1010__bigint__bench_bigint_modpow.self.txt) |
| jetstream/bigint-noble-bls12-381 | seconds | 5.567717 | 5.078894 | 1.096 | yes | https://github.com/pmatos/jsse/issues/1026 | [profile](profiles/jetstream__bigint-noble-bls12-381.self.txt) |
| jetstream/sync-fs | seconds | 0.666752 | 0.609395 | 1.094 | yes | https://github.com/pmatos/jsse/issues/1031 | [profile](profiles/jetstream__sync-fs.self.txt) |
| jetstream-registry/esprima-next-wtb | seconds | 4.056402 | 3.710360 | 1.093 | yes | https://github.com/pmatos/jsse/issues/1029 | [profile](profiles/jetstream-registry__esprima-next-wtb.self.txt) |
| diagnostic-1010/calls/bench_calls_nested | seconds | 0.240315 | 0.219919 | 1.093 | yes | https://github.com/pmatos/jsse/issues/1023 | [profile](profiles/diagnostic-1010__calls__bench_calls_nested.self.txt) |
| jetstream-registry/3d-raytrace-SP | seconds | 0.395097 | 0.369307 | 1.070 | yes | https://github.com/pmatos/jsse/issues/1029 | [profile](profiles/jetstream-registry__3d-raytrace-SP.self.txt) |
| jetstream/proxy-mobx | seconds | 0.082082 | 0.077939 | 1.053 | yes | https://github.com/pmatos/jsse/issues/1029 | [profile](profiles/jetstream__proxy-mobx.self.txt) |
| audit-controls/opmix-mixed | seconds | 1.271306 | 1.209396 | 1.051 | no | https://github.com/pmatos/jsse/issues/1030 | [profile](profiles/audit-controls__opmix-mixed.self.txt) |
| diagnostic-1010/numeric/bench_numeric_bail_throw | seconds | 0.115729 | 0.111177 | 1.041 | yes | https://github.com/pmatos/jsse/issues/1030 | [profile](profiles/diagnostic-1010__numeric__bench_numeric_bail_throw.self.txt) |
| diagnostic-1010/calls/bench_calls_method | seconds | 0.464600 | 0.446894 | 1.040 | yes | https://github.com/pmatos/jsse/issues/1023 | [profile](profiles/diagnostic-1010__calls__bench_calls_method.self.txt) |
| diagnostic-1010/numeric/bench_numeric_bail_typeof | seconds | 0.116367 | 0.112301 | 1.036 | yes | https://github.com/pmatos/jsse/issues/1030 | [profile](profiles/diagnostic-1010__numeric__bench_numeric_bail_typeof.self.txt) |
| diagnostic-1010/strings-arrays/bench_strings-arrays_arr_sort_cmp | seconds | 0.551284 | 0.535049 | 1.030 | yes | https://github.com/pmatos/jsse/issues/1023 | [profile](profiles/diagnostic-1010__strings-arrays__bench_strings-arrays_arr_sort_cmp.self.txt) |
| diagnostic-1010/numeric/bench_numeric_bail_switch | seconds | 0.115661 | 0.112298 | 1.030 | yes | https://github.com/pmatos/jsse/issues/1030 | [profile](profiles/diagnostic-1010__numeric__bench_numeric_bail_switch.self.txt) |
| diagnostic-1010/numeric/bench_numeric_bail_memberupdate | seconds | 0.115251 | 0.111901 | 1.030 | yes | https://github.com/pmatos/jsse/issues/1030 | [profile](profiles/diagnostic-1010__numeric__bench_numeric_bail_memberupdate.self.txt) |
| diagnostic-1010/generators/bench_generators_tokenizer | seconds | 0.312185 | 0.303953 | 1.027 | yes | https://github.com/pmatos/jsse/issues/1031 | [profile](profiles/diagnostic-1010__generators__bench_generators_tokenizer.self.txt) |
| diagnostic-1010/numeric/bench_numeric_bail_arrayliteral | seconds | 0.114769 | 0.111820 | 1.026 | yes | https://github.com/pmatos/jsse/issues/1030 | [profile](profiles/diagnostic-1010__numeric__bench_numeric_bail_arrayliteral.self.txt) |
| diagnostic-1010/numeric/bench_numeric_bail_letconst | seconds | 0.115197 | 0.112785 | 1.021 | no | https://github.com/pmatos/jsse/issues/1030 | [profile](profiles/diagnostic-1010__numeric__bench_numeric_bail_letconst.self.txt) |
| diagnostic-1010/numeric/bench_numeric_bail_none | seconds | 0.115149 | 0.112985 | 1.019 | yes | https://github.com/pmatos/jsse/issues/1030 | [profile](profiles/diagnostic-1010__numeric__bench_numeric_bail_none.self.txt) |
| diagnostic-1013/bench_script_handoff_loop | seconds | 0.821364 | 0.814709 | 1.008 | yes | https://github.com/pmatos/jsse/issues/1030 | [profile](profiles/diagnostic-1013__bench_script_handoff_loop.self.txt) |
| diagnostic-1010/strings-arrays/fn-scope/bench_strings-arrays_arr_sort_default | seconds | 0.535579 | 0.533480 | 1.004 | no | https://github.com/pmatos/jsse/issues/1030 | [profile](profiles/diagnostic-1010__strings-arrays__fn-scope__bench_strings-arrays_arr_sort_default.self.txt) |
| jetstream/Babylon | seconds | 0.257647 | 0.257124 | 1.002 | yes | https://github.com/pmatos/jsse/issues/1029 | [profile](profiles/jetstream__Babylon.self.txt) |
| boa-scripts/intl/datetimeformat_resolved_options | seconds | 0.007320 | 0.007314 | 1.001 | no | https://github.com/pmatos/jsse/issues/1033 | [profile](profiles/boa-scripts__intl__datetimeformat_resolved_options.self.txt) |

## Phases and longer paired controls

The opmix aggregate slightly favors JSSE, but that hides slower arithmetic and call phases. The longer paired controls and their independent profiles are the basis of #1023/#1030.

| Op-mix phase | JSSE median ms | Boa median ms | JSSE / Boa |
|---|---:|---:|---:|
| arith | 105 | 82 | 1.280 |
| elem | 223 | 284 | 0.785 |
| called | 360 | 281 | 1.281 |
| mixed | 407 | 402 | 1.012 |
| labeledLoop | 187 | 248 | 0.754 |
| labeledControl | 1089 | 1127 | 0.966 |

## Differential diagnosis and falsification

- **Prettier:** JSSE fails in both execution modes. Reduced to an async function returning a function declared after return, and a generator using a function declared after yield. Whole-body declaration instantiation is missing before state-machine fragments execute. Node and Boa pass. See #1032 and `issues/*hoist-evidence.json` / `issues/async-function-minimal-evidence.json`.
- **SSR console:** HTML validation succeeds, but JSSE emits logging that the workload disabled through `globalThis.console`. A lexical console binding is disconnected from the global object. This is a host compatibility defect (#1021), not a core ECMAScript conformance claim.
- **PluralRules:** a separate 500,000-select probe validates the checksum on all engines and takes 0.265434 s versus Boa 0.138281 s (1.920x). Locale parsing/fallback and rule reconstruction dominate a substantial part of the profile; zero AST work/GC/slot misses. See #1033 and `issues/pluralrules-*`.
- **Lexical free binding:** moving only n=3000 into the called function reduces the five-repeat JSSE/Boa ratio from 1.191 to 1.061. The independent var/let loop pair is 0.917 versus 1.540. See `investigation-measurements.json` and #1022.
- **Allocator falsification:** preloading jemalloc leaves the closure gap unchanged: JSSE 0.066146 s versus 0.066581 s with jemalloc; Boa 0.035369 s. Constructor churn is unchanged too. An allocator swap alone is not supported as the fix (#1028).
- **DateTimeFormat validation:** the upstream aggregate minimum-length assertion fails for JSSE (435674 versus required 438000) and Boa (352316). An 800-case follow-up over 25 locales, 16 style combinations and local/UTC zones has zero errors and formatToParts joined text equals format in all cases on all engines. Locale text/data differ; no spec defect has been established from this length floor. Evidence: `issues/intl-consistency-*.json`.
- **WSL:** JSSE completes the entire validation workload in 147.70 s; Boa rejects before execution. There is no valid comparative speed ratio. A redundant repetition was canceled after the completed validated run; it is recorded separately in `canceled-runs.json`.
- **Oracle/harness corrections:** Node executes bundles as classic Scripts with vm, and binary scene preloads return the upstream Int8Array shape. Three historical print-only diagnostics received an equivalent print shim. Superseded adapter results are retained in `adapter-preflight.jsonl` and are not counted as engine bugs.
- **Boa expression scripts:** most original tiny core scripts leave a final expression that Boa displays and Node/JSSE do not. The textual mismatches in those rows are CLI display behavior. All 23 separate expression-output validation variants pass on both engines. Undefined-returning Intl scripts establish completion only, not output equivalence.
- **Profile interpretation:** fat LTO folds some identical helpers into generator-named symbols even in generator-free programs; these names alone do not establish generator execution. Sampling profiles and counter runs are separate evidence, and counter-run GC times are never divided by default-binary wall times.

## Selected JSSE wins

These cold workload results provide context, not an overall engine ranking. No pooled geomean is reported: overlapping diagnostics, startup costs and different score definitions would bias it.

| Workload | JSSE seconds | Boa seconds | Boa / JSSE |
|---|---:|---:|---:|
| jetstream-extra/babylonjs-startup-es5 | 1.341465 | 52.104149 | 38.841 |
| jetstream-extra/babylonjs-startup-es6 | 0.970214 | 34.426224 | 35.483 |
| jetstream-extra/jsdom-d3-startup | 6.986586 | 14.111265 | 2.020 |
| jetstream/mandreel | 10.387797 | 12.992147 | 1.251 |
| jetstream-registry/regex-dna-SP | 0.027907 | 0.170811 | 6.121 |
| audit-controls/loop-var | 0.166327 | 0.181432 | 1.091 |

## Execution failures

Errors and benchmark-validation failures are retained as failures, never speedups. Boa failures are recorded for comparison; the requested issue scope is JSSE. The JSSE failures are classified above.

- `boa-scripts/intl/numberformat-different-options` on boa: error. `Uncaught Error: TypeError: unimplemented (/home/pmatos/dev/jsse/docs/perf/2026-10-14/boa-head-audit/workloads/boa-scripts__intl__numberformat-different-options.js:12:24)`
- `jetstream/Basic` on boa: error. `Uncaught Error: TypeError: cannot convert 'null' or 'undefined' to object (/home/pmatos/dev/jsse/docs/perf/2026-10-14/boa-head-audit/workloads/jetstream__Basic.js:1449:3)`
- `jetstream/OfflineAssembler` on boa: error. `Uncaught Error: "Line #42 differs.  Expected: \"const TagTypeNumber = 18446462598732840000\", got \"const TagTypeNumber = 18446462598732839000\""`
- `jetstream/typescript-octane` on boa: error. `Uncaught Error: RangeError: bad length for array (/home/pmatos/dev/jsse/docs/perf/2026-10-14/boa-head-audit/workloads/jetstream__typescript-octane.js:25993:3)`
- `jetstream/validatorjs` on boa: error. `Uncaught Error: Error: Assertion failure: 2010-07-02,[object Object] (/home/pmatos/dev/jsse/docs/perf/2026-10-14/boa-head-audit/workloads/jetstream__validatorjs.js:21511:3)`
- `jetstream-registry/DateTimeFormat-intl` on jsse: error. `{"error":"Error: Assertion failed: Invalid totalLength = 435674, expected >= 438000"}`
- `jetstream-registry/DateTimeFormat-intl` on boa: error. `Error: Assertion failed: Invalid totalLength = 352316, expected >= 438000`
- `jetstream-registry/NumberFormat-intl` on boa: error. `TypeError: unimplemented`
- `jetstream-registry/RelativeTimeFormat-intl` on boa: error. `TypeError: undefined is not a constructor`
- `jetstream-registry/WSL` on boa: error. `Uncaught Error: TypeError: cannot convert 'null' or 'undefined' to object (/home/pmatos/dev/jsse/docs/perf/2026-10-14/boa-head-audit/workloads/jetstream-registry__WSL.js:19710:283)`
- `jetstream-registry/babel-wtb` on boa: error. `Error: `
- `jetstream-registry/prettier-wtb` on jsse: error. `{"error":"ReferenceError: s is not defined"}`
- `jetstream-registry/prismjs-startup-es5` on boa: error. `Error: Assertion failed: Invalid result.hash = 173635432, expected -270772291`
- `jetstream-registry/prismjs-startup-es6` on boa: error. `Error: Assertion failed: Invalid result.hash = 173635432, expected -270772291`
- `jetstream-registry/source-map-wtb` on jsse: error. `{"error":"ReferenceError: WebAssembly is not defined"}`
- `jetstream-registry/source-map-wtb` on boa: error. `Error: `

## Reproduction and evidence

- `environment.json`: full revisions, build commands, compiler/Node versions, CPU information and binary hashes.
- `manifest.json`: every source path, generated script path, SHA-256 and original JetStream iteration count. `workloads.tar.zst` retains all immutable sources; scripts extract it automatically if workloads/ is absent.
- `upstream-registry.json` and `exclusions.json`: complete upstream registry inventory and unsupported leaves.
- `measurements.jsonl`: every timed process, output, status and contemporaneous load averages.
- `summary.json` and `results.csv`: medians, min/max ranges, validation outcomes and ratios.
- `profiles/`: self/inclusive sampling reports and deterministic execution counters. `profile-index.json` points to raw sampling data in the temporary audit directory.
- `issues/`: exact issue bodies and minimized repros. `issue-map.json` maps workloads to issue URLs.

Build the pinned engines, then run from this directory (the harness resumes existing results; use a copied evidence directory with measurements.jsonl removed for a fresh run):

```sh
uv run python audit.py --jsse /path/to/jsse --boa /path/to/boa
uv run python profile.py --scratch /path/to/audit-scratch
uv run python report.py
```

The retained workload scripts are immutable inputs to this run. The audit changed no engine implementation, spec/test262 submodule or conformance baseline. The full test262 suite was not rerun: this is a benchmark and differential-validation audit, not a new conformance certification.
