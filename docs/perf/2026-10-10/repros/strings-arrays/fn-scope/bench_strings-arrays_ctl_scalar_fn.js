// CONTROL: same loop inside a function (local variables). Shows jsse's baseline cost of local-variable access/arithmetic relative to boa; every function-scope repro includes this overhead.
// Deterministic; one checksum line.
(function () {
var s = 0, a = 1;
for (var i = 0; i < 3000000; i++) { s = (s + i * a) | 0; a = (a ^ i) & 1023; }
console.log(s, a);
})();
