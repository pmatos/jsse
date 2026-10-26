// RUN: 9
process.on("exit", function (code) { console.log("exit " + code + " " + process.exitCode); });
process.exitCode = 4;
setTimeout(function () { process.exit(9); }, 1);
