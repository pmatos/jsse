# Node host mode: tsc emit option matrix

Status: accepted, epic #1214, issue #1245 (builds on the project oracle of
`2026-10-27-0900-node-host-tsc-projects.md`).

## Decision

1. **A generator, not a hand-written list.** `scripts/gen-tsc-emit-cases.py` holds the
   matrix as a compact spec and writes `scripts/tsc-fixtures/cases-emit-main.txt`,
   `cases-emit-options.txt` and `cases-emit-lang.txt` (each pinned in its own
   `expected-emit-*.txt`; the harness already loads every `cases-<topic>.txt`). `--check`
   fails when a generated file is stale. The files are split so that each stays under the
   per-file token budget.
2. **One tiny source set.** Every case compiles files of `projects/emit`: `types.ts`,
   `lib.ts` (enum, `const enum`, namespace, class with a static block and a parameter
   property, `@internal`, a default export), `main.ts` (re-exports, `import type`,
   dynamic `import()`, `for await`, async generator, spread of a generator, object rest,
   `?.`/`??`/`**`), `priv.ts` (`#private` fields, methods and `#x in o`), `deco.ts` and
   `esdeco.ts` (ES decorators, `accessor`, a decorated class expression), `cexpr.ts` (decorated class expressions only), `pdeco.ts` (legacy
   parameter decorators and metadata), `ext.ts` (`.ts` import specifiers and a dynamic `import()` of one), `synth.ts` with `cjsdep.d.ts` (a default import of an `export =` module), `view.tsx`, `esm.mts`, `cjs.cts`, `pure.mts`,
   `esm/` (a nested `package.json` with `"type": "module"`, top-level `await`) and
   `js/` (CommonJS and ESM sources with JSDoc, one with a `checkJs` error).
   `globals.d.ts` declares `JSX`, `React`, the `react`/`preact` runtime modules and
   `ImportMeta`; `tslib.d.ts` and `tsconfig.helpers*.json` map `tslib` through `paths` so
   `--importHelpers` has a module to import without vendoring anything. All cases use
   `--lib es2020`: a `--noLib` compile cannot type-check async generators.
3. **What is in the matrix.**
   - `main`: target (es5 to esnext) x module (commonjs, esnext, amd, umd, system,
     node16), seven more modules at es2022, nine module x moduleResolution pairs and five
     targets over `priv.ts`; each with `--declaration --sourceMap`, so the `.js`, `.d.ts`
     and `.js.map` bytes all count.
   - `options`: `--sourceMap`, `--inlineSourceMap`, `--inlineSources`, `--sourceRoot`,
     `--mapRoot`, `--declarationMap`, `--declarationDir`, `--emitDeclarationOnly`,
     `--stripInternal`, `--removeComments`, `--newLine`, `--emitBOM`, `--noEmitHelpers`,
     `--importHelpers` (also with legacy and ES decorators), `--preserveConstEnums`,
     `--isolatedModules`, `--verbatimModuleSyntax` (clean and with the diagnostics),
     `--esModuleInterop`, `--allowSyntheticDefaultImports` (on, off with its diagnostic, and via `--esModuleInterop`), `--useDefineForClassFields`,
     `--noCheck`, `--erasableSyntaxOnly`, `--composite`, `--incremental`, `--rootDir`,
     `--listEmittedFiles`, `--rewriteRelativeImportExtensions` (over `.ts` specifiers, under esnext, commonjs, amd and nodenext), and `--outFile` with
     amd and system at es5 and es2020 (with declarations, maps, comments removed).
   - `lang`: the five `--jsx` modes under commonjs and esnext, `jsxFactory`,
     `jsxFragmentFactory`, `jsxImportSource`; legacy decorators (es5 to esnext, with
     `useDefineForClassFields`, with `emitDecoratorMetadata`), ES decorators; `.mts`,
     `.cts`, mixed projects and a `"type": "module"` package under node16, nodenext,
     preserve and esnext; `allowJs` and `checkJs` over targets and modules, with
     declarations, maps, comments removed, mixed with TypeScript, with `--outFile` (JS only and mixed with TypeScript), and two
     failing `checkJs` runs (exit 2, and exit 1 with `--noEmitOnError`).
4. **Result.** All 193 cases (57 + 65 + 71) emit byte-identical `.js`, `.d.ts`, `.map`,
   `.mjs`/`.cjs`/`.jsx` and `.tsbuildinfo` files, stdout and exit codes on Node v26.8.2 and jsse
   with TypeScript 6.0.3: no engine divergence surfaced, so there is no engine change. Seven
   cases deliberately end in diagnostics (`verbatimModuleSyntax` in a CommonJS file, `checkJs`
   errors, `--erasableSyntaxOnly`, a default import without `allowSyntheticDefaultImports`; exit 2, or 1 with `--noEmitOnError`), so the emit-despite-errors path is covered too. Wall time
   on a host at load 70 with a non-LTO release build: Node 0.3 to 0.7 s per case, jsse 3 to 10 s;
   the full matrix takes about ten minutes with `--jobs 8` (the Rust guard runs it with 4 jobs).
   The pins were taken from Node, so `--no-node` compares jsse alone.

## Guards

`scripts/test_gen_tsc_emit_cases.py` (the `unittest discover` step) checks that the generated
files are current, that the matrix has 100 or more distinct, well-formed cases whose
input files exist, and that each pin file lists exactly its cases. `tests/node_tsc_projects.rs`
already runs every cases file against the pins when `TSC_DIR` is set.

## Running it

    TSC_DIR=/path/to/typescript-6.0.3 uv run python scripts/run-tsc-projects-oracle.py --jobs 8 $(python3 scripts/gen-tsc-emit-cases.py --list)

After changing the spec, run the generator, then the same command with `--pin`.
