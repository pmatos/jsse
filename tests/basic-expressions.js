// Basic expression evaluation tests

var x = 2 + 3;
if (x !== 5) throw "2 + 3 should be 5";

var y = 10 - 4;
if (y !== 6) throw "10 - 4 should be 6";

var z = 3 * 7;
if (z !== 21) throw "3 * 7 should be 21";

var w = 15 / 3;
if (w !== 5) throw "15 / 3 should be 5";

var m = 17 % 5;
if (m !== 2) throw "17 % 5 should be 2";

// String concatenation
var s = "hello" + " " + "world";
if (s !== "hello world") throw "string concat failed";

// Boolean logic
if (!(true && true)) throw "&& failed";
if (true && false) throw "&& false case failed";
if (!true || !false) {
    // ok
} else {
    throw "|| failed";
}

// Comparison
if (!(1 < 2)) throw "1 < 2 failed";
if (!(2 > 1)) throw "2 > 1 failed";
if (!(1 <= 1)) throw "1 <= 1 failed";
if (!(1 >= 1)) throw "1 >= 1 failed";

// Typeof
if (typeof undefined !== "undefined") throw "typeof undefined failed";
if (typeof null !== "object") throw "typeof null failed";
if (typeof true !== "boolean") throw "typeof true failed";
if (typeof 42 !== "number") throw "typeof 42 failed";
if (typeof "hi" !== "string") throw "typeof string failed";
