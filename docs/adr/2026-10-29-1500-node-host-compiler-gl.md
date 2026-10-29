# Node host mode: the compiler runner, test files G to L

Status: accepted, epic #1214, issue #1271 (uses `2026-10-28-1900-node-host-compiler-runner.md`).

## Context

`ts-suite/oracle.json` pins 7848 passing `compiler` tests (1308 suites, 6 tests each) for files whose name starts with
G to L: G 1332, H 54, I 4290, J 1500, K 90, L 582. The compiler oracle compares them per test, per failure message and
per local baseline file between Node and `jsse --node`; this issue runs that range to parity.

## Result

Node passes 7848/7848. jsse passes 7847 at first run; the one failure is `largeControlFlowGraph.ts`
(`"before all" hook ... Maximum call stack size exceeded`). Per letter, jsse process time in the first full runs
(3 to 4 jobs, `CARGO_PROFILE_RELEASE_LTO=off`, host shared with other agents):

| letter | tests | wall   | jsse process time |
|--------|------:|-------:|------------------:|
| G      |  1332 | 1083 s |            2538 s |
| H      |    54 |  367 s |             365 s |
| I      |  4290 | 1591 s |            4397 s |
| J      |  1500 | 1301 s |            3454 s |
| K      |    90 |  142 s |             140 s |
| L      |   582 |  498 s |            1282 s |

## Root cause and fix: the call-depth limit

`largeControlFlowGraph.ts` is a 10000-statement block of `data[0] = 0;`. TypeScript's `getTypeAtFlowNode` recurses
up to `flowDepth === 2000` and then reports TS2563 ("The containing function or module body is too large for control
flow analysis"), the reference baseline. Reaching that depth takes more than 4000 nested JS calls (it overflowed at a
hard limit of 5000 and completes at 10000). jsse's release soft limit was 4000
(`CALL_DEPTH_SOFT_LIMIT_RELEASE`, hard 5000), below Node's 12,523 frames for a plain function (3,296 levels through
`Array.prototype.map`) on this host, so the engine threw `RangeError: Maximum call stack size exceeded` first.

ECMA-262 leaves recursion depth implementation-defined, so this is a capacity choice, not a semantics change. The
release limits are now soft 8000, hard 10000, rearm 6000 (the ratios of the old numbers), and `EVAL_DEPTH_LIMIT_RELEASE`
is 60000 so that the compile-time `EVAL_DEPTH_LIMIT > HARD * 5` coupling holds. The debug limits are unchanged.
Calibration: the 128 MiB engine stack held 30,300 plain frames at the old measurement, and the guard unit tests
(`deep_call_recursion_raises_error_before_native_overflow`, `deep_expression_nesting_raises_error_before_native_overflow`,
each driving the Proxy apply-trap forwarding shape and the member chain, the stack-hungriest ones, to twice the limit)
pass under `--profile release-nolto`. Sixteen further shapes (plain, Proxy apply and `get`, getter, `map`, `toJSON`,
`replace` callback, `Symbol.toPrimitive`, `bind`/`call`, `Reflect.apply`, constructors, generators, `eval`,
`Function`, `Array.from`, `spread`) end in a catchable RangeError with the final limits on the shipped fat-LTO release
build, both default (bytecode) and `--no-bytecode`, at most 87 MiB resident except `spread`, whose argument lists grow
quadratically (860 MiB at 10000; 1.4 GiB at a trial limit of 15000, which is why the limit stops at 10000). A recursive
`return super.m()` loops forever instead of overflowing because class code is strict and its calls are proper tail
calls, as before.
`tests/recursion-depth-reachable.js` pins the floor (plain 7000, through `map` 2000, constructor plus getter 2500);
`tests/recursion-limit-interpreter.js` keeps pinning the overflow (its Proxy case now recurses 20000 deep, above the
new hard limit). Full test262: 99911/99911 scenarios pass, no regression against the baseline; `test262-extra/`:
1646/1646.

With the new limit `largeControlFlowGraph.ts` passes on jsse (6/6, about 830 s for the one process against 5 s on
Node), so its mocha timeout needs more than the oracle's 600 s default: `scripts/run-ts-compiler-gl-oracle.py` passes
`--test-timeout 2400000 --timeout 7200`.

## Runnable

`TS_SRC_DIR=... uv run python scripts/run-ts-compiler-gl-oracle.py [--letters g,h] [oracle options]` runs the letters
(`scripts/ts_compiler_slices_gl.py`) one after another against `ts-suite/compiler-known-failures-g-l.txt`, which is
empty because nothing remains. Each oracle run still includes its corrupted-reference control.

## Alternatives

* List `largeControlFlowGraph.ts` as a known failure: rejected, the cause is a conservative engine limit that Node
  does not share, and the fix is spec-neutral.
* Raise the limit to 15000: also passes, but quadratic spread recursion then needs 1.4 GiB; 10000 is Node's order of
  magnitude.
* One oracle run for `g-l`: any crash or timeout loses all of it; a run per letter bounds the loss.
