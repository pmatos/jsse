// RUN: 1
// A throw caught inside the finally block does not hide the outer throw site.
const err = new Error("outer");
function cleanup() {
  try {
    throw new Error("inner");
  } catch {}
}
try {
  throw err;
} finally {
  cleanup();
}
