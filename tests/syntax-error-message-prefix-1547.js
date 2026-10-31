// A SyntaxError thrown for source text that fails to parse (PerformEval,
// CreateDynamicFunction, ShadowRealm evaluation, $262.evalScript) carries a
// message that is implementation-defined, but the error's name already says
// "SyntaxError": the message must not repeat it as a prefix (#1547).

function check(label, run) {
  var error;
  try {
    run();
  } catch (caught) {
    error = caught;
  }
  if (!(error instanceof SyntaxError)) {
    throw new Error(label + ": expected a SyntaxError, got " + error);
  }
  if (typeof error.message !== "string" || error.message === "") {
    throw new Error(label + ": expected a non-empty message");
  }
  if (/^SyntaxError:/.test(error.message)) {
    throw new Error(label + ": message repeats the error name: " + error.message);
  }
  if (String(error).indexOf("SyntaxError: SyntaxError") !== -1) {
    throw new Error(label + ": doubled prefix in " + String(error));
  }
}

var invalid = "var = 1";
var AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
var GeneratorFunction = Object.getPrototypeOf(function* () {}).constructor;
var AsyncGeneratorFunction = Object.getPrototypeOf(async function* () {}).constructor;

check("direct eval", function () { eval(invalid); });
check("indirect eval", function () { (0, eval)(invalid); });
check("Function", function () { new Function(invalid); });
check("Function parameters", function () { new Function("a b", ""); });
check("AsyncFunction", function () { new AsyncFunction(invalid); });
check("GeneratorFunction", function () { new GeneratorFunction(invalid); });
check("AsyncGeneratorFunction", function () { new AsyncGeneratorFunction(invalid); });
if (typeof $262 !== "undefined" && typeof $262.evalScript === "function") {
  check("$262.evalScript", function () { $262.evalScript(invalid); });
}
if (typeof ShadowRealm === "function") {
  check("ShadowRealm.evaluate", function () { new ShadowRealm().evaluate(invalid); });
}
