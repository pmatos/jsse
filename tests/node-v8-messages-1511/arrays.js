// V8 error positions of array patterns, parameter lists, comma heritage and
// lone spread arguments (#1511 stage 4), run verbatim on Node and on
// `jsse --node` by scripts/run-node-v8-messages-1511-oracle.sh.
const num = 5;
const u = undefined;
function f() {}

function show(name, fn, frames = 1) {
  try {
    fn();
    console.log("## " + name + ": no error");
  } catch (e) {
    console.log("## " + name);
    console.log(e.name + ": " + e.message);
    for (const line of String(e.stack).split("\n").slice(1, 1 + frames)) {
      console.log(line.trim());
    }
  }
}

// An array pattern parameter is reported at the `(` of the parameter list.
show("parameterPattern", () => {
  (function ([a]) {})(undefined);
});
show("parameterPatternAfterName", () => {
  (function (x, [a]) {})(1, undefined);
});
show("arrowParameterPattern", () => {
  (([a]) => 0)(undefined);
});
show("methodParameterPattern", () => {
  ({ m([a]) {} }).m(undefined);
});
show("nestedParameterPattern", () => {
  (function ([[a]]) {})([null]);
});
show("generatorParameterPattern", () => {
  (function* ([a]) {})(undefined);
});

// Any other array pattern is reported at its `[`; a declaration's own
// pattern at the initializer.
show("declarationPattern", () => {
  let [a] = undefined;
});
show("nestedDeclarationPattern", () => {
  var [[t]] = [null];
});
show("propertyDeclarationPattern", () => {
  var { s: [t] } = { s: null };
});
show("defaultedDeclarationPattern", () => {
  var { s: [t] = null } = {};
});
show("elementDeclarationPattern", () => {
  var [{ a }] = [null];
});

// A parenthesised comma expression is reported at its last operand.
show("commaHeritage", () => {
  class B extends (1, num) {}
});
show("commaHeritageOfCalls", () => {
  class B extends (f(), 1) {}
});
show("parenthesisedHeritage", () => {
  class B extends (num) {}
});
show("mathHeritage", () => {
  class B extends Math {}
});

// A lone spread argument is reported at the call.
show("loneCallSpread", () => {
  f(...5);
});
show("loneNewSpread", () => {
  new f(...5);
});
