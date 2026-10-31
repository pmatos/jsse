# Library Tests: stack-frame leftovers (#1508)

Part of epic #1214 (tracker #1418). Follows `library-tests-typescript-conformance-stack-frames-1368.md`: the four
differences from Node 26 its "Remaining differences" listed. Two groups are fixed, the generator group is not.

## Fixed (`--node` only; default mode and both engines unchanged)

- **`at async Promise.all (index N)`** (also `allSettled`, `any`). V8's `at async` walk
  (`builtins/node_host_stack/async_frames.rs`) now recognises the element functions of the three combinators and
  continues from the combinator's result promise. `Promise.all`/`allSettled` are read from the single fulfill
  reaction's handler, `any` from the reject reaction's (its fulfill handler is the capability's resolve function), as
  V8 does. A pending promise's own resolving function, the handler `Promise.race` passes to `then`, is followed
  without a frame, so `race` keeps its tail. The element functions carry hidden slots (`combinator_frames.rs`: the
  result promise and `index * 4 + kind`), written only while the hooks are installed, so the collector traces the
  result promise through them and a recycled object id cannot alias another function. A promise with two reactions
  still ends the walk. `CallSite` of such a frame: `getTypeName() === "Promise"`, `getFunctionName()` and
  `getMethodName()` the combinator's name, `getPromiseIndex()` and `getPosition()` the index, `isPromiseAll()` true
  only for `all`, `getFileName()`/`getLineNumber()` `null`, `toString()` `async Promise.all (index 1)`.
- **` [as method]`** (`at Timeout.tm [as _onTimeout]`, `at Object.named [as other]`). The frame record has a sixth
  slot, the method name V8's `CallSiteInfo::GetMethodName` finds (`node_host_stack/method_name.rs`): the function's
  own name (without `get `/`set `) if the receiver has a property of that name holding the function, an accessor half
  included; else the one enumerable, non-index key along the receiver's prototype chain that holds it, none if two do.
  It reads property records only: no getter, trap or script runs. `CallSite.getMethodName()` returns it. The prelude
  prints the suffix unless the function name equals the method name or ends with `.` plus it, or is an accessor. Two
  rules it had wrong are fixed with it: V8 prefixes the type name even when the function name starts with it
  (`at Foo.FooBar`, `at process.processTimers`), and an element key never names a method (`a[0] = function () {}` has
  no `[as 0]`).
- **Timer frames.** The timers shim's frames now carry Node's names: `listOnTimeout` and `process.processTimers` for a
  timeout or interval, `process.processImmediate` for an immediate (the callbacks are bound to `process`). The
  position inside `node:internal/timers` differs, as it does for every other embedded module, and every oracle drops
  `node:internal` frames.

## Not fixed (#1529, sub-issue of #1418)

Generator bodies have no call frame at all, in either engine, so the group that depends on it stays open:

- A sync or async generator body runs without a frame: `new Error()` inside it prints the caller's frame at the
  generator's own position (`at main (f.js:8:26)` where Node prints `at sync (f.js:8:26)` then `at main (f.js:17:14)`).
- An async generator parked at an `await` or `yield` has no `at async gen` frame, and the walk stops at the
  generator's request promise, so `for await (const s of gen())` with an error raised in `gen` loses
  `at async caller`. Needs a frame for every body driver (bytecode chunk and state machine, sync and async), the
  front request's promise as the job promise of an `AsyncGen` reaction, and the await position of a parked generator.
- A `for await` or `await using` suspension reports the function start. Node reports the loop's binding (the
  declarator or left-hand side: `for await (const s of g())` is the column of `s`), which the AST does not carry.

## Verification

`scripts/v8-error-fixtures/async-1508.js` (combinators and method suffixes) and `timers-1508.js`, with Node 26.8's
output pinned in `expected-async-1508.txt` and `expected-timers-1508.txt` (regenerate with
`node scripts/v8-error-fixtures/<name>.js > .../expected-<name>.txt`). Unlike `async.js` they drop path-less lines
themselves, since the combinator frames have no file. `tests/node_stack_frames_1508.rs` replays both under the VM,
the tree-walker and `JSSE_GC_STRESS=7`, checks the `CallSite` API of a combinator frame, and checks default mode
stays stackless while the combinators still work. No `test262-extra/` test: nothing here is observable through
ECMAScript, only through the host's `Error.stack`, which `tests/` covers.
