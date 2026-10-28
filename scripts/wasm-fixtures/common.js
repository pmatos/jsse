// Shared helpers of the WebAssembly differential corpus (issue #1378): each fixture prints one line
// per check and its stdout must be byte-identical on `jsse --node` and Node. Run via
// scripts/run-node-wasm-oracle.sh.
"use strict";

const out = [];
function show(v) {
  if (typeof v === "bigint") return v + "n";
  if (typeof v === "function") return "function " + v.name + "/" + v.length;
  if (typeof v === "number" && Object.is(v, -0)) return "-0";
  if (typeof v === "number" && v !== v) return "NaN";
  if (v instanceof ArrayBuffer) return "ArrayBuffer(" + v.byteLength + ")";
  if (v instanceof Uint8Array) return "Uint8Array[" + Array.from(v).join(",") + "]";
  if (Array.isArray(v)) return "[" + v.map(show).join(",") + "]";
  if (v === undefined) return "undefined";
  if (v !== null && typeof v === "object") {
    return "{" + Object.keys(v).map((k) => k + ":" + show(v[k])).join(",") + "}";
  }
  return JSON.stringify(v);
}
function emit(label, value) {
  out.push(label + " " + (typeof value === "string" ? value : show(value)));
}
// Records the result, or the error's name (and message when `withMessage`).
function attempt(label, fn, withMessage = false) {
  try {
    emit(label, fn());
  } catch (e) {
    emit(label, "throws " + (e && e.name) + (withMessage ? ": " + e.message : ""));
  }
}
async function attemptAsync(label, fn, withMessage = false) {
  try {
    emit(label, await fn());
  } catch (e) {
    emit(label, "rejects " + (e && e.name) + (withMessage ? ": " + e.message : ""));
  }
}
function flush() {
  process.stdout.write(out.join("\n") + "\n");
}

module.exports = { emit, attempt, attemptAsync, flush, show };
