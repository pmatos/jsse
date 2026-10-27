var k = 'x';
exports[k] = 1;
Object.assign(exports, { y: 2 });
function later() { exports.inFunction = 3; }
if (k) { exports.inIf = 4; }
