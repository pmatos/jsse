// RUN: 1
// A throw in an imported module has that module's top-level frame.
import "./fatal-esm-dep.helper.mjs";
console.error("never");
