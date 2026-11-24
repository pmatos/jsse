const { a = 1, b: { c } = { c: 2 }, ...others } = { x: 1, y: 2 };
show(a, c, others);
const [d, , e = "default", ...tail] = [1, 2, undefined, 4, 5];
show(d, e, tail);
function params(p, { q = 2 } = {}, ...r) { return [p, q, r, arguments.length]; }
show(params(1), params(1, { q: 3 }, 4, 5), params.length);
let s1 = 1, s2 = 2;
[s1, s2] = [s2, s1];
show(s1, s2);
const getterOrder = [];
const src = { get first() { getterOrder.push("first"); return 1; }, get second() { getterOrder.push("second"); return 2; } };
const { second, first } = src;
show(first, second, getterOrder);
function defaults(x, y = x * 2, z = () => x + y) { x = 10; return [x, y, z()]; }
show(defaults(1));
