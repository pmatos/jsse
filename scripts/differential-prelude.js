// Prepended to every tests/differential/ file by scripts/run-differential.py.
// `show(...values)` prints one line in an engine-neutral format, so a corpus
// file never depends on a host's console.log/inspect styling. Everything here
// is plain ECMAScript, which means the formatter itself is under test too.
var show = (function () {
  function key(k) {
    return typeof k === "symbol" ? "[" + String(k) + "]" : /^[A-Za-z_$][\w$]*$/.test(k) ? k : JSON.stringify(k);
  }
  function fmt(v, seen, top) {
    switch (typeof v) {
      case "string": return top ? v : JSON.stringify(v);
      case "number": return Object.is(v, -0) ? "-0" : String(v);
      case "bigint": return String(v) + "n";
      case "symbol": return String(v);
      case "undefined": return "undefined";
      case "boolean": return String(v);
      case "function": return "[Function " + (v.name || "(anonymous)") + "]";
    }
    if (v === null) return "null";
    if (seen.indexOf(v) !== -1) return "[Circular]";
    seen = seen.concat([v]);
    const tag = Object.prototype.toString.call(v).slice(8, -1);
    if (v instanceof Error) return "[" + v.name + ": " + v.message + "]";
    if (v instanceof Date) return isNaN(v.getTime()) ? "Date(Invalid)" : "Date(" + v.toISOString() + ")";
    if (v instanceof RegExp) return String(v);
    if (v instanceof Map) {
      return "Map{" + Array.from(v, ([k, x]) => fmt(k, seen) + " => " + fmt(x, seen)).join(", ") + "}";
    }
    if (v instanceof Set) return "Set{" + Array.from(v, (x) => fmt(x, seen)).join(", ") + "}";
    if (ArrayBuffer.isView(v) && !(v instanceof DataView)) {
      return tag + "[" + Array.from(v, (x) => fmt(x, seen)).join(", ") + "]";
    }
    if (Array.isArray(v)) {
      const parts = [];
      for (let i = 0; i < v.length; i++) parts.push(i in v ? fmt(v[i], seen) : "<hole>");
      return "[" + parts.join(", ") + "]";
    }
    const keys = Reflect.ownKeys(v).filter((k) => Object.prototype.propertyIsEnumerable.call(v, k));
    const body = keys.map((k) => key(k) + ": " + fmt(v[k], seen)).join(", ");
    const name = tag === "Object" ? "" : tag + " ";
    return name + "{" + body + "}";
  }
  return function show() {
    const parts = [];
    for (let i = 0; i < arguments.length; i++) parts.push(fmt(arguments[i], [], true));
    console.log(parts.join(" "));
  };
})();
