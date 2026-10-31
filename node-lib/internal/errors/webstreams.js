'use strict';

// Error codes the web streams use that internal/errors does not define, and
// the `.TypeError`/`.RangeError` variants of two it does; kept apart so
// internal/errors stays under the file budget.
module.exports = function defineWebStreamsErrors(E, codes) {
  E('ERR_MISSING_OPTION', TypeError, (name) => `${name} is required`);

  // `new ERR_INVALID_STATE.TypeError(...)`: the same code and message with a
  // different base class.
  function addVariant(code, Base) {
    const Original = codes[code];
    E(code, Base, (...args) => new Original(...args).message);
    Original[Base.name] = codes[code];
    codes[code] = Original;
  }
  addVariant('ERR_INVALID_ARG_VALUE', RangeError);
  addVariant('ERR_INVALID_STATE', TypeError);
  addVariant('ERR_INVALID_STATE', RangeError);
};
