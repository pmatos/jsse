// V8 text of array patterns that fail to iterate (#1545), run verbatim on Node
// and on `jsse --node`: `.for` for a for-of head, and the declaration's
// initializer for an array pattern nested in an object pattern.
function run(name, fn) {
  try {
    fn();
    console.log(name, "-> ok");
  } catch (e) {
    console.log(name, "->", `${e.name}: ${e.message}`);
  }
}

const five = { a: 5 };
const empty = { a: null };
const o = { p: { a: 5 }, q: { a: null } };
function f(v) {
  return v;
}

run("for-of const [a]", () => { for (const [a] of [5]) {} });
run("for-of let [a]", () => { for (let [a] of [5]) {} });
run("for-of var [a]", () => { for (var [a] of [5]) {} });
run("for-of [a] null", () => { for (const [a] of [null]) {} });
run("for-of [a] undefined", () => { for (const [a] of [undefined]) {} });
run("for-of [a] object", () => { for (const [a] of [{}]) {} });
run("for-of [...r]", () => { for (const [...r] of [5]) {} });
run("for-of []", () => { for (const [] of [5]) {} });
run("for-of [a = 1]", () => { for (const [a = 1] of [5]) {} });
run("for-of [[a]]", () => { for (const [[a]] of [[5]]) {} });
run("for-of [[a]] null", () => { for (const [[a]] of [[null]]) {} });
run("for-of {a} null", () => { for (const { a } of [null]) {} });
run("for-of {a} undefined", () => { for (const { a } of [undefined]) {} });
run("for-of {} null", () => { for (const {} of [null]) {} });
run("for-of {...r} null", () => { for (const { ...r } of [null]) {} });
run("for-of {a: [b]}", () => { for (const { a: [b] } of [five]) {} });
run("for-of {a: [b]} null", () => { for (const { a: [b] } of [empty]) {} });
run("for-of var {a: [b]}", () => { for (var { a: [b] } of [five]) {} });
run("for-of {a: {c: [b]}}", () => { for (const { a: { c: [b] } } of [{ a: { c: 5 } }]) {} });
run("for-of [{b}]", () => { for (const [{ b }] of [[null]]) {} });
run("generator for-of", () => { (function* () { for (const [a] of [5]) { yield 1; } })().next(); });
run("generator for-of {a}", () => { (function* () { for (const { a } of [null]) { yield 1; } })().next(); });

run("var {a: [b]} = ident", () => { var { a: [b] } = five; });
run("var {a: [b]} = ident null", () => { var { a: [b] } = empty; });
run("var {a: [b]} = member", () => { var { a: [b] } = o.p; });
run("var {a: [b]} = member null", () => { var { a: [b] } = o.q; });
run("var {a: [b]} = call", () => { var { a: [b] } = f(five); });
run("var {a: [b]} = call null", () => { var { a: [b] } = f(empty); });
run("var {a: [b]} = literal", () => { var { a: [b] } = { a: 5 }; });
run("var {a: [b]} = literal null", () => { var { a: [b] } = { a: null }; });
run("var {a: [b]} = literal undefined", () => { var { a: [b] } = { a: undefined }; });
run("var {a: [b]} = literal object", () => { var { a: [b] } = { a: {} }; });
run("var {x, a: [b]} = literal", () => { var { x, a: [b] } = { x: 1, a: 5 }; });
run("var {x, a: [b]} = ident", () => { var { x, a: [b] } = five; });
run("var {a: [...b]} = ident", () => { var { a: [...b] } = five; });
run("let {a: [b]} = ident", () => { let { a: [b] } = five; });
run("const {a: [b]} = ident null", () => { const { a: [b] } = empty; });
run("var {[key]: [b]} = ident", () => { var { ["a"]: [b] } = five; });
run("var {a: {c: [b]}} = ident", () => { var { a: { c: [b] } } = { a: { c: 5 } }; });
run("var {a: {c: [b]}} = ident null", () => { var { a: { c: [b] } } = { a: { c: null } }; });
run("var [[a]] = [5]", () => { var [[a]] = [5]; });
run("var [[a]] = [null]", () => { var [[a]] = [null]; });
run("param {a: [b]}", () => { (function ({ a: [b] }) {})(five); });
run("param {a: [b]} null", () => { (function ({ a: [b] }) {})(empty); });
run("param [b]", () => { (function ([b]) {})(5); });
