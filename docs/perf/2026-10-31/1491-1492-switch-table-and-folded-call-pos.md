# #1491 switch dispatch table and #1492 folded call positions

Both come from the `--node` profile of the TypeScript suite (#1297, #1418): `compiler-first` of
`scripts/run-ts-budget-oracle.py` (10 compiler suites, 60 tests) executes 1655 M VM ops, of which 42.8 M
`CaseStrictEq` (plus the `LoadConst`/`Pop` that evaluate and drop each selector) and 95.1 M `SetCallPos`.

## What changed

**#1491.** A `switch` whose case selectors are all number or string literals (`-1` counts; at least 3 of them)
compiles to `<discriminant>; SwitchTable t` followed by the clause bodies in source order. `Chunk::switch_tables[t]`
(`src/interpreter/bytecode/switch_table.rs`) maps the discriminant to the code offset of its clause: a dense array
for integers in a span of at most 4096, a hash for the rest, a hash on the UTF-16 units for strings, the `default`
body (or the end of the statement) for anything else. Literal selectors have no effects, so CaseBlockEvaluation
(ECMA-262 14.12.4) reduces to finding the first clause whose selector is `===` the discriminant: no coercion (a
number only matches a number, a string a string, BigInt, objects and symbols match nothing), `+0` and `-0` share a key,
`NaN` has none, and the first of several equal selectors wins. Any other selector (identifier, member, BigInt, call,
...) keeps the linear `CaseStrictEq` chain unchanged, so lazy in-order selector evaluation is untouched. The
tree-walker is unchanged.

**#1492.** Under the `--node` stack hooks the call ops (`Call`, `ReturnCall`, `Construct`) carry the call
position themselves: bit 15 of the argument-count operand (`CALL_POS_FLAG`) says a u32 position follows the call-site
id, and `vm/call_operands.rs` stores it in `node_host.call_pos` while decoding the operands. There is no `SetCallPos`
op before those calls any more. Default chunks never set the bit, so they are byte-identical to before
(`tests_hook_isolation.rs` pins both layouts). `SetCallPos` stays for the spread calls and `throw`, which are cold
(8.5 k executions in `compiler-first`), and for a call with 32768 or more arguments.

## Measurement

Execution counters (`cargo build --profile release-nolto --features perf-counters`, deterministic), `jsse --node`
over `TS_SRC_DIR` v6.0.3 through `scripts/run-ts-budget-oracle.py`:

| case | VM ops before | after | change |
|---|---|---|---|
| `bundle-load` | 566,645,829 | 526,677,759 | -7.05% |
| `compiler-first` | 1,655,096,334 | 1,473,753,936 | -10.96% |

| op in `compiler-first` | before | after |
|---|---|---|
| `SetCallPos` | 95,096,840 | 8,522 |
| `CaseStrictEq` | 42,818,649 | 573,875 |
| `SwitchTable` | 0 | 1,712,627 |
| `LoadConst` | 99,773,023 | 57,528,225 |
| `Pop` | 92,786,969 | 91,074,312 |
| `Jump` | 75,574,370 | 73,861,675 |

In `bundle-load` `SetCallPos` goes 39,808,814 -> 1,024 and `CaseStrictEq` 57,290 -> 21. The `CaseStrictEq` that
remain are switches with a non-literal selector. A scanner-shaped micro benchmark (30 integer cases called 3 M times
plus 8 string cases called 1 M times) executes 248.1 M ops before and 74.0 M after; a call-bound `--node` loop
(`fib(30)` plus 2 M two-call iterations) 54.3 M before and 47.6 M after.

## Wall time (fat-LTO `cargo build --release`, base and new each in its own target dir, shared 24-core host)

Micro benchmarks, 7 interleaved runs each (min / median / max, seconds):

| benchmark | base | new |
|---|---|---|
| `sw-bench` (switch scanner shape) | 1.239 / 1.272 / 1.570 | 0.894 / 1.007 / 1.369 |
| `call-bench` (`--node`, call bound) | 0.507 / 0.752 / 0.775 | 0.511 / 0.752 / 0.772 |

The switch benchmark is 21% (median) to 28% (min) faster. Removing `SetCallPos` is invisible on the clock: a dispatch
of that op is a store, so the 12% fewer ops of `call-bench` do not move the wall time.

The TypeScript cases (`bundle-load`, `compiler-first`, 3 interleaved runs per binary, seconds, `jsse --node`) are
inside the host's noise and no wall-time claim is made for them: `bundle-load` base 22.2 / 30.8 / 25.8, new 29.4 /
25.9 / 21.8; `compiler-first` base 78.3 / 105.6 / 72.0, new 88.9 / 99.1 / 70.7. The op counts above are the evidence.
