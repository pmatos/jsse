// Shared in-process test-runner harness for jsse library-test bundles
// (Node host-compat epic, issue #232).
//
// Many real-world npm test suites don't ship a self-contained runner: they lean
// on a framework whose *assertion* library is pure JS but whose *runner* would
// otherwise need fs/workers/vm (mocha's/jest's CLI, QUnit's Node runner,
// qunit-extras' setInterval progress ticker, …). This prelude supplies the
// runner in-process so those suites execute on jsse:
//
//   * a QUnit adapter installed as global `QUnit` — suites that do
//     `root.QUnit || require('qunit-extras')` (e.g. lodash) pick it up and the
//     bundled framework stays dormant; and
//   * a TAP-emitting describe/it/test/before/after runner (mocha/jest/tape
//     shape) as the reusable spine for later library clusters.
//
// Like node-shim.js and node-buffer-shim.js, the whole prelude is INERT on real
// Node: there the suite's own framework (real qunit-extras, mocha, …) runs
// instead, which is exactly what lets run-library-tests.sh use Node as a
// same-bundle reference oracle — the count jsse reports through this adapter is
// cross-checked against the count the real framework reports on Node.
//
// No jsse src/ changes and nothing here touches jsse's default globals outside a
// library run, so test262 is unaffected.

(function () {
  "use strict";

  // Activate only under jsse's Node-host mode. `__host_write` is the #229
  // syscall floor, installed only when jsse runs with `--node`; it never exists
  // on real Node. Detecting real Node via `process`/`process.versions.node`
  // would be wrong here because node-shim.js (which loads before this prelude)
  // installs a *fake* process with `versions.node` set — so this prelude must
  // key off something node-shim.js does not fake. Staying inert on real Node is
  // essential: there the suite's own framework (real qunit-extras, mocha, …)
  // must run so run-library-tests.sh can use Node as a same-bundle reference
  // oracle for the cross-checked test count.
  if (typeof __host_write === "undefined") return;

  var g = globalThis;

  // jsse exposes only `globalThis`, not Node's `global` alias. Libraries and
  // their suites routinely compute their root object as
  // `(typeof global == 'object' && global) || this`; without this alias that
  // root would differ from `globalThis`, so properties a bundle sets on one
  // would be invisible through the other.
  if (typeof g.global === "undefined") {
    g.global = g;
  }

  var toString = Object.prototype.toString;

  // Capture the native timer at load time: some suites temporarily replace the
  // global `setTimeout` (e.g. lodash swaps in a synchronous stub while testing
  // debounce/throttle), and the runner's own scheduling must not be hijacked by
  // that.
  var nativeSetTimeout =
    typeof setTimeout === "function" ? setTimeout : null;
  function schedule(fn, ms) {
    if (nativeSetTimeout) return nativeSetTimeout(fn, ms);
    return Promise.resolve().then(fn);
  }

  // jsse's setTimeout spawns a fresh OS thread per call and always returns a
  // timer id of 0 (see src/interpreter/builtins/mod.rs), and it provides no
  // clearTimeout/setInterval/clearInterval at all. Timer-heavy libraries
  // (lodash's debounce/throttle churn thousands of setTimeout/clearTimeout
  // calls) both need cancellation *and* exhaust OS threads under that model,
  // which stalls the run. So back all four globals with a single userland timer
  // queue driven by ONE native timer at a time (the "pump"): library timers
  // become cheap queue entries with real ids, cancellation works, and the
  // process never holds more than a couple of native timer threads. The
  // runner's own scheduling keeps using the captured `nativeSetTimeout` above.
  if (nativeSetTimeout) {
    var timerQueue = [];
    var timerIdCounter = 1;
    var pumpArmed = false;

    function schedulePump() {
      if (pumpArmed || timerQueue.length === 0) return;
      pumpArmed = true;
      var soonest = Infinity;
      for (var i = 0; i < timerQueue.length; i++) {
        if (timerQueue[i].fireAt < soonest) soonest = timerQueue[i].fireAt;
      }
      var delay = soonest - Date.now();
      nativeSetTimeout(pump, delay > 0 ? delay : 0);
    }

    function pump() {
      pumpArmed = false;
      var now = Date.now();
      // Snapshot the due timers before running any callback: a callback may add
      // new timers (e.g. a recursive debounce), which belong to the next pump.
      var due = [];
      for (var i = 0; i < timerQueue.length; i++) {
        if (timerQueue[i].fireAt <= now) due.push(timerQueue[i]);
      }
      due.sort(function (a, b) {
        return a.fireAt - b.fireAt || a.id - b.id;
      });
      for (var j = 0; j < due.length; j++) {
        var t = due[j];
        var idx = timerQueue.indexOf(t);
        if (idx === -1) continue; // cleared during this pump
        timerQueue.splice(idx, 1);
        try {
          t.fn.apply(undefined, t.args);
        } catch (e) {
          // A host setTimeout callback that throws would crash the process; for
          // a test runner, keep pumping so one bad callback can't wedge the run.
        }
        if (t.interval != null) {
          t.fireAt = Date.now() + t.interval;
          timerQueue.push(t);
        }
      }
      schedulePump();
    }

    function addTimer(fn, ms, extraArgs, interval) {
      if (typeof fn !== "function") return 0;
      var id = timerIdCounter++;
      var delay = typeof ms === "number" && ms > 0 ? ms : 0;
      timerQueue.push({
        id: id,
        fireAt: Date.now() + delay,
        fn: fn,
        args: extraArgs,
        interval: interval,
      });
      schedulePump();
      return id;
    }

    function removeTimer(id) {
      if (id == null) return;
      for (var i = 0; i < timerQueue.length; i++) {
        if (timerQueue[i].id === id) {
          timerQueue.splice(i, 1);
          return;
        }
      }
    }

    g.setTimeout = function (fn, ms) {
      return addTimer(fn, ms, Array.prototype.slice.call(arguments, 2), null);
    };
    g.setInterval = function (fn, ms) {
      var iv = typeof ms === "number" && ms > 0 ? ms : 0;
      return addTimer(fn, ms, Array.prototype.slice.call(arguments, 2), iv);
    };
    g.clearTimeout = removeTimer;
    g.clearInterval = removeTimer;
  }

  // ==========================================================================
  // objectType + equiv — ported verbatim from qunitjs 2.4.1 (qunit/qunit.js),
  // the deepEqual engine. Hand-rolling structural equality is a known trap
  // (NaN, Date, RegExp, Map/Set, cyclic, null-proto objects), so we keep QUnit's
  // own implementation to match its semantics exactly.
  // ==========================================================================
  function objectType(obj) {
    if (typeof obj === "undefined") return "undefined";
    if (obj === null) return "null";
    var match = toString.call(obj).match(/^\[object\s(.*)\]$/),
      type = match && match[1];
    switch (type) {
      case "Number":
        return isNaN(obj) ? "nan" : "number";
      case "String":
      case "Boolean":
      case "Array":
      case "Set":
      case "Map":
      case "Date":
      case "RegExp":
      case "Function":
      case "Symbol":
        return type.toLowerCase();
      default:
        return typeof obj;
    }
  }

  var equiv = (function () {
    var pairs = [];
    var getProto =
      Object.getPrototypeOf ||
      function (obj) {
        return obj.__proto__;
      };

    function useStrictEquality(a, b) {
      if (typeof a === "object") a = a.valueOf();
      if (typeof b === "object") b = b.valueOf();
      return a === b;
    }

    function compareConstructors(a, b) {
      var protoA = getProto(a);
      var protoB = getProto(b);
      if (a.constructor === b.constructor) return true;
      if (protoA && protoA.constructor === null) protoA = null;
      if (protoB && protoB.constructor === null) protoB = null;
      if (
        (protoA === null && protoB === Object.prototype) ||
        (protoB === null && protoA === Object.prototype)
      ) {
        return true;
      }
      return false;
    }

    function getRegExpFlags(regexp) {
      return "flags" in regexp
        ? regexp.flags
        : regexp.toString().match(/[gimuy]*$/)[0];
    }

    function isContainer(val) {
      return ["object", "array", "map", "set"].indexOf(objectType(val)) !== -1;
    }

    function breadthFirstCompareChild(a, b) {
      if (a === b) return true;
      if (!isContainer(a)) return typeEquiv(a, b);
      if (
        pairs.every(function (pair) {
          return pair.a !== a || pair.b !== b;
        })
      ) {
        pairs.push({ a: a, b: b });
      }
      return true;
    }

    var callbacks = {
      string: useStrictEquality,
      boolean: useStrictEquality,
      number: useStrictEquality,
      null: useStrictEquality,
      undefined: useStrictEquality,
      symbol: useStrictEquality,
      date: useStrictEquality,

      nan: function () {
        return true;
      },

      regexp: function (a, b) {
        return a.source === b.source && getRegExpFlags(a) === getRegExpFlags(b);
      },

      function: function () {
        return false;
      },

      array: function (a, b) {
        var i, len;
        len = a.length;
        if (len !== b.length) return false;
        for (i = 0; i < len; i++) {
          if (!breadthFirstCompareChild(a[i], b[i])) return false;
        }
        return true;
      },

      set: function (a, b) {
        var innerEq,
          outerEq = true;
        if (a.size !== b.size) return false;
        a.forEach(function (aVal) {
          if (!outerEq) return;
          innerEq = false;
          b.forEach(function (bVal) {
            var parentPairs;
            if (innerEq) return;
            parentPairs = pairs;
            if (innerEquiv(bVal, aVal)) innerEq = true;
            pairs = parentPairs;
          });
          if (!innerEq) outerEq = false;
        });
        return outerEq;
      },

      map: function (a, b) {
        var innerEq,
          outerEq = true;
        if (a.size !== b.size) return false;
        a.forEach(function (aVal, aKey) {
          if (!outerEq) return;
          innerEq = false;
          b.forEach(function (bVal, bKey) {
            var parentPairs;
            if (innerEq) return;
            parentPairs = pairs;
            if (innerEquiv([bVal, bKey], [aVal, aKey])) innerEq = true;
            pairs = parentPairs;
          });
          if (!innerEq) outerEq = false;
        });
        return outerEq;
      },

      object: function (a, b) {
        var i,
          aProperties = [],
          bProperties = [];
        if (compareConstructors(a, b) === false) return false;
        for (i in a) {
          aProperties.push(i);
          if (
            a.constructor !== Object &&
            typeof a.constructor !== "undefined" &&
            typeof a[i] === "function" &&
            typeof b[i] === "function" &&
            a[i].toString() === b[i].toString()
          ) {
            continue;
          }
          if (!breadthFirstCompareChild(a[i], b[i])) return false;
        }
        for (i in b) {
          bProperties.push(i);
        }
        return typeEquiv(aProperties.sort(), bProperties.sort());
      },
    };

    function typeEquiv(a, b) {
      var type = objectType(a);
      return objectType(b) === type && callbacks[type](a, b);
    }

    function innerEquiv(a, b) {
      var i, pair;
      if (arguments.length < 2) return true;
      pairs = [{ a: a, b: b }];
      for (i = 0; i < pairs.length; i++) {
        pair = pairs[i];
        if (pair.a !== pair.b && !typeEquiv(pair.a, pair.b)) return false;
      }
      return (
        arguments.length === 2 ||
        innerEquiv.apply(this, [].slice.call(arguments, 1))
      );
    }

    return function () {
      var result = innerEquiv.apply(undefined, arguments);
      pairs.length = 0;
      return result;
    };
  })();

  // A compact value renderer for failure diagnostics (not Node's util.inspect).
  function dump(v) {
    try {
      if (typeof v === "string") return JSON.stringify(v);
      if (typeof v === "function")
        return "function " + (v.name || "(anonymous)");
      if (typeof v === "bigint") return String(v) + "n";
      if (v === undefined) return "undefined";
      var t = objectType(v);
      if (t === "nan") return "NaN";
      if (t === "array" || t === "object") {
        var s = JSON.stringify(v);
        if (s !== undefined) return s.length > 200 ? s.slice(0, 200) + "…" : s;
      }
      return String(v);
    } catch (e) {
      return Object.prototype.toString.call(v);
    }
  }

  // A microtask/timer yield so a long synchronous suite still lets the event
  // loop breathe between tests (and async tests resolve).
  function yieldTick() {
    return new Promise(function (resolve) {
      schedule(resolve, 0);
    });
  }

  // ==========================================================================
  // QUnit adapter — installed as global `QUnit`.
  // Mirrors qunitjs 2.x's module/test/assert surface and its assertion counting
  // (config.stats.all += assertions.length per test; expect() mismatch and the
  // no-assertion case each push one failing assertion — see Test#finish), so the
  // TOTAL this prints equals the TOTAL real qunit-extras prints on Node.
  // ==========================================================================
  var ASYNC_TIMEOUT_MS = 10000;
  // Backstop for the whole run: armed once at run start (while the engine's
  // timer subsystem is healthy), so even if a per-test guard later fails to fire
  // the harness still emits a summary rather than hanging forever.
  var GLOBAL_WATCHDOG_MS = 600000;

  function installQUnit() {
    var config = {
      autostart: true,
      current: null,
      // Fields real suites poke at (lodash sets these); accepted, and noglobals
      // is intentionally NOT enforced here — see the note in runAll().
      noglobals: false,
      hidepassed: false,
      requireExpects: false,
      asyncRetries: 0,
      testTimeout: undefined,
      modules: [],
    };

    var modules = [];
    var rootModule = { name: "", tests: [] };
    modules.push(rootModule);
    var currentModule = rootModule;

    var stats = { all: 0, bad: 0 };
    var doneCbs = [],
      beginCbs = [],
      testDoneCbs = [],
      moduleDoneCbs = [],
      logCbs = [];
    var started = false;
    var totalTests = 0;

    function normalizeHooks(hooks) {
      hooks = hooks || {};
      return {
        before: hooks.before,
        beforeEach: hooks.beforeEach,
        afterEach: hooks.afterEach,
        after: hooks.after,
      };
    }

    function moduleFn(name, hooks, nested) {
      // QUnit.module(name), QUnit.module(name, hooks), or
      // QUnit.module(name, hooks, nestedCallback). lodash uses only the first.
      if (typeof hooks === "function") {
        nested = hooks;
        hooks = undefined;
      }
      var mod = { name: name, tests: [], hooks: normalizeHooks(hooks) };
      modules.push(mod);
      var prev = currentModule;
      currentModule = mod;
      if (typeof nested === "function") {
        nested.call(mod.hooks);
        currentModule = prev;
      }
    }

    function testFn(name, callback) {
      currentModule.tests.push({
        testName: name,
        module: currentModule,
        callback: callback,
      });
      totalTests++;
    }

    function skipFn(name) {
      currentModule.tests.push({
        testName: name,
        module: currentModule,
        callback: null,
        skip: true,
      });
      totalTests++;
    }

    // ---- assert -------------------------------------------------------------
    function makeAssert(testObj) {
      var assert = {
        // async() returns a done() callback; the test finishes only once every
        // outstanding token has been released and the body has returned.
        async: function (count) {
          if (count === undefined) count = 1;
          testObj.pending += count;
          var remaining = count;
          return function done() {
            if (remaining <= 0) {
              pushResult(testObj, {
                result: false,
                message: "assert.async callback called more times than expected",
              });
              return;
            }
            remaining--;
            testObj.pending--;
            if (testObj.pending === 0 && testObj._asyncResolve) {
              var r = testObj._asyncResolve;
              testObj._asyncResolve = null;
              r();
            }
          };
        },

        expect: function (n) {
          if (arguments.length === 1) {
            testObj.expected = n;
            return;
          }
          return testObj.assertions.length;
        },

        step: function (message) {
          pushResult(testObj, {
            result: !!message,
            actual: message,
            message: message || "You must provide a message to assert.step",
          });
        },

        ok: function (state, message) {
          pushResult(testObj, {
            result: !!state,
            actual: !!state,
            expected: true,
            message: message,
          });
        },
        notOk: function (state, message) {
          pushResult(testObj, {
            result: !state,
            actual: !!state,
            expected: false,
            message: message,
          });
        },
        equal: function (actual, expected, message) {
          pushResult(testObj, {
            result: actual == expected,
            actual: actual,
            expected: expected,
            message: message,
          });
        },
        notEqual: function (actual, expected, message) {
          pushResult(testObj, {
            result: actual != expected,
            actual: actual,
            expected: expected,
            message: message,
          });
        },
        strictEqual: function (actual, expected, message) {
          pushResult(testObj, {
            result: actual === expected,
            actual: actual,
            expected: expected,
            message: message,
          });
        },
        notStrictEqual: function (actual, expected, message) {
          pushResult(testObj, {
            result: actual !== expected,
            actual: actual,
            expected: expected,
            message: message,
          });
        },
        deepEqual: function (actual, expected, message) {
          pushResult(testObj, {
            result: equiv(actual, expected),
            actual: actual,
            expected: expected,
            message: message,
          });
        },
        notDeepEqual: function (actual, expected, message) {
          pushResult(testObj, {
            result: !equiv(actual, expected),
            actual: actual,
            expected: expected,
            message: message,
          });
        },
        propEqual: function (actual, expected, message) {
          pushResult(testObj, {
            result: equiv(ownProps(actual), ownProps(expected)),
            actual: actual,
            expected: expected,
            message: message,
          });
        },
        pushResult: function (r) {
          pushResult(testObj, r);
        },
      };

      function throwsImpl(block, expected, message) {
        if (
          typeof expected === "string" &&
          arguments.length === 2 &&
          message === undefined
        ) {
          message = expected;
          expected = undefined;
        }
        var actual;
        try {
          block.call(testObj.testEnv);
        } catch (e) {
          actual = e;
        }
        var result = false;
        if (actual !== undefined) {
          if (expected === undefined) {
            result = true;
          } else if (expected instanceof RegExp) {
            result = expected.test(errorString(actual));
          } else if (
            typeof expected === "function" &&
            actual instanceof expected
          ) {
            result = true;
          } else if (typeof expected === "function") {
            // Validator function form (not an Error subclass match).
            result = expected.call(null, actual) === true;
          } else if (expected instanceof Error) {
            result =
              actual.name === expected.name &&
              actual.message === expected.message;
          } else if (objectType(expected) === "object") {
            result =
              actual.name === expected.name &&
              actual.message === expected.message;
          }
        }
        pushResult(testObj, {
          result: result,
          actual: actual && errorString(actual),
          expected: expected,
          message: message,
        });
      }
      assert.throws = throwsImpl;
      assert.raises = throwsImpl;

      return assert;
    }

    function ownProps(obj) {
      if (obj === null || typeof obj !== "object") return obj;
      var out = {};
      for (var k in obj) {
        if (Object.prototype.hasOwnProperty.call(obj, k)) out[k] = obj[k];
      }
      return out;
    }

    function errorString(err) {
      if (err && typeof err.toString === "function") {
        var s = err.toString();
        if (s !== "[object Object]") return s;
      }
      if (err && err.name !== undefined) {
        return err.name + (err.message ? ": " + err.message : "");
      }
      return String(err);
    }

    function pushResult(testObj, r) {
      testObj.assertions.push({
        result: !!r.result,
        message: r.message,
        actual: r.actual,
        expected: r.expected,
      });
    }

    function pushFailure(testObj, message) {
      testObj.assertions.push({ result: false, message: message });
    }

    // ---- runner -------------------------------------------------------------
    var failedTests = [];

    async function runHook(hook, testObj, assert) {
      if (typeof hook === "function") {
        await Promise.resolve(hook.call(testObj.testEnv, assert));
      }
    }

    async function runTest(testObj) {
      testObj.assertions = [];
      testObj.expected = null;
      testObj.pending = 0;
      testObj.testEnv = {};
      config.current = testObj;

      if (testObj.skip) {
        // Skipped tests contribute no assertions, matching QUnit.
        finalize(testObj);
        return;
      }

      var assert = makeAssert(testObj);
      var hooks = testObj.module.hooks || {};

      try {
        await runHook(hooks.beforeEach, testObj, assert);
        var ret = testObj.callback.call(testObj.testEnv, assert);
        await Promise.resolve(ret);
        if (testObj.pending > 0) {
          // Wait for every assert.async() token to be released, but bound the
          // wait: a test whose done() never fires (e.g. a deferred callback
          // that throws on jsse before calling it) must not stall the whole
          // run. On timeout, record a failure and move on — mirrors QUnit's
          // config.testTimeout. jsse has no clearTimeout, so the guard timer
          // fires harmlessly later (it no-ops once _asyncResolve is cleared).
          await new Promise(function (resolve) {
            testObj._asyncResolve = resolve;
            // Use the captured native timer: a suite may have swapped the global
            // setTimeout for a synchronous or broken stub by now, and the guard
            // must still fire so a never-completing async test can't stall the
            // whole run.
            schedule(function () {
              if (testObj._asyncResolve) {
                testObj._asyncResolve = null;
                pushFailure(
                  testObj,
                  "async test timed out after " + ASYNC_TIMEOUT_MS + "ms"
                );
                resolve();
              }
            }, ASYNC_TIMEOUT_MS);
          });
        }
        await runHook(hooks.afterEach, testObj, assert);
      } catch (e) {
        pushFailure(
          testObj,
          "Died on test #" +
            testObj.testName +
            ": " +
            (e && e.stack ? e.stack : e)
        );
      }

      finalize(testObj);
    }

    function finalize(testObj) {
      // Replicates qunitjs 2.4.1 Test#finish exactly so TOTAL matches Node.
      if (
        config.requireExpects &&
        testObj.expected === null &&
        !testObj.skip
      ) {
        pushFailure(
          testObj,
          "Expected number of assertions to be defined, but expect() was not called."
        );
      } else if (
        testObj.expected !== null &&
        testObj.expected !== testObj.assertions.length
      ) {
        pushFailure(
          testObj,
          "Expected " +
            testObj.expected +
            " assertions, but " +
            testObj.assertions.length +
            " were run"
        );
      } else if (
        testObj.expected === null &&
        !testObj.assertions.length &&
        !testObj.skip
      ) {
        pushFailure(
          testObj,
          "Expected at least one assertion, but none were run - call expect(0) to accept zero assertions."
        );
      }

      var bad = 0;
      stats.all += testObj.assertions.length;
      for (var i = 0; i < testObj.assertions.length; i++) {
        if (!testObj.assertions[i].result) bad++;
      }
      stats.bad += bad;

      if (bad) recordFailure(testObj);

      for (var j = 0; j < testDoneCbs.length; j++) {
        testDoneCbs[j]({
          name: testObj.testName,
          module: testObj.module.name,
          failed: bad,
          passed: testObj.assertions.length - bad,
          total: testObj.assertions.length,
          runtime: 0,
        });
      }
      config.current = null;
    }

    function recordFailure(testObj) {
      var label =
        (testObj.module.name ? testObj.module.name + ": " : "") +
        testObj.testName;
      var lines = ["not ok - " + label];
      for (var i = 0; i < testObj.assertions.length; i++) {
        var a = testObj.assertions[i];
        if (a.result) continue;
        var detail = "    " + (a.message || "(no message)");
        if ("expected" in a || "actual" in a) {
          detail +=
            " | expected: " +
            dump(a.expected) +
            ", actual: " +
            dump(a.actual);
        }
        lines.push(detail);
      }
      failedTests.push(lines.join("\n"));
    }

    async function runAll() {
      for (var b = 0; b < beginCbs.length; b++) {
        beginCbs[b]({ totalTests: totalTests });
      }

      // NOTE on config.noglobals: real QUnit fails a test if it leaks a new
      // global. We deliberately do NOT enforce it here. The Node oracle (real
      // qunit-extras) does enforce it and the suite passes it there (0 leaks),
      // so enforcing on jsse could only ADD failures the oracle doesn't have —
      // e.g. from jsse-specific global enumeration — which would diverge the
      // cross-checked count. Not enforcing keeps jsse strictly no stricter than
      // the oracle, so a genuine leak still shows up as a Node-side failure.

      var aborted = false;
      var abortedAt = 0;
      schedule(function () {
        aborted = true;
        // Unstick a test currently blocked on an async token that will never
        // resolve, so the run loop can observe `aborted` and finish.
        var cur = config.current;
        if (cur && cur._asyncResolve) {
          var r = cur._asyncResolve;
          cur._asyncResolve = null;
          pushFailure(
            cur,
            "run aborted by global watchdog after " + GLOBAL_WATCHDOG_MS + "ms"
          );
          r();
        }
      }, GLOBAL_WATCHDOG_MS);

      var count = 0;
      for (var m = 0; m < modules.length && !aborted; m++) {
        var mod = modules[m];
        for (var t = 0; t < mod.tests.length && !aborted; t++) {
          await runTest(mod.tests[t]);
          count++;
          if (count % 200 === 0) await yieldTick();
          // Liveness on stderr (ignored by the verdict, which parses the final
          // stdout summary) so a slow tree-walker run is visibly progressing.
          if (count % 1000 === 0) {
            process.stderr.write(
              "… " + count + " tests run (" + stats.bad + " failing assertions)\n"
            );
          }
        }
      }
      if (aborted) abortedAt = count;

      var details = {
        passed: stats.all - stats.bad,
        failed: stats.bad,
        total: stats.all,
        runtime: 0,
      };

      if (aborted) {
        console.log("");
        console.log(
          "WATCHDOG: run aborted after " +
            abortedAt +
            " tests (engine timer/timing limit reached); remaining tests not run."
        );
      }

      if (failedTests.length) {
        console.log("");
        console.log(
          "Failures (" + failedTests.length + " test(s) with failing assertions):"
        );
        var shown = failedTests.length > 100 ? 100 : failedTests.length;
        for (var f = 0; f < shown; f++) console.log(failedTests[f]);
        if (failedTests.length > shown) {
          console.log(
            "… " + (failedTests.length - shown) + " more failing test(s) omitted"
          );
        }
        console.log("");
      }

      for (var d = 0; d < doneCbs.length; d++) doneCbs[d](details);

      // The line run-library-tests.sh's verdict parses — byte-identical to
      // qunit-extras' summary (qunit-extras.js line 506), which real Node prints.
      console.log(
        "    PASS: " +
          details.passed +
          "  FAIL: " +
          details.failed +
          "  TOTAL: " +
          details.total
      );
    }

    function start() {
      if (started) return;
      started = true;
      // Fire-and-forget: the returned promise chain continues on the microtask/
      // timer queue, which jsse drains after the main script returns.
      runAll().catch(function (e) {
        console.log("Harness error: " + (e && e.stack ? e.stack : e));
        console.log("    PASS: 0  FAIL: 1  TOTAL: 1");
      });
    }

    var QUnit = {
      config: config,
      module: moduleFn,
      test: testFn,
      skip: skipFn,
      todo: testFn,
      only: testFn,
      start: start,
      load: function () {},
      begin: function (cb) {
        beginCbs.push(cb);
      },
      done: function (cb) {
        doneCbs.push(cb);
      },
      testDone: function (cb) {
        testDoneCbs.push(cb);
      },
      moduleDone: function (cb) {
        moduleDoneCbs.push(cb);
      },
      log: function (cb) {
        logCbs.push(cb);
      },
      testStart: function () {},
      moduleStart: function () {},
      extend: function (target, mixin) {
        for (var k in mixin)
          if (Object.prototype.hasOwnProperty.call(mixin, k))
            target[k] = mixin[k];
        return target;
      },
      push: function () {},
      assert: null,
      equiv: equiv,
      objectType: objectType,
      dump: { parse: dump },
      is: function (type, obj) {
        return objectType(obj) === type;
      },
    };
    return QUnit;
  }

  g.QUnit = installQUnit();

  // ==========================================================================
  // TAP describe/it/test/before/after runner — the reusable spine.
  // Not exercised by lodash (which is QUnit); it is the shape mocha/jest/tape
  // suites use and is self-verified by scripts/shim-fixtures. Emits TAP 13.
  // ==========================================================================
  function installTap() {
    function makeSuite(name, parent) {
      return {
        name: name,
        parent: parent,
        // A single ordered list of children (tests and nested suites) so
        // execution follows definition order, the way mocha/jest run them.
        children: [],
        before: [],
        after: [],
        beforeEach: [],
        afterEach: [],
      };
    }

    var rootSuite = makeSuite("", null);
    var currentSuite = rootSuite;
    var started = false;
    var counter = 0;
    var passed = 0,
      failed = 0;

    function describe(name, fn) {
      var suite = makeSuite(name, currentSuite);
      currentSuite.children.push({ kind: "suite", suite: suite });
      var prev = currentSuite;
      currentSuite = suite;
      if (typeof fn === "function") fn.call(suite);
      currentSuite = prev;
    }

    function it(name, fn) {
      currentSuite.children.push({
        kind: "test",
        test: { name: name, fn: fn, suite: currentSuite },
      });
    }

    function xit(name) {
      currentSuite.children.push({
        kind: "test",
        test: { name: name, fn: null, suite: currentSuite, skip: true },
      });
    }

    function hookRegister(kind) {
      return function (fn) {
        currentSuite[kind].push(fn);
      };
    }

    function fullName(suite, testName) {
      var parts = [];
      var s = suite;
      while (s && s.name) {
        parts.unshift(s.name);
        s = s.parent;
      }
      parts.push(testName);
      return parts.join(" > ");
    }

    function eachHook(suite, kind, cb) {
      // beforeEach: outermost→innermost; afterEach: innermost→outermost.
      var chain = [];
      var s = suite;
      while (s) {
        chain.push(s);
        s = s.parent;
      }
      if (kind === "beforeEach") chain.reverse();
      var out = [];
      for (var i = 0; i < chain.length; i++) {
        var hooks = chain[i][kind];
        for (var j = 0; j < hooks.length; j++) out.push(hooks[j]);
      }
      return out;
    }

    async function runHooks(hooks, ctx) {
      for (var i = 0; i < hooks.length; i++) {
        await Promise.resolve(hooks[i].call(ctx));
      }
    }

    async function runOneTest(t) {
      counter++;
      var name = fullName(t.suite, t.name);
      if (t.skip) {
        console.log("ok " + counter + " - " + name + " # SKIP");
        passed++;
        return;
      }
      var testCtx = {};
      var ok = true,
        errText = "";
      try {
        await runHooks(eachHook(t.suite, "beforeEach"), testCtx);
        await Promise.resolve(t.fn.call(testCtx));
        await runHooks(eachHook(t.suite, "afterEach"), testCtx);
      } catch (e) {
        ok = false;
        errText = e && e.stack ? e.stack : String(e);
      }
      if (ok) {
        console.log("ok " + counter + " - " + name);
        passed++;
      } else {
        console.log("not ok " + counter + " - " + name);
        console.log("  ---");
        var lines = String(errText).split("\n");
        for (var e2 = 0; e2 < lines.length; e2++) {
          console.log("  " + lines[e2]);
        }
        console.log("  ...");
        failed++;
      }
    }

    async function runSuite(suite) {
      var ctx = {};
      await runHooks(suite.before, ctx);
      // Children run in definition order (tests interleaved with nested suites).
      for (var i = 0; i < suite.children.length; i++) {
        var child = suite.children[i];
        if (child.kind === "test") {
          await runOneTest(child.test);
        } else {
          await runSuite(child.suite);
        }
      }
      await runHooks(suite.after, ctx);
    }

    async function runAll() {
      console.log("TAP version 13");
      await runSuite(rootSuite);
      console.log("1.." + counter);
      console.log("# tests " + counter);
      console.log("# pass " + passed);
      console.log("# fail " + failed);
      // A summary line in the same shape the QUnit adapter and verdict use.
      console.log(
        "    PASS: " + passed + "  FAIL: " + failed + "  TOTAL: " + counter
      );
    }

    function run() {
      if (started) return;
      started = true;
      runAll().catch(function (e) {
        console.log("Harness error: " + (e && e.stack ? e.stack : e));
        console.log("    PASS: 0  FAIL: 1  TOTAL: 1");
      });
    }

    // Auto-run after the main script returns, so a bundle that only registers
    // describe/it blocks (no explicit run call) still executes — but stay silent
    // (no TAP, no summary line) when nothing was registered, so a QUnit-only
    // suite like lodash sees exactly one PASS/FAIL/TOTAL line (QUnit's).
    Promise.resolve().then(function () {
      if (rootSuite.children.length) run();
    });

    return {
      describe: describe,
      it: it,
      xit: xit,
      xdescribe: function (name) {
        describe(name, function () {});
      },
      before: hookRegister("before"),
      after: hookRegister("after"),
      beforeEach: hookRegister("beforeEach"),
      afterEach: hookRegister("afterEach"),
      run: run,
    };
  }

  var tap = installTap();
  // Expose the mocha/jest/tape-shaped globals. `test` is an alias for `it`
  // (jest/tape style); suites that also want QUnit.test use the QUnit global.
  g.describe = tap.describe;
  g.it = tap.it;
  g.test = tap.it;
  g.xit = tap.xit;
  g.xdescribe = tap.xdescribe;
  g.before = tap.before;
  g.after = tap.after;
  g.beforeEach = tap.beforeEach;
  g.afterEach = tap.afterEach;
  g.__tapRun = tap.run;
})();
