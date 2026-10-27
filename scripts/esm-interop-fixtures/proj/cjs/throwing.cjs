var holder = { get x() { throw new Error('boom'); } };
Object.defineProperty(exports, 't', { enumerable: true, get: function () { return holder.x; } });
exports.ok = 'ok';
