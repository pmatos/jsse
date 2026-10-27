# Node host mode: tsc diagnostics, colour and terminal behaviour

Status: accepted, epic #1214, issue #1244 (builds on the project oracle of
`2026-10-27-0900-node-host-tsc-projects.md`).

## Decision

1. **Cases in their own file.** `scripts/tsc-fixtures/cases-diagnostics.txt`, pinned in
   `expected-diagnostics.txt`, over one new project tree `projects/diag`. The harness now
   loads `cases.txt` and every `cases-<topic>.txt` (pins of the latter in
   `expected-<topic>.txt`), so a ticket adds cases without touching the shared files;
   `--pin` rewrites each pin file from its own cases.
2. **Two harness additions.** A leading `NAME=value` token of an invocation sets that
   environment variable for it (`NO_COLOR`, `FORCE_COLOR`; the harness still removes
   both, `COLUMNS` and `NODE_OPTIONS` from the inherited environment). A leading
   `@pty:ROWSxCOLS` runs the invocation with stdin, stdout and stderr on one
   pseudo-terminal of that size through `scripts/tty-fixtures/pty-run.py`, the runner of
   the tty oracle: it waits for the child to exit and the terminal to hang up, with no
   sleeps. A terminal merges stderr into stdout; tsc writes everything to stdout anyway.
   A source file `x.ts.esc` in a project is written as `x.ts` after decoding `\r`,
   `\n`, `\uXXXX` escapes (UTF-8, or UTF-16 for `.utf16le.esc` and `.utf16be.esc`), because the repository's hooks (mixed line endings,
   final newline) would rewrite CRLF, BOM, lone CR and U+2028 sources.
3. **What `tsc` decides from the terminal.** `defaultIsPretty` is `NO_COLOR` set (any
   non-empty value) then plain, else `FORCE_COLOR` set (any non-empty value, `0`
   included) then pretty, else `process.stdout.isTTY`; `--pretty true|false` overrides all
   three. `process.stdout.columns` is read only by `--help` and the banner: the TS logo
   appears when the width is at least the banner length plus 5 (49) and `--help --all`
   switches to the wrapped two-column layout at 80; the banner is capped at 120. The
   other half of the ticket, `sys.setBlocking` and `process.exit` after a large write,
   is covered by 245 KB of pretty output through a pipe and through a terminal.
4. **Cases (113).**
   - 33 error fixtures `diag-eNN-*`, each compiled `--pretty false` then `--pretty`
     with `--noLib` and a hand-written minimal `globals.d.ts`: mismatches, syntax errors,
     unterminated literals, multi-line spans (the `...` elision of a code frame), tabs,
     a 300-column line, CJK and emoji, CRLF, lone CR, U+2028 and U+2029, a BOM, UTF-16
     LE and BE sources, invalid UTF-8, no final newline, three-digit gutters, related
     information, message chains, `@ts-expect-error`, unresolved modules, a union too
     long for the 160-character message limit, `.js` with `--checkJs`, `.d.ts`, `.tsx`,
     invalid characters, a nested path, a file name with spaces and accents, `--strict`
     diagnostics, class and generic errors.
   - Auto mode: pipe, terminal, terminal 200 wide, `NO_COLOR`, `FORCE_COLOR`,
     both, `NO_COLOR=` (empty), `FORCE_COLOR=0`, and `--pretty` against each.
   - Summary lines (one error, several in a file, a file table, none), exit codes 0, 1
     (`--noEmitOnError`, bad option, bad option value, no input) and 2 (also a missing input file).
   - `--help`, `--help --all`, no arguments, `--version`, `--build --help` on terminals
     of 0 to 1000 columns (including 48/49, 79/80, 120/121) and on a pipe.
   - `--noErrorTruncation` with plain and pretty output.
   - `--locale` for the 13 locale directories the package ships (`cs de es fr it ja ko pl
     pt-br ru tr zh-cn zh-tw`): semantic errors plain and pretty, syntax errors, and
     `--help`; mixed-case spellings, `en`, unsupported but well-formed locales (`xx`,
     `de-DE`, `pt`), malformed ones (`fr-`, empty) and a missing value.
   - tsconfig diagnostics with code frames in a JSON file, and `many.ts`, 70 errors on
     one 2.7 KB line (245 KB pretty), plain and pretty on a pipe and pretty on a terminal.
5. **Result.** All cases are byte-identical on Node v26.8.2 and jsse with TypeScript
   6.0.3: no engine divergence surfaced, so there is no engine change in this PR. Manual
   probes outside the harness agree too: stdout redirected to a file, to `/dev/null`,
   closed (`>&-`), a reader that sleeps 1.5 s before reading 245 KB, and `| head -c`.
   Wall time with a non-LTO release build: Node 0.3 to 0.7 s per case, jsse 1 to 4 s;
   the whole file takes about 40 s with `--jobs 8`.
6. **Guards.** `tests/node_tsc_projects.rs` runs every cases file against the pins
   (skipped without `TSC_DIR`, like the rest); `scripts/test_run_tsc_cases_files.py`
   (in the `unittest discover` step) drives the harness with fake executables to prove
   the second cases file and its pin file, the environment and terminal directives and
   the `.esc` decoding, and checks that the diagnostics file is well formed, pinned,
   holds 30 or more error fixtures compiled both ways and covers every locale.

## Running it

    TSC_DIR=/path/to/typescript-6.0.3 uv run python scripts/run-tsc-projects-oracle.py --jobs 8 $(sed -n 's/^\(diag-[^ ]*\) .*/\1/p' scripts/tsc-fixtures/cases-diagnostics.txt)
