JetStream `web-ssr` replaces `globalThis.console` to suppress logging. JSSE keeps resolving bare `console` to its original host object, so the benchmark still prints `renderToString took ...ms`. Its HTML validation passes, but the host side effect differs from Boa and Node.

## Reproduced revisions

- JSSE: `2ea6417535037ca58d78a433a72a2a0e2dfb4d77`, default fat-LTO release build.
- Boa HEAD: `f0789a20d87ad52eef6460953929c2b3cdbf8716`, default release CLI.
- Node: v26.8.2.
- Same result with JSSE bytecode enabled and `--no-bytecode`; fresh process for each run.

## Minimal reproducer

```js
if (console !== globalThis.console)
  throw new Error("console is not the globalThis.console property");
```

Node and Boa exit 0. Both JSSE execution modes exit 1 with `Error: console is not the globalThis.console property`.

The observable replacement failure can also be reproduced with:

```js
globalThis.console = { log() {} };
console.log("this should be suppressed");
```

The audit fixture also checks that `console === replacement` after assigning a replacement through `globalThis.console`.

## Root cause

[`setup_console`](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/builtins/host.rs#L166) declares `console` as a Const binding in the bootstrap global environment. [`BUILTIN_GLOBAL_NAMES`](https://github.com/pmatos/jsse/blob/2ea6417535037ca58d78a433a72a2a0e2dfb4d77/src/interpreter/builtins/global_object.rs#L4) omits it. Global-object population and removal of bootstrap bindings therefore never move `console` into the Object Environment Record. Assigning `globalThis.console` creates an unrelated property, while identifier resolution continues to see the original lexical binding.

## Expected host behavior and scope

Expose the provided console through a replaceable global-object property, and have bare-name lookup observe that property. The [Console Standard](https://console.spec.whatwg.org/#namespace-console) declares an exposed console namespace; [Web IDL namespace bindings](https://webidl.spec.whatwg.org/#namespaces) expose namespace objects as properties of the realm's global object. This is a host compatibility bug; it should have a focused fixture under `tests/` because console is outside test262's ECMAScript core surface.

## Suggested validation

- Initial identity: `console === globalThis.console`.
- Replacement via `globalThis.console` and via bare `console`; subsequent reads and calls observe the replacement.
- Property deletion/redefinition and descriptors follow the selected host-global policy.
- Run the fixture in both execution modes and rerun JetStream web-ssr; its console stub should suppress the diagnostic line while HTML validation remains green.

No engine implementation was changed during this audit. Raw differential evidence and the full benchmark run are retained locally under `docs/perf/2026-10-14/boa-head-audit/`.
