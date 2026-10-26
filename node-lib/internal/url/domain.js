'use strict';

// UTS #46 ToASCII as ada applies it: an all-ASCII name is only lowercased
// (labels starting with "xn--" included), anything else goes to the native
// primitive. `toASCII` returns undefined on failure.

const idna = require('internal/url/idna');

const asciiOnly = /^[\0-\x7f]*$/;

function toASCII(domain) {
  return asciiOnly.test(domain) ? domain.toLowerCase() : idna.toASCII(domain);
}

module.exports = { toASCII, toUnicode: idna.toUnicode };
