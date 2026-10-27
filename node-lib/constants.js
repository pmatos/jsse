'use strict';

// The deprecated `constants` module: the os dlopen/errno/priority/signals
// tables, then fs constants, in the order Node merges them. Node appends the
// crypto constants; the embedded crypto module defines none yet.

const {
  ObjectAssign,
  ObjectFreeze,
} = primordials;

const { constants: osConstants } = require('os');
const { constants: fsConstants } = require('fs');

module.exports = ObjectFreeze(ObjectAssign(
  {},
  osConstants.dlopen,
  osConstants.errno,
  osConstants.priority,
  osConstants.signals,
  fsConstants,
));
