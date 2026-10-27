# Vendored web-platform-tests URL data

Real WPT `url/resources` data, taken from the Node.js v26.8.2 repository
(`test/fixtures/wpt/url/resources/`, WPT commit
`4832db47614f5f48cc57374cbf5c1f70937fad48` per Node's
`test/fixtures/wpt/versions.json`), source prefix
`https://raw.githubusercontent.com/nodejs/node/v26.8.2/test/fixtures/wpt/url/resources/`.
Licence: BSD-3-Clause, web-platform-tests contributors
(https://github.com/web-platform-tests/wpt/blob/master/LICENSE.md).

| file | bytes | sha256 |
| --- | --- | --- |
| `urltestdata.json` | 228373 | `355c9f1e5f34aae66ba8adfabf3c853f5cd30ea22964ef7a53eb292e7975d81e` |
| `urltestdata-javascript-only.json` | 902 | see `manifest.json` |
| `setters_tests.json` | 82134 | see `manifest.json` |
| `toascii.json` | 9101 | see `manifest.json` |
| `IdnaTestV2.json` | 313831 | see `manifest.json` |

The repo caps authored files at 20,000 bytes, so each file is stored as
numbered byte slices cut at line boundaries (`<name>.partNN`); concatenating
them in order restores the original bytes. `manifest.json` pins the sha256 of
every part and of the whole file, and `run.js` refuses to run on a mismatch.

`run.js` replays the data the way the WPT harness files do
(`url-constructor.any.js`, `url-origin`, `url-setters.any.js`,
`toascii.window.js` for `toascii.json` and `IdnaTestV2.json`; plus `URL.canParse`, `URL.parse` and a `URL` base) and
prints the failing vectors, a per-suite count and a digest line. Node and
`jsse --node` must print identical output; `expected.output` is that output.
Run it with `./scripts/run-node-url-wpt.sh`. To refresh the data, download the
five files from the prefix above, split them at line boundaries at most 19,000
bytes, and regenerate `manifest.json` and `expected.output`.

`IdnaTestV2-removed.json` and the WPT `*.window.js` / `a-element` harness files
are not vendored: the first only lists tests dropped from `IdnaTestV2.json`,
the others need a DOM. `toascii.window.js` was read at the pinned WPT commit;
the `<a>`/`<area>` variants it runs are DOM-only and skipped here.
