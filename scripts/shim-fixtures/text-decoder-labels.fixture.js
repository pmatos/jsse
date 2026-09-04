// Self-verifying TextDecoder label fixture for scripts/node-buffer-shim.js.
//
// The Encoding Standard has a different label table from Node's Buffer API.
// Run through run-shim-fixtures.sh, these assertions compare the shim on jsse
// with Node's native TextDecoder.

(function () {
  "use strict";

  var passed = 0;
  var failed = 0;

  function fail(msg) {
    failed++;
    console.log("FAIL: " + msg);
  }

  function eq(actual, expected, msg) {
    if (actual === expected) {
      passed++;
    } else {
      fail(msg + " — expected " + expected + ", got " + actual);
    }
  }

  function throwsRangeError(fn, msg) {
    try {
      fn();
      fail(msg + " — expected a RangeError");
    } catch (e) {
      if (e instanceof RangeError) {
        passed++;
      } else {
        fail(msg + " — expected a RangeError, got " + e.name);
      }
    }
  }

  ["utf16le", "ucs2", "utf16be", "binary"].forEach(function (label) {
    throwsRangeError(function () {
      new TextDecoder(label);
    }, "TextDecoder rejects Buffer-only label " + label);
  });

  [
    ["unicode-1-1-utf-8", "utf-8"],
    ["unicode11utf8", "utf-8"],
    ["unicode20utf8", "utf-8"],
    ["utf8", "utf-8"],
    ["x-unicode20utf8", "utf-8"],
    ["ansi_x3.4-1968", "windows-1252"],
    ["cp1252", "windows-1252"],
    ["cp819", "windows-1252"],
    ["csisolatin1", "windows-1252"],
    ["ibm819", "windows-1252"],
    ["iso-ir-100", "windows-1252"],
    ["iso8859-1", "windows-1252"],
    ["iso88591", "windows-1252"],
    ["iso_8859-1", "windows-1252"],
    ["iso_8859-1:1987", "windows-1252"],
    ["l1", "windows-1252"],
    ["x-cp1252", "windows-1252"],
    ["csunicode", "utf-16le"],
    ["iso-10646-ucs-2", "utf-16le"],
    ["ucs-2", "utf-16le"],
    ["unicode", "utf-16le"],
    ["unicodefeff", "utf-16le"],
    ["utf-16", "utf-16le"],
    ["unicodefffe", "utf-16be"],
  ].forEach(function (entry) {
    eq(
      new TextDecoder(entry[0]).encoding,
      entry[1],
      "TextDecoder canonicalizes standard label " + entry[0]
    );
  });

  var total = passed + failed;
  console.log("SHIM-FIXTURE: " + passed + " of " + total + " assertions passed");
  if (failed > 0) {
    throw new Error(failed + " assertion(s) failed");
  }
})();
