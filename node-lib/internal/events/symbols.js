'use strict';

const { Symbol, SymbolFor } = primordials;

module.exports = {
  kFirstEventParam: Symbol('kFirstEventParam'),
  kHybridDispatch: SymbolFor('nodejs.internal.kHybridDispatch'),
  kIsNodeEventTarget: Symbol('kIsNodeEventTarget'),
  kIsNodeStyleListener: Symbol('kIsNodeStyleListener'),
};
