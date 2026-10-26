// RUN: 0
process.on("warning", function (w) {
  if (w.name === "TestWarning" || w.name === "DeprecationWarning") {
    console.log(w.name, w.message, w.code, w instanceof Error);
  }
});
process.emitWarning("careful", "TestWarning", "TW001");
process.emitWarning("old thing", { type: "DeprecationWarning", code: "DEP0XXX" });
console.log("sync done");
