// Only error names and messages we control are compared; engine-specific
// messages (e.g. "x is not a function") are deliberately not printed.
const cases = [
  () => null.prop,
  () => undefinedVariable,
  () => { "use strict"; Object.defineProperty({}, "x", { value: 1 }).x = 2; },
  () => new Array(-1),
  () => (1).toFixed(101),
  () => BigInt(1.5),
  () => Symbol() + "",
  () => { class A { constructor() { this.x; } } class B extends A { constructor() { this.y = 1; super(); } } new B(); },
  () => { throw new AggregateError([new Error("inner")], "agg"); },
  () => decodeURIComponent("%"),
  () => eval("let let = 1"),
];
for (const c of cases) {
  try { c(); show("no throw"); } catch (e) { show(e.constructor.name, e instanceof Error); }
}
const err = new Error("outer", { cause: "the cause" });
show(err.message, err.cause, String(err), Object.prototype.toString.call(err), "stack" in err);
class MyError extends Error { constructor(m) { super(m); this.name = "MyError"; } }
show(String(new MyError("custom")), new MyError("x") instanceof Error, new RangeError().message === "");
