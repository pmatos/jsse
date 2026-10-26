// RUN: 1
// ERR: just a string
setTimeout(function () { throw "just a string"; }, 1);
