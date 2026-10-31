// The frames of a timer or immediate callback (issue #1508): V8's
// ` [as _onTimeout]` suffix and the names of Node's own timer frames, run
// verbatim on Node and on `jsse --node` and pinned in expected-timers-1508.txt
// (tests/node_stack_frames_1508.rs). Only the position inside node:internal/timers
// differs between the hosts, so it is dropped.
const path = require("node:path");
const dir = path.dirname(__filename);

function show(label, stack) {
  const lines = String(stack)
    .split("\n")
    .filter((line) => !/^\s+at /.test(line) || line.includes(dir + "/") || line.includes("node:internal/timers:"))
    .map((line) => line.replace(/\(node:internal\/timers:\d+:\d+\)/, "(node:internal/timers)"));
  console.log("## " + label);
  console.log(lines.join("\n").split(dir).join("<dir>"));
}

setTimeout(function named() {
  show("a named timeout callback", new Error("timeout").stack);
  setImmediate(function immediate() {
    show("a named immediate callback", new Error("immediate").stack);
    const interval = setInterval(function tick() {
      show("a named interval callback", new Error("interval").stack);
      clearInterval(interval);
      setTimeout(() => {
        show("an arrow timeout callback", new Error("arrow").stack);
        setTimeout(function () {
          show("an anonymous timeout callback", new Error("anonymous").stack);
        }, 1);
      }, 1);
    }, 1);
  });
}, 1);
