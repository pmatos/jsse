'use strict';

// TextEncoder and TextDecoder are the engine's own (the web-stream encoders,
// `stream/consumers` and the stream adapters take them from here).
module.exports = require('internal/host/encoding');
