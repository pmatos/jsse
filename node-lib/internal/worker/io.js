'use strict';

// Only the web-stream transfer paths construct a MessageChannel, and nothing
// reaches them without a message port, so this is a placeholder.

const { codes: { ERR_METHOD_NOT_IMPLEMENTED } } = require('internal/errors');

class MessageChannel {
  constructor() {
    throw new ERR_METHOD_NOT_IMPLEMENTED('MessageChannel');
  }
}

module.exports = { MessageChannel };
