# Node host mode: the `WebAssembly` global

Status: accepted, epic #1214, issue #1378 (extends `2026-10-27-1600-node-host-hereby-local.md`).

## Context

`hereby local` ended with `Error in dts-services ... WebAssembly is not defined`: the declaration bundles
(`typescript.d.ts`, `typescript.internal.d.ts`, `tsserverlibrary.d.ts`, `tsserverlibrary.internal.d.ts`) are
formatted by `scripts/dtsBundler.mjs` with `@dprint/formatter`, which instantiates the 4 MB
`@dprint/typescript` plugin (`plugin.wasm`, schema v3) and drives it through its linear memory. jsse had no
`WebAssembly`. A human ruling (2026-10-28) allows an existing pure-Rust WebAssembly interpreter as a utility
dependency (not a JS parser or engine crate), exposed under `--node` first.

## Decisions

1. **Engine: `wasmi` 2.0** (`default-features = false`, features `std`, `validate`, `portable-dispatch`).
   Candidates were `wasmi`, `tinywasm`, `wasmtime` (a JIT with native code, not a pure interpreter) and a
   from-scratch interpreter. `wasmi` is the mature one (Parity/Wasmi Labs, register-machine IR, lazy
   function translation, the full MVP plus bulk memory, reference types, multi-value, sign extension and
   saturating conversions, which a current Rust `wasm32-unknown-unknown` target emits). Licences: `wasmi*` are
   `MIT/Apache-2.0`; the rest of the new graph is `MIT OR Apache-2.0`, `Apache-2.0 WITH LLVM-exception OR
   Apache-2.0 OR MIT` (`wasmparser`), `MIT` (`spin`, `libm`), `Zlib` (`foldhash`), `Unlicense OR MIT`: every
   expression has an alternative in `deny.toml`'s allow list, which is unchanged. The `wat` text parser (and with it
   `wast`, `wat`, `wasm-encoder`, `bumpalo`) and SIMD are off. Measured on the dprint plugin with a
   standalone harness: 23 ms to compile the module, 0.5 ms to instantiate, 2.1 s to format the 588 KB
   `typescript.d.ts` stripped of its indentation (Node: 0.25 s).
2. **`portable-dispatch`.** wasmi's default dispatch chains one handler call per instruction and relies on LLVM
   turning each into a sibling call. That holds in some builds and not in others (it overflowed the 128 MiB
   engine stack after a few million instructions with `lto = "off"`, which is what the quick-iteration profile and
   possibly `release-nolto`/`release-checked` produce), and it is invisible until a long-running wasm program
   runs. The loop dispatch needs no optimiser help; measured cost on the same input is within noise (2.1 s vs 1.4 to
   2.6 s).
3. **`--node` only.** The default environment has no `WebAssembly`; nothing is installed unless the Node
   prelude runs, so test262 is unaffected by construction (the prelude is never parsed without `--node`). The
   binding is the hidden `__host_wasm` global, captured and deleted by `scripts/node-wasm-shim.js`, the same
   shape as `__host_crypto`.
4. **The JS API is JavaScript over an integer-handle binding.** The shim
   (`scripts/node-wasm-shim.js`) defines `Module`, `Instance`, `Memory`, `Table`, `Global`, `CompileError`,
   `LinkError`, `RuntimeError`, `validate`, `compile`, `instantiate` and does every value coercion
   (`ToInt32`, `ToBigInt64`, `ToNumber`, `fround`) because it may run user code; the Rust binding
   (`node_host_wasm*.rs`) only sees plain numbers, BigInts and references. Property attributes, messages and
   argument checks follow V8 (cross-checked against Node 26 by `scripts/run-node-wasm-oracle.sh`); Compile/Link
   error text after the `WebAssembly.X(): ` prefix is wasmi's.
5. **`Memory.buffer` aliases the linear memory.** The dprint protocol writes into `memory.buffer` from JS and
   reads wasm's writes back, thousands of times for a 600 KB file against a 48 MB memory, so copying at the
   boundary is not an option. `BufferData::Foreign(ForeignBytes)` is a `(pointer, length)` window onto the
   wasmi memory. The window is valid because every path from wasm back to JS (an export returning, an import
   being entered, `instantiate`) runs `refresh_buffers`, which detaches (flag, empty `Owned` data, also swapped
   into the old `Rc` the views hold) any handed-out buffer whose memory's pointer or size changed, and
   `Memory.prototype.grow` always detaches, as the JS API specifies. The buffer is the same object until then.
   `transfer`, `transferToFixedLengthBuffer`, `$262.detachArrayBuffer` and a transferring `structuredClone`
   reject it (`TypeError`/`DataCloneError`), and it is never counted as external memory.
6. **Re-entrancy through the `Caller`.** The store lives in `WasmState` between calls and is lent to wasmi
   during one. An import trampoline records its `Caller` in `WasmState::caller` for the duration of the JS
   callback, and `with_ctx` uses it, so a callback that calls an export (or touches a memory) uses the store that
   is already borrowed. The `Interpreter` pointer in the store data is set by each outermost call and only
   dereferenced inside it. Nesting is capped (64 levels, then `RangeError`).
7. **GC.** Import callables, externref payloads, the buffers handed out and a pending throw sit in one
   `RootedSlots` array pinned on the binding object (never shrunk: externref payloads and per-instantiate import
   callables live as long as the process). wasmi handles have no equality, so identity (an exported function
   found again in a table is the same JS function) is interned by their `Debug` text.
8. **Errors.** A JS exception, `process.exit` or abrupt completion inside an import is parked and re-thrown
   unchanged after wasm unwinds. Traps become `RuntimeError` with V8's messages, stack exhaustion a
   `RangeError`, a failed link or start-function trap the corresponding error.

## Not provided

`Tag`, `Exception`, `JSTag` (exception handling), shared memories, `compileStreaming`/`instantiateStreaming`
(no `Response`; `@dprint/formatter` falls back to `createFromBuffer`), `Memory.prototype.toFixedLengthBuffer`,
`toResizableBuffer`, v128 values (SIMD is off), `WebAssembly.Function`/JSPI, and a function's `toString`
text. Exported functions are named by export index only for the first wrapper made of a function (a function
first met through a table is named `0`).

## Verification

`scripts/run-node-wasm-oracle.sh` (fixtures `scripts/wasm-fixtures/{api,run}.js`, a JS module encoder, stdout equal
to Node's), `tests/node_wasm.rs` (known answers, plain, `--no-bytecode` and `JSSE_GC_STRESS=3`), and the
`hereby local` oracle, which no longer tolerates the missing bundles
(`scripts/run-hereby-local-oracle.py`). Numbers are in the PR.
