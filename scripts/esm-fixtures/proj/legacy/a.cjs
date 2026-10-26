globalThis.log.push('legacy/a.cjs');
exports.name = 'legacy-a';
exports.dynamicSibling = () => import('./sibling.mjs');
exports.dynamicLater = () => new Promise((resolve) => setTimeout(() => resolve(import('./sibling.mjs')), 1));
exports.dynamicEsmAbove = () => import('../src/dyn.js');
exports.self = () => module.exports === exports;
