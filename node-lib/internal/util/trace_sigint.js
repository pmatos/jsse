'use strict';

const { validateBoolean } = require('internal/validators');

// jsse has no SIGINT watchdog thread to arm, so enabling is accepted and does nothing.
function setTraceSigInt(enable) {
  validateBoolean(enable, 'enable');
}

module.exports = {
  setTraceSigInt,
};
