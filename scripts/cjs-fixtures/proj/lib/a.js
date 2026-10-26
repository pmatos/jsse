exports.early = 'a-early';
const b = require('./b');
exports.sawB = b.snapshot;
exports.done = true;
