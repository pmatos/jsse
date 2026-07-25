// Worker invoked as its own fresh OS process by gen-esprima-test262-corpus.js,
// once for the strip-safety check and once for the real recording pass. This
// keeps the two concerns from sharing a process:
//
//   - one call checks whether stripping each file's leading boilerplate is
//     parse-neutral (needs its own extra probe calls that must never affect
//     the recording pass);
//   - a SEPARATE call does the real recording, replaying the exact
//     (module, string) sequence in the exact order esprima-jsse-entry.js's
//     test262 runner will replay at run time, with nothing else interleaved —
//     so generation and runtime are structurally the same parse sequence.
//
// Usage: node gen-esprima-test262-corpus-worker.js <esprimaPath> <inputJsonPath> <outputJsonPath>
//   input: [{"module": bool, "strings": [string, ...]}, ...]  (one entry per
//          scenario call to make, "strings" is 1 item normally, or 2 for a
//          before/after strip-safety check)
//   output: [["pass"|"fail", ...], ...]  (same shape as "strings" per entry)

var fs = require("fs");

var esprimaPath = process.argv[2];
var inputPath = process.argv[3];
var outputPath = process.argv[4];

var esprima = require(esprimaPath);
var items = JSON.parse(fs.readFileSync(inputPath, "utf-8"));

var results = items.map(function (item) {
  var parseFn = item.module ? esprima.parseModule : esprima.parseScript;
  return item.strings.map(function (src) {
    try {
      parseFn(src);
      return "pass";
    } catch (e) {
      return "fail";
    }
  });
});

fs.writeFileSync(outputPath, JSON.stringify(results));
