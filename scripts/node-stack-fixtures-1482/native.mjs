// Builtin frames (`at Array.map (<anonymous>)`) of --node stacks (issue #1482).
Error.stackTraceLimit = 100;
const fail = () => {
  throw new Error("x");
};
const frames = (e) =>
  e.stack
    .split("\n")
    .slice(1)
    .filter((line) => !line.includes("node:"))
    .join("\n");
const show = (label, run) => {
  try {
    run();
  } catch (e) {
    console.log("## " + label + "\n" + frames(e));
  }
};

show("Array.prototype.map", () => [1].map(fail));
show("Array.prototype.forEach", () => [1].forEach(fail));
show("Array.prototype.sort", () => [2, 1].sort(fail));
show("Array.prototype.toSorted", () => [2, 1].toSorted(fail));
show("Array.prototype.flatMap", () => [1].flatMap(fail));
show("Array.prototype.find", () => [1].find(fail));
show("Array.prototype.map.call", () => Array.prototype.map.call([1], fail));
show("Array.from", () => Array.from([1], fail));
show("Uint8Array.prototype.map", () => new Uint8Array(1).map(fail));
show("new Map", () => new Map({ [Symbol.iterator]: fail }));
show("new Set", () => new Set({ [Symbol.iterator]: fail }));
show("Map.prototype.forEach", () => new Map([[1, 1]]).forEach(fail));
show("String.prototype.replace", () => "a".replace(/a/, fail));
show("String.prototype.replaceAll", () => "a".replaceAll("a", fail));
show("RegExp.prototype[Symbol.replace]", () => /a/[Symbol.replace]("a", fail));
show("Object.fromEntries", () => Object.fromEntries({ [Symbol.iterator]: fail }));
show("JSON.parse reviver", () => JSON.parse("[1]", fail));
show("JSON.parse syntax error", () => JSON.parse("{"));
show("Array.prototype.reduce, empty", () => [].reduce((a) => a));
show("Number.prototype.toFixed", () => (1).toFixed(1000));
show("Object.defineProperty", () => Object.defineProperty(1, "a", {}));
show("new Intl.NumberFormat", () => new Intl.NumberFormat("xx-invalid-locale-"));
show("Reflect.apply has no frame", () => Reflect.apply(fail, null, []));
show("Function.prototype.call has no frame", () => fail.call(null));
show("Function.prototype.apply has no frame", () => fail.apply(null, []));
show("bound function has no frame", () => fail.bind(null)());
show("new Array has no frame", () => new Array(-1));
show("a builtin called from a builtin", () => [[1]].map((a) => a.map(fail)));
show("a limited trace", () => {
  Error.stackTraceLimit = 2;
  try {
    [1].map(fail);
  } finally {
    Error.stackTraceLimit = 100;
  }
});

function Outer() {
  try {
    [1].forEach(function inner() {
      fail();
    });
  } catch (e) {
    console.log("## captureStackTrace\n" + frames(e));
    const target = {};
    [1].forEach(function capture() {
      Error.captureStackTrace(target);
    });
    console.log(frames(target));
    [1].forEach(function skipping() {
      Error.captureStackTrace(target, skipping);
    });
    console.log(frames(target));
  }
}
Outer();

new Promise(fail).catch((e) => console.log("## new Promise\n" + frames(e)));
