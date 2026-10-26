
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
  out("== " + scenarios[i][0]);
  var finished = false;
  function next() { if (!finished) { finished = true; setImmediate(run, i + 1); } }
  var timer = setTimeout(function () { out("TIMEOUT", scenarios[i][0]); next(); }, 3000);
  Promise.resolve().then(scenarios[i][1]).then(function () {
    return new Promise(function (resolve) { setImmediate(resolve); });
  }).then(function () { clearTimeout(timer); next(); }, function (err) {
    out("scenario failed", err && err.stack || err);
    clearTimeout(timer);
    next();
  });
})(0);
