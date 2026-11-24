const m = new Map([[1, "a"], ["1", "b"], [NaN, "nan"]]);
m.set(-0, "zero").delete("1");
show([...m], m.get(NaN), m.get(0), m.size, [...m.keys()]);
const s = new Set([3, 1, 3, 2, 1]);
s.add(0);
show([...s], s.has(2), [...s.entries()][0]);
show([...new Set([1, 2, 3]).union(new Set([4]))], [...new Set([1, 2, 3]).intersection(new Set([2, 3, 9]))]);
show(new Set([1]).isSubsetOf(new Set([1, 2])), [...new Set([1, 2]).symmetricDifference(new Set([2, 3]))]);
const order = new Map();
order.set("a", 1).set("b", 2);
for (const [k] of order) { if (k === "a") { order.delete("b"); order.set("c", 3); } show("visit", k); }
const wm = new WeakMap();
const key = {};
wm.set(key, "weak");
show(wm.get(key), wm.has({}), new WeakSet([key]).has(key));
show(Map.groupBy([1, 2, 3], (x) => x % 2).get(1), Object.prototype.toString.call(m));
