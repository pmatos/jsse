'use strict';

// The streaming UTF-8 decoder readline needs, over the global TextDecoder, so
// that readline does not depend on the string_decoder module.

class StringDecoder {
  constructor(encoding) {
    this.encoding = encoding;
    this._decoder = new TextDecoder('utf-8');
  }

  write(chunk) {
    if (typeof chunk === 'string') return chunk;
    return this._decoder.decode(chunk, { stream: true });
  }

  end(chunk) {
    const head = chunk === undefined ? '' : this.write(chunk);
    return head + this._decoder.decode();
  }
}

module.exports = { StringDecoder };
