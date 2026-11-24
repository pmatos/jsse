const tagged = { [Symbol.toStringTag]: "Custom" };
show(String(tagged), Symbol.for("k") === Symbol.for("k"), Symbol.keyFor(Symbol.for("k")), Symbol("d").description);
const prim = { [Symbol.toPrimitive](hint) { return hint === "number" ? 42 : "str:" + hint; } };
show(+prim, `${prim}`, prim + "", prim * 2);
const valueOf = { valueOf: () => 7, toString: () => "seven" };
show(valueOf + 1, `${valueOf}`, [valueOf] + "", valueOf > 6);
class Even { static [Symbol.hasInstance](n) { return n % 2 === 0; } }
show(2 instanceof Even, 3 instanceof Even);
const spreadable = { length: 2, 0: "x", 1: "y", [Symbol.isConcatSpreadable]: true };
show([1].concat(spreadable), Array.prototype[Symbol.unscopables].flat);
show(typeof Symbol.iterator, Object.getOwnPropertySymbols({ [Symbol.iterator]: 1 }).length);
