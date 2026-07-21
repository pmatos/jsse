// Copied into <repo>/test/jsse-entry.js by scripts/libs/esprima.sh's
// lib_prepare. Drives esprima's own suites through node-test-harness.js's
// describe/it TAP frontend (forced on both engines — see esprima.sh for why),
// so the harness's usual "PASS: p  FAIL: f  TOTAL: t" verdict line covers:
//
//   * the ~1,650 fixture-based unit tests (test/utils/create-testcases.js +
//     evaluate-testcase.js, ported inline because test/unit-tests.js itself
//     requires fs/path/json-diff at module load time purely for its
//     incomplete-fixture/diff-reporting fallbacks — paths we never take since
//     every fixture here is already complete);
//   * test/api-tests.js unmodified (its `require('assert')` resolves to
//     scripts/node-assert-module.js via an esbuild alias);
//   * a parse-only smoke check of everything.js's es2015-script/module
//     fixtures (test/grammar-tests.js, ported to avoid its runtime
//     `require('fs')`); and
//   * the two test/hostile-environment-tests.js environment-poisoning checks.
//
// test/regression-tests.js (7 real-world libraries diffed against
// test/3rdparty/syntax/*.json baselines) is deliberately NOT ported: even
// parse-only (no baseline diffing) it costs minutes of jsse tree-walker time
// to parse a few hundred KB of minified/real-world JS through esprima's own
// recursive-descent parser — itself interpreted — for only 7 assertions. The
// unit fixtures and the test262 corpus already cover "parses complex
// real-world JS" far more densely per second of wall-clock.
//
// test262 grammar-conformance corpus (scripts/gen-esprima-test262-corpus.js):
// one parse call per (file, scenario), in the exact file/scenario order the
// generator recorded `expected` in, so this replay is structurally the same
// parse sequence Node saw at generation time.

var esprima = require("../");
var createTestCases = require("./utils/create-testcases");
var evaluateTestCase = require("./utils/evaluate-testcase");
var everythingFixtures = require("./jsse-everything-fixtures");
var test262Corpus = JSON.parse(require("./jsse-test262-corpus"));

var STRICT_PREFIX = '"use strict";\n';

describe("esprima test262 grammar corpus", function () {
  test262Corpus.files.forEach(function (file) {
    file.scenarios.forEach(function (scenario) {
      var source = scenario.type === "strict mode" ? STRICT_PREFIX + file.source : file.source;
      it(file.path + " (" + scenario.type + ")", function () {
        var parseFn = scenario.module ? esprima.parseModule : esprima.parseScript;
        var actual;
        try {
          parseFn(source);
          actual = "pass";
        } catch (e) {
          actual = "fail";
        }
        if (actual !== scenario.expected) {
          throw new Error("expected " + scenario.expected + " but got " + actual);
        }
      });
    });
  });
});

describe("esprima unit fixtures", function () {
  var cases = createTestCases();
  Object.keys(cases).forEach(function (key) {
    var testCase = cases[key];
    if (
      testCase.hasOwnProperty("tree") ||
      testCase.hasOwnProperty("tokens") ||
      testCase.hasOwnProperty("failure") ||
      testCase.hasOwnProperty("result")
    ) {
      it(key, function () {
        evaluateTestCase(testCase);
      });
    }
  });
});

require("./api-tests.js");

describe("esprima grammar (everything.js smoke)", function () {
  it("parses es2015-script without throwing", function () {
    esprima.parse(everythingFixtures.script);
  });
  it("parses es2015-module without throwing", function () {
    esprima.parse(everythingFixtures.module, { sourceType: "module" });
  });
});

describe("esprima hostile environment", function () {
  it("parses when Object.defineProperty is deleted", function () {
    var defineProperty = Object.defineProperty;
    delete Object.defineProperty;
    try {
      esprima.parse("function f(a){}");
    } finally {
      Object.defineProperty = defineProperty;
    }
  });

  it("parses when Object.prototype has a poisoned getter", function () {
    Object.defineProperty(Object.prototype, "$a", {
      get: function () {},
      configurable: true,
    });
    try {
      esprima.parse("function f(a){}");
    } finally {
      delete Object.prototype.$a;
    }
  });
});
