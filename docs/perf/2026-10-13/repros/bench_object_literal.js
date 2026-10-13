// Statically keyed object literals in a hot function: NewObject.
function mk(i) {
  return { x: i, y: i + 1, z: 0, name: "p" };
}
function main() {
  var s = 0;
  for (var i = 0; i < 1000000; i++) {
    var p = mk(i);
    s += p.x + p.y;
  }
  return s;
}
console.log(main());
