'use strict';

// The error codes only zlib raises, registered on internal/errors' `codes`
// (v26.8.2 messages).

const { codes, E } = require('internal/errors');

if (codes.ERR_BUFFER_TOO_LARGE === undefined) {
  E('ERR_BUFFER_TOO_LARGE', RangeError, (x) => `Cannot create a Buffer larger than ${x} bytes`);
  E('ERR_TRAILING_JUNK_AFTER_STREAM_END', TypeError,
    'Trailing junk found after the end of the compressed stream');
  E('ERR_BROTLI_INVALID_PARAM', RangeError, (x) => `${x} is not a valid Brotli parameter`);
  E('ERR_ZLIB_INITIALIZATION_FAILED', Error, 'Initialization failed');
}
