'use strict';

// No workers or message ports (#1528): the transfer symbols exist so the class
// bodies read as upstream.

const { Symbol } = primordials;

module.exports = {
  kDeserialize: Symbol('kDeserialize'),
  kTransfer: Symbol('kTransfer'),
  kTransferList: Symbol('kTransferList'),
  markTransferMode() {},
  structuredClone: require('internal/host/webstreams').structuredClone,
};
