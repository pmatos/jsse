// Calling a missing method: V8 names the callee in the message and points the
// caret at the method name.
const service = { start() {} };
function boot() {
  service.stop();
}
boot();
