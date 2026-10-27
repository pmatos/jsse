'use strict';

// The deprecated `constants` module: the os dlopen/errno/priority/signals
// tables, then fs and crypto constants, in the order Node merges them. A
// table an embedded module does not define yet (crypto.constants) is skipped.

const {
  ObjectAssign,
  ObjectFreeze,
} = primordials;

const { constants: osConstants } = require('os');
const { constants: fsConstants } = require('fs');
const { constants: cryptoConstants } = require('crypto');

module.exports = ObjectFreeze(ObjectAssign(
  {},
  osConstants.dlopen,
  osConstants.errno,
  osConstants.priority,
  osConstants.signals,
  fsConstants,
  cryptoConstants,
));
