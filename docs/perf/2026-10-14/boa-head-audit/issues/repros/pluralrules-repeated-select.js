function main(n) {
  var rules = new Intl.PluralRules("fr-CA");
  var sum = 0;
  for (var i = 0; i < n; i++) sum += rules.select(i % 1000).length;
  return sum;
}
console.log(main(500000));
