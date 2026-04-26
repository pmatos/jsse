// Function.prototype.toString must preserve source text for user functions.
// Spec: ECMAScript 2024, sec-function.prototype.tostring

var filler = " ";
for (var i = 0; i < 17; i++) {
    filler = filler + filler;
}

var functionSource = "function largeSourceFunction() {" + filler + "return 123;}";
var fn = eval("(" + functionSource + ")");

if (fn.toString() !== functionSource) {
    throw "large function source text was not preserved";
}

if (fn() !== 123) {
    throw "large function did not execute correctly";
}

var classSource = "class LargeSourceClass {" + filler + "method(){return 456;}}";
var C = eval("(" + classSource + ")");

if (C.toString() !== classSource) {
    throw "large class source text was not preserved";
}

if ((new C()).method() !== 456) {
    throw "large class did not execute correctly";
}
