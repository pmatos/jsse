// Self-verifying fixture for scripts/node-string-decoder-module.js.
//
// The runner loads the JSSE selector into `module.exports`; under Node the same
// selector exposes the native node:string_decoder implementation as the oracle.

(function () {
  "use strict";

  var StringDecoder = module.exports.StringDecoder;
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
      fail(
        msg +
          " — expected " +
          JSON.stringify(expected) +
          ", got " +
          JSON.stringify(actual)
      );
    }
  }

  var utf8 = new StringDecoder("utf8");
  eq(utf8.write(Buffer.from([0xe2])), "", "utf8 buffers partial lead");
  eq(
    utf8.write(Buffer.from([0x82, 0xac])),
    "€",
    "utf8 completes across writes"
  );
  eq(utf8.end(), "", "utf8 end after complete input");

  var emoji = new StringDecoder("utf-8");
  eq(
    emoji.write(Buffer.from([0xf0, 0x9f])),
    "",
    "utf8 buffers multi-byte prefix"
  );
  eq(
    emoji.end(Buffer.from([0x98, 0x80])),
    "😀",
    "end buffer completes utf8 character"
  );

  var truncated = new StringDecoder("utf8");
  eq(
    truncated.write(Buffer.from([0xe2])),
    "",
    "utf8 holds truncated input until end"
  );
  eq(truncated.end(), "�", "utf8 end flushes incomplete input");
  eq(truncated.write(Buffer.from("A")), "A", "utf8 decoder is reusable after end");

  var invalid = new StringDecoder("utf8");
  eq(
    invalid.write(Buffer.from([0xf4, 0xa9])),
    "",
    "utf8 defers an invalid structural prefix"
  );
  eq(
    invalid.end(Buffer.from([0x8c, 0x76])),
    "���v",
    "utf8 finalizes deferred invalid bytes together"
  );

  var bom = new StringDecoder("utf8");
  eq(bom.write(Buffer.from([0xef])), "", "utf8 buffers split BOM");
  eq(
    bom.end(Buffer.from([0xbb, 0xbf, 0x41])),
    "﻿A",
    "StringDecoder preserves UTF-8 BOM"
  );

  var utf16 = new StringDecoder("ucs2");
  eq(
    utf16.write(Buffer.from([0x41])),
    "",
    "utf16 buffers odd trailing byte"
  );
  eq(
    utf16.write(Buffer.from([0x00])),
    "A",
    "utf16 completes code unit across writes"
  );
  eq(
    utf16.write(Buffer.from([0x3d, 0xd8])),
    "",
    "utf16 buffers trailing high surrogate"
  );
  eq(
    utf16.end(Buffer.from([0x00, 0xde])),
    "😀",
    "utf16 completes surrogate pair across writes"
  );

  var utf16Trailing = new StringDecoder("utf16le");
  utf16Trailing.write(Buffer.from([0x41]));
  eq(
    utf16Trailing.end(),
    "",
    "utf16 end drops an unmatched trailing byte"
  );
  var utf16Surrogate = new StringDecoder("utf16le");
  utf16Surrogate.write(Buffer.from([0x3d, 0xd8]));
  eq(
    utf16Surrogate.end(),
    "\ud83d",
    "utf16 end preserves unmatched high surrogate"
  );

  var base64 = new StringDecoder("base64");
  eq(base64.write(Buffer.from([0x61])), "", "base64 buffers incomplete group");
  eq(
    base64.write(Buffer.from([0x62, 0x63])),
    "YWJj",
    "base64 completes group across writes"
  );
  eq(
    base64.end(Buffer.from([0x64, 0x65])),
    "ZGU=",
    "base64 end flushes partial group"
  );

  var base64url = new StringDecoder("base64url");
  eq(
    base64url.write(Buffer.from([0xfb])),
    "",
    "base64url buffers incomplete group"
  );
  eq(
    base64url.end(Buffer.from([0xff, 0xbf])),
    "-_-_",
    "base64url completes group at end"
  );

  var total = passed + failed;
  console.log("SHIM-FIXTURE: " + passed + " of " + total + " assertions passed");
  if (failed > 0) {
    throw new Error(failed + " assertion(s) failed");
  }
})();
