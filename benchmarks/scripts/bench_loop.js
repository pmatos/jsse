// Tight loop — tests raw interpreter overhead
var sum = 0;
for (var i = 0; i < 1000000; i++) {
  sum += i;
}
console.log(sum);
