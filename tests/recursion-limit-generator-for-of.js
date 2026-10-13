// Recursion through generator resumption driven by for-of must throw a
// catchable RangeError instead of overflowing the native stack: each nested
// `next()` step counts as a call level even though for-of resumes the
// generator directly rather than through a native `next` call.

function* nested(n) {
  for (const x of nested(n + 1)) yield x;
}

var err = null;
try {
  for (const x of nested(0)) {
  }
} catch (e) {
  err = e;
}
if (!(err instanceof RangeError)) {
  throw new Error("expected a RangeError, got " + String(err));
}

function* spreading(n) {
  yield* [...spreading(n + 1)];
}
err = null;
try {
  [...spreading(0)];
} catch (e) {
  err = e;
}
if (!(err instanceof RangeError)) {
  throw new Error("expected a RangeError from spread recursion, got " + String(err));
}

function* ok() {
  yield 1;
}
var seen = [];
for (const x of ok()) seen.push(x);
if (seen.length !== 1 || seen[0] !== 1) {
  throw new Error("generator iteration must still work after the overflow");
}
