// Regex matching
var s = "the quick brown fox jumps over the lazy dog ".repeat(1000);
var re = /[a-z]+/g;
var count = 0;
var m;
while ((m = re.exec(s)) !== null) {
  count++;
}
console.log(count);
