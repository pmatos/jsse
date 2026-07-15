// Shared Node host-compat prelude for jsse library-test bundles.
//
// This shim is prepended (via scripts/run-library-tests.sh) to an esbuild
// bundle so that a real-world npm library's own test runner — written against
// Node globals — can execute on jsse. It is a pure-JS shim: nothing here is
// baked into jsse's default global object, so test262 is unaffected.
//
// Every stub is guarded so that on Node (where these globals already exist)
// the shim is an inert no-op. That lets `run-library-tests.sh --node` run the
// exact same bundle against Node as a reference oracle.

if (typeof console.group === "undefined") {
  console.group = function (name) {
    console.log("--- " + name + " ---");
  };
}
if (typeof console.groupEnd === "undefined") {
  console.groupEnd = function () {};
}

if (typeof process === "undefined") {
  // Line-buffered writer: jsse only exposes newline-appending console.log, so
  // accumulate partial writes and emit one console.log per completed line.
  // This reconstructs Node's byte-stream stdout/stderr closely enough for
  // test runners that print progress with process.stdout.write(str) (no "\n").
  var makeStream = function () {
    var buf = "";
    return {
      write: function (chunk, encodingOrCb, cb) {
        buf += String(chunk);
        var idx;
        while ((idx = buf.indexOf("\n")) !== -1) {
          console.log(buf.slice(0, idx));
          buf = buf.slice(idx + 1);
        }
        var callback = typeof encodingOrCb === "function" ? encodingOrCb : cb;
        if (typeof callback === "function") callback();
        return true;
      },
      // Flush any trailing partial line (no terminating newline).
      _flush: function () {
        if (buf.length) {
          console.log(buf);
          buf = "";
        }
      },
    };
  };

  var stdout = makeStream();
  var stderr = makeStream();

  globalThis.process = {
    argv: ["node", "bundle.js"],
    env: {},
    platform: "linux",
    stdout: stdout,
    stderr: stderr,
    exit: function (code) {
      // Flush buffered output before exiting so a final partial line is not
      // lost. exit(0) falls through (a library's runner often calls it at the
      // end); a non-zero code throws so the harness sees the failure.
      stdout._flush();
      stderr._flush();
      if (code) throw new Error("process.exit(" + code + ")");
    },
    // Node's high-resolution timer, [seconds, nanoseconds]. jsse has no
    // monotonic clock (that arrives with the flag-gated Rust floor), so derive
    // it from Date.now(). Only used by test runners to report elapsed time, so
    // millisecond resolution is sufficient; correctness never depends on it.
    hrtime: function (prev) {
      var ms = Date.now();
      var s = Math.floor(ms / 1000);
      var ns = (ms % 1000) * 1e6;
      if (prev) {
        var ds = s - prev[0];
        var dns = ns - prev[1];
        if (dns < 0) {
          ds -= 1;
          dns += 1e9;
        }
        return [ds, dns];
      }
      return [s, ns];
    },
  };
}
