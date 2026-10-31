'use strict';

// Placeholder: only the web-stream transfer paths construct one (#1528).

const { codes: { ERR_METHOD_NOT_IMPLEMENTED } } = require('internal/errors');

class MessageChannel {
  constructor() {
    throw new ERR_METHOD_NOT_IMPLEMENTED('MessageChannel');
  }
}

module.exports = { MessageChannel };
