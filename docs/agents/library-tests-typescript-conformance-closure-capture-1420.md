# Library Tests: closures keep dead locals of a returned call (#1420)

Issue #1420 (epic #1214): a closure holds its defining environment, so a callback that outlives the call that made it also keeps every local of that call. Two effects were claimed: Node's `test-abortsignal-drop-settled-signals.mjs` subtest `does not prevent source signal from being GCed if it is short-lived` had to be skipped on jsse, and the dead locals were a suspected contributor to the tsc self-compile RSS (#1401).

## What shipped: exit-time trimming (`src/interpreter/env_trim.rs`)

When a compiled or general user-function call returns and something still holds its environment (`Rc::strong_count` above the call's own references, i.e. a closure, or the body environment's parent link), the bindings that no closure can name are overwritten with `undefined`. The entry stays, so slot hints, the frame layout and the VM's `LoadLocal` verification are untouched and the tree-walker and the bytecode VM share one implementation (both reach it through `run_activation`'s callers: `call_compiled_function*` in `eval/activation.rs` and the general path of `call_function_inner_impl`).

- **What a closure can name** is the set of identifiers mentioned inside the functions, arrows, classes (heritage, computed keys, members, static blocks) and parameter defaults nested in the literal, at any depth, shadowing ignored (`CaptureWalk::nested_mentions` in `src/ast/capture_walk.rs`, `WalkMode::Record` finds the nested nodes and `WalkMode::Inside` collects every identifier in them). It is computed once per literal on the first captured exit and cached in `LiteralFrame::nested_names` (a 64-bit name filter, a scan up to 12 names, a hash set above).
- **Bail-outs** (nothing is trimmed): a direct `eval` or `with` in the literal or anything nested in it (closures made by `eval` are invisible to the walk), and a sloppy function with simple parameters, whose mapped `arguments` object (or `f.arguments`) aliases the parameter bindings and can outlive the call. Generators and async functions never reach the hook (their environment stays live across suspensions); scripts, modules and the global environment are not function activations. `this`/`new.target` are not touched.
- Only values that can hold memory are cleared (objects, strings, symbols, BigInts); numbers, booleans, `undefined` and `null` are skipped before the name probe.

Tests: `test262-extra/closure-captured-bindings-after-activation-returns.js` (every way a closure can name a binding still reads it after the call: reads, writes, shorthand, defaults, templates, class members, nested eval, `with`, mapped `arguments`, `f.arguments`, parameter-expression scopes, generators, arrows with `this`/`arguments`, per-iteration bindings), `tests/closure-dead-binding-collectable.js` (`WeakRef` + `$262.gc()`: a dead local is collected, a named one and one reachable by `eval` are not). The events oracle (`scripts/run-node-events-oracle.sh`) no longer skips the subtest: it failed on the previous binary (`test-abortsignal-drop-settled-signals.mjs: jsse --node exit 1`) and passes now, in the tree-walker and the VM.

## What did not help: tsc RSS

`compiler-dts` (`scripts/run-tsc-selfcompile-oracle.py`, LTO-off build, origin/main at 14698d37, which does not yet contain #1556):

| binary | jsse s | jsse MiB | ratio to Node (643 MiB) | output vs Node |
| --- | ---: | ---: | ---: | --- |
| before | 244.1 | 2992 | 4.7 | byte-identical |
| exit-time trimming | 238.7 | 3008 | 4.7 | byte-identical |

No change (the 16 MiB difference is noise and the mention sets). The cause is in the census: a temporary counter in the major collection, run during `tsc -p src/compiler --noEmit`, saw about 2.3 million live objects but only **13,100 user functions and 4,900 environments** (22,000 bindings: 320 bytes per `Environment` and 24 per binding, about 2 MiB in all). Closures and their environments are not where the heap is; the 2.3 million objects (AST nodes, symbols, types and their property storage, #1401) are. Further capture analysis (block and iteration environments, liveness inside a running activation, dropping the `Environment` of a closure that names nothing) cannot move the self-compile figure, so it was not done. The small `tsc --noEmit a.ts` run (lib.d.ts only) is also unchanged, 498 to 511 MiB.

Where it does help is a program that keeps many closures over big dead locals: `bench1.js`-style code keeping 300,000 closures of a function with an unused object local peaked at 423 MiB before and 322 MiB after (-24%). Cost: the walk runs once per literal; each exit that leaves the environment captured scans its bindings (about 560,000 exits in a small `tsc` run, a third of the 7 million bindings seen held a clearable value). A micro-benchmark that makes a closure in every call (3 closure-making calls and 1 plain call per iteration, 1.5 million iterations) measured about 5% more CPU (min of 7 alternating runs on a loaded host; the host's noise is larger than that), the `tsc` run showed none.

## Not done

Block and per-iteration environments are not trimmed when they are left (the VM's `PopEnv`/`unwind_scopes`/`CopyIterEnv` and the tree-walker's block scopes), nor is a live activation's dead local released while the call still runs; the repro of the issue is a finished function activation, which is covered.
