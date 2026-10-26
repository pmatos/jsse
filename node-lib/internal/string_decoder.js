'use strict';

// Port of the native `internalBinding('string_decoder')` (Node v26.8.2
// src/string_decoder.cc, MIT) over the prelude's Buffer. The decoder state is
// the same seven-byte layout: four bytes of incomplete character, then the
// missing and buffered byte counts and the encoding.

const { Buffer } = require('internal/host/buffer');

const encodings = [
  'ascii', 'utf8', 'base64', 'utf16le', 'latin1', 'hex', 'buffer', 'base64url',
];
const ASCII = 0;
const UTF8 = 1;
const BASE64 = 2;
const UCS2 = 3;
const BASE64URL = 7;

const kIncompleteCharactersStart = 0;
const kIncompleteCharactersEnd = 4;
const kMissingBytes = 4;
const kBufferedBytes = 5;
const kEncodingField = 6;
const kNumFields = 7;

function makeString(bytes, start, end, encoding) {
  if (end <= start) return '';
  return Buffer.from(bytes.buffer, bytes.byteOffset + start, end - start)
    .toString(encodings[encoding]);
}

function copyInto(state, at, bytes, start, end) {
  for (let i = start; i < end; i++) state[at + i - start] = bytes[i];
}

function decode(state, buf) {
  const bytes = buf instanceof Uint8Array ?
    buf : new Uint8Array(buf.buffer, buf.byteOffset, buf.byteLength);
  const encoding = state[kEncodingField];
  let pos = 0;
  let nread = bytes.length;

  if (encoding !== UTF8 && encoding !== UCS2 &&
      encoding !== BASE64 && encoding !== BASE64URL) {
    return makeString(bytes, 0, nread, encoding);
  }

  let prepend = '';
  if (state[kMissingBytes] > 0) {
    if (encoding === UTF8) {
      for (let i = 0; i < nread && i < state[kMissingBytes]; ++i) {
        if ((bytes[pos + i] & 0xC0) !== 0x80) {
          state[kMissingBytes] = 0;
          copyInto(state, state[kBufferedBytes], bytes, pos, pos + i);
          state[kBufferedBytes] += i;
          pos += i;
          nread -= i;
          break;
        }
      }
    }

    const found = nread < state[kMissingBytes] ? nread : state[kMissingBytes];
    copyInto(state, state[kBufferedBytes], bytes, pos, pos + found);
    pos += found;
    nread -= found;
    state[kMissingBytes] -= found;
    state[kBufferedBytes] += found;

    if (state[kMissingBytes] === 0) {
      prepend = makeString(state, 0, state[kBufferedBytes], encoding);
      state[kBufferedBytes] = 0;
    }
  }

  if (nread === 0) return prepend;

  if (encoding === UTF8 && (bytes[pos + nread - 1] & 0x80) !== 0) {
    for (let i = nread - 1; ; --i) {
      state[kBufferedBytes]++;
      if ((bytes[pos + i] & 0xC0) === 0x80) {
        if (state[kBufferedBytes] >= 4 || i === 0) {
          state[kBufferedBytes] = 0;
          break;
        }
      } else {
        const lead = bytes[pos + i];
        if ((lead & 0xE0) === 0xC0) {
          state[kMissingBytes] = 2;
        } else if ((lead & 0xF0) === 0xE0) {
          state[kMissingBytes] = 3;
        } else if ((lead & 0xF8) === 0xF0) {
          state[kMissingBytes] = 4;
        } else {
          state[kBufferedBytes] = 0;
          break;
        }
        if (state[kBufferedBytes] >= state[kMissingBytes]) {
          state[kMissingBytes] = 0;
          state[kBufferedBytes] = 0;
        }
        state[kMissingBytes] -= state[kBufferedBytes];
        break;
      }
    }
  } else if (encoding === UCS2) {
    if ((nread % 2) === 1) {
      state[kBufferedBytes] = 1;
      state[kMissingBytes] = 1;
    } else if ((bytes[pos + nread - 1] & 0xFC) === 0xD8) {
      state[kBufferedBytes] = 2;
      state[kMissingBytes] = 2;
    }
  } else if (encoding === BASE64 || encoding === BASE64URL) {
    state[kBufferedBytes] = nread % 3;
    if (state[kBufferedBytes] > 0) {
      state[kMissingBytes] = 3 - state[kBufferedBytes];
    }
  }

  if (state[kBufferedBytes] > 0) {
    nread -= state[kBufferedBytes];
    copyInto(state, 0, bytes, pos + nread, pos + nread + state[kBufferedBytes]);
  }

  return prepend + makeString(bytes, pos, pos + nread, encoding);
}

function flush(state) {
  const encoding = state[kEncodingField];
  if (encoding === UCS2 && state[kBufferedBytes] % 2 === 1) {
    state[kMissingBytes]--;
    state[kBufferedBytes]--;
  }
  if (state[kBufferedBytes] === 0) return '';
  const ret = makeString(state, 0, state[kBufferedBytes], encoding);
  state[kMissingBytes] = 0;
  state[kBufferedBytes] = 0;
  return ret;
}

// lib/internal/util.js normalizeEncoding.
function normalizeEncoding(enc) {
  if (enc == null || enc === 'utf8' || enc === 'utf-8') return 'utf8';
  switch (enc.length) {
    case 4:
      if (enc === 'UTF8') return 'utf8';
      if (enc === 'ucs2' || enc === 'UCS2') return 'utf16le';
      enc = `${enc}`.toLowerCase();
      if (enc === 'utf8') return 'utf8';
      if (enc === 'ucs2') return 'utf16le';
      break;
    case 3:
      if (enc === 'hex' || enc === 'HEX' ||
          `${enc}`.toLowerCase() === 'hex') {
        return 'hex';
      }
      break;
    case 5:
      if (enc === 'ascii') return 'ascii';
      if (enc === 'ucs-2') return 'utf16le';
      if (enc === 'UTF-8') return 'utf8';
      if (enc === 'ASCII') return 'ascii';
      if (enc === 'UCS-2') return 'utf16le';
      enc = `${enc}`.toLowerCase();
      if (enc === 'utf-8') return 'utf8';
      if (enc === 'ascii') return 'ascii';
      if (enc === 'ucs-2') return 'utf16le';
      break;
    case 6:
      if (enc === 'base64') return 'base64';
      if (enc === 'latin1' || enc === 'binary') return 'latin1';
      if (enc === 'BASE64') return 'base64';
      if (enc === 'LATIN1' || enc === 'BINARY') return 'latin1';
      enc = `${enc}`.toLowerCase();
      if (enc === 'base64') return 'base64';
      if (enc === 'latin1' || enc === 'binary') return 'latin1';
      break;
    case 7:
      if (enc === 'utf16le' || enc === 'UTF16LE' ||
          `${enc}`.toLowerCase() === 'utf16le') {
        return 'utf16le';
      }
      break;
    case 8:
      if (enc === 'utf-16le' || enc === 'UTF-16LE' ||
          `${enc}`.toLowerCase() === 'utf-16le') {
        return 'utf16le';
      }
      break;
    case 9:
      if (enc === 'base64url' || enc === 'BASE64URL' ||
          `${enc}`.toLowerCase() === 'base64url') {
        return 'base64url';
      }
      break;
    default:
      if (enc === '') return 'utf8';
  }
}

module.exports = {
  encodings,
  kIncompleteCharactersStart,
  kIncompleteCharactersEnd,
  kMissingBytes,
  kBufferedBytes,
  kEncodingField,
  kNumFields,
  kSize: kNumFields,
  decode,
  flush,
  normalizeEncoding,
};
