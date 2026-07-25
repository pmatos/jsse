// Run once from esprima.sh's lib_prepare, cwd'd into the cloned esprima repo,
// AFTER dist/esprima.js has been compiled and test262-stream +
// results-interpreter have been installed (see lib_prepare).
//
// Generates the test262 grammar-conformance corpus the issue #295 comment
// specifies: esprima's own `test-262` npm script drives test262-stream +
// results-interpreter over a live clone of test262 pinned at the short SHA
// `36d2d2d` (full: 36d2d2d348d83e9d6554af59a672fbcd9413914b) and a whitelist
// of known esprima-vs-spec divergences. That live pipeline needs `fs`,
// `stream`, and a real test262 checkout at RUNTIME — none of which exist in a
// jsse bundle — so this script instead runs the equivalent traversal ONCE,
// here, in Node, and freezes the result into one bundlable data file:
//
//   - test262-stream (with `omitRuntime: true` — we only ever *parse*, never
//     *evaluate*, so harness "includes" are pure noise) enumerates every
//     scenario (default / strict mode) test262 actually defines, respecting
//     each file's onlyStrict/noStrict/raw/module flags — that enumeration
//     logic is exactly the kind of thing not worth hand-rolling.
//   - Per FILE (not per scenario — this is what keeps the payload near the
//     ~26 MiB the issue targets instead of ~50 MiB), we store the raw
//     include-free source ONCE. create-scenarios.js (test262-stream's own
//     source) proves the "strict mode" scenario is always exactly
//     '"use strict";\n' + <the file's raw source> — verified byte-for-byte
//     against a handful of real files before relying on it — so the runtime
//     harness reconstructs it by prepending that literal instead of storing a
//     second copy.
//   - For each scenario that exists, `expected` is recorded by running LOCAL
//     Node against the exact string (raw source, or the reconstructed strict
//     variant) esprima-jsse-entry.js's test262 runner will feed the SAME
//     compiled dist/esprima.js at runtime — not test262-stream's
//     harness-bloated `contents`, and not the spec-ideal outcome. This means
//     a jsse run can only diverge from what's recorded by actually behaving
//     differently from Node on the identical esprima code — esprima's own
//     pre-existing spec limitations (tracked in test/test-262-whitelist.txt)
//     don't masquerade as new jsse bugs.
//   - The whitelist + spec-ideal `negative` expectation are cross-referenced
//     only as a generation-time sanity log (divergences from spec that
//     AREN'T whitelisted get printed) — informational, never blocking.
//
// Output: test/jsse-test262-corpus.js containing
//   module.exports = "<JSON-encoded corpus, itself JSON.stringify'd into a JS
//   string literal>";
// A JSON *string* rather than a live object/array literal so jsse's own
// tokenizer only has to scan one big string token when it loads the bundle,
// not build an AST for tens of thousands of nested object/array literals.
// esprima-jsse-entry.js does `JSON.parse(require('./jsse-test262-corpus'))`.

var fs = require("fs");
var path = require("path");
var childProcess = require("child_process");

var TEST262_SHA = "36d2d2d348d83e9d6554af59a672fbcd9413914b";
var TEST262_REPO = "https://github.com/tc39/test262.git";

var repoRoot = process.cwd();
var scratchDir = path.join(repoRoot, ".test262-corpus-src");

function run(cmd, args, opts) {
  var res = childProcess.spawnSync(cmd, args, Object.assign({ stdio: "inherit" }, opts));
  if (res.status !== 0) {
    throw new Error("command failed: " + cmd + " " + args.join(" "));
  }
}

if (!fs.existsSync(path.join(scratchDir, ".git"))) {
  console.log("Cloning test262 @ " + TEST262_SHA + " ...");
  run("git", ["clone", "--filter=blob:none", TEST262_REPO, scratchDir]);
  run("git", ["checkout", "-q", TEST262_SHA], { cwd: scratchDir });
} else {
  console.log("Using cached test262 checkout at", scratchDir);
}

// This script itself lives under jsse's scripts/, but it always runs cwd'd
// into the cloned esprima repo (see esprima.sh's lib_prepare) — resolve both
// esprima's own build output and test262-stream against that cwd's
// node_modules, not against this script's own location.
var esprima = require(path.join(repoRoot, "dist", "esprima.js"));
var TestStream = require(require.resolve("test262-stream", { paths: [repoRoot] }));

var STRICT_PREFIX = '"use strict";\n';

// path -> { module, source, scenarios: [{type}], negativeByType: {type: bool} }
var files = Object.create(null);

var whitelist = new Set();
var whitelistPath = path.join(repoRoot, "test", "test-262-whitelist.txt");
fs.readFileSync(whitelistPath, "utf-8")
  .split("\n")
  .map(function (l) { return l.trim(); })
  .filter(Boolean)
  .forEach(function (l) { whitelist.add(l); });

function specExpected(attrs) {
  var neg = attrs.negative;
  return neg && (neg.phase === "early" || neg.phase === "parse") ? "fail" : "pass";
}

console.log("Enumerating test262 scenarios (this walks the whole test/ tree)...");

var stream = new TestStream(scratchDir, { omitRuntime: true });
var count = 0;

stream.on("data", function (test) {
  count++;
  var entry = files[test.file];
  if (!entry) {
    entry = files[test.file] = {
      module: !!test.attrs.flags.module,
      source: null,
      scenarios: [],
      specExpectedByType: {},
    };
  }
  var raw =
    test.scenario === "strict mode" && test.contents.indexOf(STRICT_PREFIX) === 0
      ? test.contents.slice(STRICT_PREFIX.length)
      : test.contents;
  if (entry.source === null) {
    entry.source = raw;
  } else if (entry.source !== raw) {
    console.warn("WARNING: scenario content mismatch for", test.file, test.scenario);
  }
  entry.scenarios.push(test.scenario);
  entry.specExpectedByType[test.scenario] = specExpected(test.attrs);
});

stream.on("error", function (err) {
  console.error("test262-stream error:", err);
  process.exitCode = 1;
});

// Strips leading //-comments, /*...*/ blocks (including the YAML frontmatter
// block every test262 file has), and blank space from the START of a source
// string only. Every test262 file leads with a boilerplate copyright comment
// plus its frontmatter, none of which a parser needs — this is what closes
// most of the gap to the issue comment's ~26 MiB target. Never touches
// anything once real code starts, so it can't touch a mid-file directive
// prologue, a string literal, or a regex literal.
function stripLeadingBoilerplate(src) {
  var i = 0;
  var n = src.length;
  while (i < n) {
    var c = src[i];
    if (c === "\n" || c === "\r" || c === " " || c === "\t") {
      i++;
      continue;
    }
    if (c === "/" && src[i + 1] === "/") {
      var nl = src.indexOf("\n", i);
      i = nl === -1 ? n : nl + 1;
      continue;
    }
    if (c === "/" && src[i + 1] === "*") {
      var end = src.indexOf("*/", i + 2);
      if (end === -1) break;
      i = end + 2;
      continue;
    }
    break;
  }
  return src.slice(i);
}

// Both the strip-safety check and the real recording pass run via
// gen-esprima-test262-corpus-worker.js, invoked once each as a fresh child
// process, so the two never share process state. That worker's recording
// invocation replays the exact (module, string) sequence in the exact order
// esprima-jsse-entry.js's test262 runner will replay at run time, with
// nothing else interleaved, so generation and runtime are structurally the
// same parse sequence and agree.
var esprimaModulePath = path.join(repoRoot, "dist", "esprima.js");
var workerPath = path.join(__dirname, "gen-esprima-test262-corpus-worker.js");
var scratchTmp = path.join(repoRoot, ".test262-corpus-tmp");
fs.mkdirSync(scratchTmp, { recursive: true });

function runWorker(items) {
  var inputPath = path.join(scratchTmp, "worker-in.json");
  var outputPath = path.join(scratchTmp, "worker-out.json");
  fs.writeFileSync(inputPath, JSON.stringify(items));
  run("node", [workerPath, esprimaModulePath, inputPath, outputPath]);
  return JSON.parse(fs.readFileSync(outputPath, "utf-8"));
}

stream.on("end", function () {
  console.log("Enumerated", count, "scenarios across", Object.keys(files).length, "files.");
  var fileOrder = Object.keys(files);

  console.log("Deciding which files' leading boilerplate is safe to strip (isolated child process)...");
  var checkItems = [];
  var checkableFiles = [];
  fileOrder.forEach(function (filePath) {
    var entry = files[filePath];
    var stripped = stripLeadingBoilerplate(entry.source);
    if (stripped === entry.source) return;
    checkableFiles.push(filePath);
    checkItems.push({
      module: entry.module,
      strings: entry.scenarios.map(function (type) {
        return type === "strict mode" ? STRICT_PREFIX + entry.source : entry.source;
      }),
    });
    checkItems.push({
      module: entry.module,
      strings: entry.scenarios.map(function (type) {
        return type === "strict mode" ? STRICT_PREFIX + stripped : stripped;
      }),
    });
  });
  var checkResults = runWorker(checkItems);
  var strippedFiles = 0;
  checkableFiles.forEach(function (filePath, i) {
    var beforeResult = checkResults[i * 2];
    var afterResult = checkResults[i * 2 + 1];
    var safe = beforeResult.every(function (v, j) { return v === afterResult[j]; });
    if (safe) {
      files[filePath].source = stripLeadingBoilerplate(files[filePath].source);
      strippedFiles++;
    }
  });
  console.log("Stripped leading boilerplate from " + strippedFiles + "/" + fileOrder.length + " files.");

  console.log("Recording Node+esprima's actual parse outcome per scenario (isolated child process, single pass)...");
  var recordItems = fileOrder.map(function (filePath) {
    var entry = files[filePath];
    return {
      module: entry.module,
      strings: entry.scenarios.map(function (type) {
        return type === "strict mode" ? STRICT_PREFIX + entry.source : entry.source;
      }),
    };
  });
  var recordResults = runWorker(recordItems);

  var totalScenarios = 0;
  var specDivergencesUnlisted = 0;
  var outFiles = [];

  fileOrder.forEach(function (filePath, fileIdx) {
    var entry = files[filePath];
    var scenarioOut = [];

    entry.scenarios.forEach(function (type, typeIdx) {
      var actual = recordResults[fileIdx][typeIdx];
      totalScenarios++;

      var specWanted = entry.specExpectedByType[type];
      if (specWanted !== actual) {
        var id = filePath + "(" + type + ")";
        if (!whitelist.has(id)) {
          specDivergencesUnlisted++;
          console.warn(
            "SANITY: " + id + " diverges from spec (expected " + specWanted +
            ", esprima+Node says " + actual + ") and is NOT in the upstream whitelist"
          );
        }
      }

      scenarioOut.push({ type: type, module: entry.module, expected: actual });
    });

    outFiles.push({ path: filePath, source: entry.source, scenarios: scenarioOut });
  });

  console.log(
    "Recorded " + totalScenarios + " scenario outcomes (" +
    specDivergencesUnlisted + " unlisted spec divergences, informational only)."
  );

  fs.rmSync(scratchTmp, { recursive: true, force: true });

  var corpusJson = JSON.stringify({ files: outFiles });
  var outPath = path.join(repoRoot, "test", "jsse-test262-corpus.js");
  fs.writeFileSync(outPath, "module.exports = " + JSON.stringify(corpusJson) + ";\n");
  console.log(
    "Wrote", outPath, "(" + (fs.statSync(outPath).size / (1024 * 1024)).toFixed(1) + " MiB)"
  );
});
