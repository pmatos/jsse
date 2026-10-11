// CONTROL: scalar arithmetic loop on global variables, no builtins. Shows the baseline jsse/boa ratio of plain variable access + arithmetic at global scope.
// Deterministic; one checksum line.
var s = 0, a = 1;
for (var i = 0; i < 3000000; i++) { s = (s + i * a) | 0; a = (a ^ i) & 1023; }
console.log(s, a);
