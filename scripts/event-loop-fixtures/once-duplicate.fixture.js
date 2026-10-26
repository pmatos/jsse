// RUN: 0
// A once listener removes itself, not an equal plain listener registered
// with the same function.
function f() { console.log("f"); }
process.on("custom", f);
process.once("custom", f);
console.log(process.listenerCount("custom"));
process.emit("custom");
console.log(process.listenerCount("custom"));
process.emit("custom");
