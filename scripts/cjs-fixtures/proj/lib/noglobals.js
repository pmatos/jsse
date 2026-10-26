var topLevelVar = 1;
function topLevelFn() {}
module.exports = [typeof globalThis.topLevelVar, typeof globalThis.topLevelFn, typeof topLevelVar];
