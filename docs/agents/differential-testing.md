# Differential testing

`scripts/run-differential.py` runs every file in `tests/differential/`
through jsse and node and compares what they print and how they finish. Boa
joins as a second opinion when a `boa` binary is on `PATH` or passed with
`--boa`. It complements test262: test262 asserts what the spec requires on
narrow cases, while the differential corpus runs ordinary programs end to end
and catches output that is plausible but wrong (number formatting, ordering,
rounding) where no test262 assertion happens to look.

The `differential` fuzz target (`fuzz/`) is related but different: it feeds
random programs to both engines and only flags crashes and parse
accept/reject mismatches, because random programs make output comparison too
noisy. This harness compares output, on a curated corpus.

## Running it

```bash
cargo build --release
uv run python scripts/run-differential.py                 # whole corpus
uv run python scripts/run-differential.py tests/differential/json.js
uv run python scripts/run-differential.py --boa /path/to/boa --json out.json
```

Exit status is 0 when every file matches or is approved, 1 on an unapproved
divergence, 2 on a setup error. `--observe` always exits 0. The `verify`
skill and `scripts/verify.sh` run it, and CI runs it observe-only in the
`build` job.

## How a file is run

Each file is prefixed with `scripts/differential-prelude.js` and run as a
classic script. Node runs it through `scripts/differential-node-runner.js`
(`vm.runInThisContext`), so top-level `this` and `var` behave as in a script,
not a CommonJS module. The compared result is stdout, with line endings and
trailing whitespace normalized, plus an outcome: `ok`, `throw:<ErrorClass>`,
`crash`, `oom` or `timeout`. Error messages are not compared, because they are
engine-specific.

## Writing corpus files

- Print with `show(...)` from the prelude, never with `console.log` of an
  object: host `inspect` formatting differs between engines and is not what
  we are testing. `show` prints strings bare at top level, `-0` as `-0`,
  BigInts with `n`, holes as `<hole>`, and objects by own enumerable keys.
- Stick to ECMAScript plus `setTimeout` and `console.log`. Host globals that
  jsse does not provide outside `--node` mode (`queueMicrotask`, `process`,
  `require`) make the file diverge for the wrong reason.
- Avoid anything nondeterministic or host-dependent: local time zones,
  `Math.random`, timing, `Intl` output that depends on ICU data versions,
  and error message text.
- Keep each file to one area and small enough that a divergence is easy to
  read. Name it after the area (`json.js`, `async-ordering.js`).
- When you fix a bug that test262 missed, add the program that shows it.

## Approved differences

`tests/differential/approved.toml` lists the divergences that were reviewed
and accepted, one entry per file with a reason. Each entry carries the
fingerprint the runner prints for an unapproved divergence, a hash of both
engines' results. If the divergence changes shape, the file is reported as
approved-but-changed and has to be looked at again; if it disappears, the
entry is reported as stale and should be deleted. Approve a difference only
when jsse is right (cite the spec) or the difference is host behaviour;
a jsse bug gets an issue or a fix, not an approval.
