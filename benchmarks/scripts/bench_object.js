// Object property access and creation
var obj = {};
for (var i = 0; i < 100000; i++) {
  obj["key" + i] = i;
}
var sum = 0;
for (var k in obj) {
  sum += obj[k];
}
console.log(sum);
