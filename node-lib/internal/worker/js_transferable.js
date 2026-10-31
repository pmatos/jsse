'use strict';

// jsse has no workers or message ports, so web streams are never transferred:
// the symbols exist so the class bodies read as upstream, and the mark is a
// no-op. `structuredClone` is the engine's, used by `ReadableStream#tee`'s
// clone mode.

const { SymbolFor } = primordials;

module.exports = {
  kDeserialize: SymbolFor('jsse.webstreams.deserialize'),
  kTransfer: SymbolFor('jsse.webstreams.transfer'),
  kTransferList: SymbolFor('jsse.webstreams.transferList'),
  markTransferMode() {},
  structuredClone: require('internal/host/webstreams').structuredClone,
};
