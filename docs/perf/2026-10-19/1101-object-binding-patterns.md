# #1101: native plain-name `let`/`const` object binding patterns

`const { a, b } = this;` was a tree-walker hand-off (`Unsupported("lexical binding pattern")`) that bound by name. A pattern
whose properties are all `name` or `key: name` (identifier, string or number key, no default, rest, computed key or nested
pattern) now lowers to existing ops only:

```
init; (DupN 1; GetProp key; InitLexical name)* ; GetProp key; InitLexical name
```

The last property consumes the source instead of duplicating it. There is no new opcode and no change under `vm/`: the VM
dispatch loop is byte-for-byte the code of the base, only the compiler changed (`compiler/lexical.rs`, which also now holds
`compile_lexical_declaration`).

A `null`/`undefined` source fails at the first read (`Cannot read properties of undefined (reading 'a')`) instead of in a
separate RequireObjectCoercible: both are a TypeError thrown before any observable step, so a guard op would only cost
dispatch. ResolveBinding of a `let`/`const` name is unobservable, so the spec's resolve-then-GetV order needs nothing either.
The empty pattern `const {} = x` (nothing to read, so nothing to fail) and every pattern with a default, rest element, computed
key, nested pattern or non-UTF-16 string key stay on the tree-walker.

## Measured (buildbox3, shipped profile: fat LTO, cgu=1, nightly, `--locked`)

Base = `origin/main` at `f2f57af0`, new = base plus this change. cachegrind `--cache-sim=no`, whole-process `I refs`. Ir per
call = (Ir of 20,000 calls minus Ir of 2,000 calls) / 18,000, a method called in a loop, `this` = `{a:1,b:2,c:3,d:4}`.
`dN` is the explicit reads `const a = this.a, ...` (N names), `oN` the pattern, `o4r` the renamed `{a: w, b: x, c: y, d: z}`.

| probe | base | new | new over `dN` |
|---|---:|---:|---:|
| `empty` method | 2,042 | 2,043 | |
| `d1` / `o1` | 2,595 / 3,257 | 2,595 / 2,595 | +0.0% |
| `d2` / `o2` | 3,088 / 3,846 | 3,088 / 3,102 | +0.5% (criterion 2%) |
| `d4` / `o4` | 4,074 / 5,198 | 4,074 / 4,115 | +1.0% (criterion 2%) |
| `o4r` | 6,216 | 4,116 | |

Gap over the explicit reads, net of the empty method: `o2` +758 -> +14, `o4` +1,124 -> +41. `ast_fallback_execs` is 0 for every
plain-name form and one per execution for the forms that stay (`tests/perf_counters_object_pattern.rs`).

1M calls, pinned core, median of 11 interleaved runs: `o2` 0.79 s -> 0.58 s (-26.6%), `o4` 1.05 s -> 0.77 s (-26.7%) (load
average 2.2-2.9); the `empty`/`d2`/`d4` controls moved -2.7/-3.4/-1.3% in that run. Peak RSS of the probes 16.7-16.9 MB on both sides.

## No-regression gates

Whole-process Ir of the control set (one run each; Ir is deterministic to about +-60 per run):

| benchmark | base Ir | new Ir | delta |
|---|---:|---:|---:|
| bigint-noble-secp256k1 | 4,785,893,138 | 4,705,639,556 | -1.68% |
| opmix | 40,702,430,620 | 40,030,764,831 | -1.65% |
| Box2D | 5,409,747,529 | 5,405,554,578 | -0.08% |
| richards | 8,555,419,496 | 8,552,890,533 | -0.03% |
| delta-blue | 6,263,670,780 | 6,263,178,072 | -0.01% |
| lazy-collections | 3,663,140,730 | 3,662,922,719 | -0.01% |
| fib | 4,934,245,758 | 4,934,247,405 | +0.00% |
| gaussian-blur | 20,234,823,004 | 20,234,828,933 | +0.00% |
| splay | 7,593,582,516 | 7,599,581,915 | +0.08% |

`opmix` and `bigint-noble-secp256k1` contain plain-name patterns. Wall and peak RSS, pinned core, median of 15 interleaved runs,
rerun at load average 1.0-1.3 (an earlier pass at 2.2-2.9 gave the same picture), base -> new: fib 0.96 -> 0.97 s (+1.0%),
richards 1.97 -> 1.97 s (0.0%), delta-blue 1.44 -> 1.41 s (-2.1%), splay 2.10 -> 2.07 s (-1.4%), Box2D 1.26 -> 1.24 s (-1.6%),
lazy-collections 0.87 -> 0.86 s (-1.1%); from the earlier pass opmix 7.28 -> 7.06 s (-3.0%), bigint-noble-secp256k1 1.15 -> 1.14 s
(-0.9%). Peak RSS within +-1.3% everywhere (splay 582 MB on both sides). 1M-call probes at low load: `o2` 0.80 -> 0.58 s
(-27.5%), `o4` 1.04 -> 0.77 s (-26.0%).

## What the layout lottery looked like

The first version fused the duplicate and the read into one new op, `GetPropKeep` (`[..., base] -> [..., base, base.name]`, out of
line in a new `vm/` module, with a `RequireObjectCoercible` guard dropped as above). It was faster than the shipped form
(`o2` 3,003, `o4` 3,820 Ir, below `d2`/`d4`) and kept every control within +0.4% Ir (gaussian-blur +0.37%, opmix +0.22%,
richards +0.13%), but one extra arm in `run_chunk_inner` moved wall in a repeatable way on workloads that never reach it:
richards +2.0/+2.5%, Box2D +1.6/+1.6%, splay +1.9/+1.0% (two interleaved runs of 11 and 21). The shipped form needs no VM
change and meets the 2% criterion, so the fused op was dropped.

## Residual

- `var {a, b} = o` (ResolveBinding is observable there: `with`, the global object) and patterns in function parameters and
  `for (const {a, b} of xs)` heads (`BindHead`) are still tree-walker work.
- Defaults (`{a = 1}`), rest elements, computed keys and nested patterns are the remaining `lexical binding pattern` hand-offs.
- `const {} = x` stays on the tree-walker.

## Correctness gates

`test262-extra/bytecode-object-binding-pattern-simple.js` (checked against Node): key forms (shorthand, renamed, string,
numeric, array-index-like on Array/String/typed-array/`arguments`/Proxy sources), missing and inherited properties, getter
order and count (once each, in pattern order, a later name still in TDZ while an earlier getter runs, an earlier name already
bound), a throwing getter mid-pattern, Proxy traps (only `get`, with the proxy as receiver), primitive receivers (a strict getter
sees an unboxed `this`), `null`/`undefined` sources, the empty pattern, TDZ in the initializer, `const` assignment, block,
`switch`, loop and `try`/`finally` scopes, closures, several declarators. `test262-extra/bytecode-object-binding-pattern-gc-rooting.js`:
getters that allocate and collect while the source, earlier bindings and results are held only by the operand stack, a primitive
source's wrapper, a throw after a collection, nested patterns in getters; also run under `JSSE_GC_STRESS=1,3,7` and on the
`release-checked` build (debug root-stack asserts).

Full test262 (99,911 scenarios) passes with and without `--no-bytecode`; `test262-extra` passes normally, under
`JSSE_GC_STRESS=7` and with `--no-bytecode` (plain and stress); `cargo test --profile release-nolto`, lint and the custom tests
pass.
