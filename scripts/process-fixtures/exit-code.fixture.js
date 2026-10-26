// RUN: 7 code7
// RUN: 7 string
// RUN: 16 hex
// RUN: 44 wrap
// RUN: 255 negative
// RUN: 1 throw
// RUN: 5 exit-no-arg
// RUN: 0 exit-zero
// RUN: 3 exit-arg
// RUN: 0 null
// RUN: 0 undef
// RUN: 6 timer
// RUN: 8 promise
// RUN: 9 skip-catch
// RUN: 0 range
// RUN: 0 type
// RUN: 0 exit-range
// RUN: 2 reassign
function show(label, value) {
  console.log(label + " " + JSON.stringify(value));
}
function err(fn) {
  try {
    fn();
    show("no error", null);
  } catch (e) {
    show("error", [e.name, e.code, e.message]);
  }
}
var mode = process.argv[2];
show("initial", typeof process.exitCode);
show("descriptor", (function () {
  var d = Object.getOwnPropertyDescriptor(process, "exitCode");
  return [typeof d.get, typeof d.set, d.enumerable, d.configurable];
})());
switch (mode) {
  case "code7": process.exitCode = 7; break;
  case "string": process.exitCode = "7"; show("readback", [typeof process.exitCode, process.exitCode]); break;
  case "hex": process.exitCode = "0x10"; break;
  case "wrap": process.exitCode = 300; break;
  case "negative": process.exit(-1); break;
  case "throw": process.exitCode = 4; throw new Error("boom");
  case "exit-no-arg": process.exitCode = 5; process.exit(); break;
  case "exit-zero": process.exitCode = 5; process.exit(0); break;
  case "exit-arg": process.exit(3); break;
  case "null": process.exitCode = 3; process.exitCode = null; show("readback", typeof process.exitCode); break;
  case "undef": process.exitCode = 3; process.exitCode = undefined; show("readback", typeof process.exitCode); break;
  case "timer": setTimeout(function () { process.exitCode = 6; show("timer", true); }, 5); break;
  case "promise": void Promise.resolve().then(function () { process.exitCode = 8; show("promise", true); }); break;
  case "skip-catch":
    try { process.exit(9); } catch (e) { show("caught", true); } finally { show("finally", true); }
    break;
  case "range": err(function () { process.exitCode = 1.5; }); err(function () { process.exitCode = NaN; }); err(function () { process.exitCode = "1.5"; }); break;
  case "type":
    err(function () { process.exitCode = "abc"; });
    err(function () { process.exitCode = ""; });
    err(function () { process.exitCode = {}; });
    err(function () { process.exitCode = true; });
    err(function () { process.exitCode = []; });
    err(function () { process.exitCode = 10n; });
    break;
  case "exit-range": err(function () { process.exit(1.5); }); err(function () { process.exit("x"); }); break;
  case "reassign": process.exitCode = 1; process.exitCode = 2; break;
}
show("end", mode);
