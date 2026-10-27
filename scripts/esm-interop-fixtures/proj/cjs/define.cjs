var src = { y: 'y-value', z: 'z-value' };
Object.defineProperty(exports, 'val', { value: 1 });
Object.defineProperty(exports, 'enumVal', { enumerable: true, value: 2 });
Object.defineProperty(exports, 'getFn', { enumerable: true, get: function () { return src.y; } });
Object.defineProperty(exports, 'getShort', { enumerable: true, get() { return src.z; } });
Object.defineProperty(exports, 'getIndex', { enumerable: true, get: function () { return src['y']; } });
Object.defineProperty(exports, 'getCall', { enumerable: true, get: function () { return compute(); } });
Object.defineProperty(exports, 'plainGet', { get: function get() { return src.y; } });
Object.defineProperty(module.exports, 'viaModule', { enumerable: true, value: 3 });
Object.defineProperty(exports, '__esModule', { value: true });
function compute() { return 'computed'; }
