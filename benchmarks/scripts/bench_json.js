// JSON parse and stringify
var data = [];
for (var i = 0; i < 10000; i++) {
  data.push({ id: i, name: "item" + i, value: i * 1.5 });
}
var str = JSON.stringify(data);
var parsed = JSON.parse(str);
console.log(parsed.length);
