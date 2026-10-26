(function run(i) {
  if (i === scenarios.length) {
    var digest = 0x811c9dc5;
    var text = lines.join("\n");
    for (var k = 0; k < text.length; k++) {
      digest = Math.imul(digest ^ text.charCodeAt(k), 0x01000193) >>> 0;
    }
    console.log(text);
    console.log("digest " + digest.toString(16));
    process.exit(0);
  }
  var s = scenarios[i];
  Promise.resolve().then(s[1]).then(function (v) {
    report(s[0], v, undefined, false);
  }, function (err) {
    report(s[0], undefined, err, true);
  }).then(function () { run(i + 1); });
})(0);
