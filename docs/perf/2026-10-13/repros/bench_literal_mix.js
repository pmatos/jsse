// splay-shaped node construction: nested object and array literals, a
// regular expression literal and a method closure per node.
function node(key, depth) {
  if (depth === 0) {
    return { key: key, payload: [key, key + 1], re: /a+/ };
  }
  return { key: key, left: node(key * 2, depth - 1), right: node(key * 2 + 1, depth - 1),
           tag: function () { return key; } };
}
function main() {
  var n = 0;
  for (var i = 0; i < 300; i++) n += node(i, 8).key;
  return n;
}
console.log(main());
