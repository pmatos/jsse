const a = require('./a');
exports.snapshot = { aEarly: a.early, aDone: a.done, aLoaded: module.parent.loaded };
