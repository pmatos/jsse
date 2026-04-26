// String concatenation and manipulation
var s = "";
for (var i = 0; i < 100000; i++) {
  s += "x";
}
console.log(s.length);
console.log(s.indexOf("x"));
console.log(s.slice(0, 10).length);
