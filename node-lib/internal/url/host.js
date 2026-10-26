'use strict';

// WHATWG URL host parsing (https://url.spec.whatwg.org/#host-parsing). The
// UTS #46 step is the native `__host_idna_to_ascii` primitive, which the
// builtin loader hands to `internal/url/idna` (see internal/url/domain).

const { encodeString, percentDecodeToString, sets } = require('internal/url/encode');
const { toASCII } = require('internal/url/domain');

const forbiddenHost = /[\0\t\n\r #/:<>?@[\\\]^|]/;
const forbiddenDomain = /[\0-\x1f #%/:<>?@[\\\]^|\x7f]/;
const digits = /^[0-9]+$/;

const radixDigits = { 8: /^[0-7]*$/, 10: /^[0-9]*$/, 16: /^[0-9a-fA-F]*$/ };

function parseIPv4Number(input) {
  if (input === '') return NaN;
  let radix = 10;
  if (input.length >= 2 && input[0] === '0' && (input[1] === 'x' || input[1] === 'X')) {
    input = input.slice(2);
    radix = 16;
  } else if (input.length >= 2 && input[0] === '0') {
    input = input.slice(1);
    radix = 8;
  }
  if (!radixDigits[radix].test(input)) return NaN;
  let value = 0;
  for (let i = 0; i < input.length && value < 2 ** 40; i++) {
    value = value * radix + parseInt(input[i], radix);
  }
  return value;
}

function endsInNumber(input) {
  const parts = input.split('.');
  if (parts[parts.length - 1] === '') {
    if (parts.length === 1) return false;
    parts.pop();
  }
  const last = parts[parts.length - 1];
  if (last !== '' && digits.test(last)) return true;
  return !Number.isNaN(parseIPv4Number(last));
}

function parseIPv4(input) {
  const parts = input.split('.');
  if (parts[parts.length - 1] === '' && parts.length > 1) parts.pop();
  if (parts.length > 4) return null;
  const numbers = [];
  for (let i = 0; i < parts.length; i++) {
    const n = parseIPv4Number(parts[i]);
    if (n !== n) return null;
    numbers.push(n);
  }
  for (let i = 0; i < numbers.length - 1; i++) {
    if (numbers[i] > 255) return null;
  }
  const last = numbers[numbers.length - 1];
  if (last >= 256 ** (5 - numbers.length)) return null;
  let ipv4 = last;
  for (let i = 0; i < numbers.length - 1; i++) {
    ipv4 += numbers[i] * 256 ** (3 - i);
  }
  return [ipv4 >>> 24, (ipv4 >>> 16) & 255, (ipv4 >>> 8) & 255, ipv4 & 255].join('.');
}

const hexDigit = /^[0-9a-fA-F]$/;
const decDigit = /^[0-9]$/;

function parseIPv6(input) {
  const address = [0, 0, 0, 0, 0, 0, 0, 0];
  let pieceIndex = 0;
  let compress = null;
  let p = 0;
  const at = (i) => (i < input.length ? input[i] : '');
  if (at(p) === ':') {
    if (at(p + 1) !== ':') return null;
    p += 2;
    pieceIndex++;
    compress = pieceIndex;
  }
  while (at(p) !== '') {
    if (pieceIndex === 8) return null;
    if (at(p) === ':') {
      if (compress !== null) return null;
      p++;
      pieceIndex++;
      compress = pieceIndex;
      continue;
    }
    let value = 0;
    let length = 0;
    while (length < 4 && hexDigit.test(at(p))) {
      value = value * 16 + parseInt(at(p), 16);
      p++;
      length++;
    }
    if (at(p) === '.') {
      if (length === 0) return null;
      p -= length;
      if (pieceIndex > 6) return null;
      let numbersSeen = 0;
      while (at(p) !== '') {
        let ipv4Piece = null;
        if (numbersSeen > 0) {
          if (at(p) === '.' && numbersSeen < 4) p++;
          else return null;
        }
        if (!decDigit.test(at(p))) return null;
        while (decDigit.test(at(p))) {
          const number = parseInt(at(p), 10);
          if (ipv4Piece === null) ipv4Piece = number;
          else if (ipv4Piece === 0) return null;
          else ipv4Piece = ipv4Piece * 10 + number;
          if (ipv4Piece > 255) return null;
          p++;
        }
        address[pieceIndex] = address[pieceIndex] * 256 + ipv4Piece;
        numbersSeen++;
        if (numbersSeen === 2 || numbersSeen === 4) pieceIndex++;
      }
      if (numbersSeen !== 4) return null;
      break;
    } else if (at(p) === ':') {
      p++;
      if (at(p) === '') return null;
    } else if (at(p) !== '') {
      return null;
    }
    address[pieceIndex] = value;
    pieceIndex++;
  }
  if (compress !== null) {
    let swaps = pieceIndex - compress;
    pieceIndex = 7;
    while (pieceIndex !== 0 && swaps > 0) {
      const tmp = address[compress + swaps - 1];
      address[compress + swaps - 1] = address[pieceIndex];
      address[pieceIndex] = tmp;
      pieceIndex--;
      swaps--;
    }
  } else if (pieceIndex !== 8) {
    return null;
  }
  return address;
}

function serializeIPv6(address) {
  let best = -1;
  let bestLen = 1;
  for (let i = 0; i < 8; ) {
    if (address[i] !== 0) {
      i++;
      continue;
    }
    let j = i;
    while (j < 8 && address[j] === 0) j++;
    if (j - i > bestLen) {
      best = i;
      bestLen = j - i;
    }
    i = j;
  }
  let out = '';
  let ignore0 = false;
  for (let i = 0; i < 8; i++) {
    if (ignore0 && address[i] === 0) continue;
    ignore0 = false;
    if (best === i) {
      out += i === 0 ? '::' : ':';
      ignore0 = true;
      continue;
    }
    out += address[i].toString(16);
    if (i !== 7) out += ':';
  }
  return out;
}

function parseHost(input, isOpaque) {
  if (input[0] === '[') {
    if (input[input.length - 1] !== ']') return null;
    const address = parseIPv6(input.slice(1, -1));
    return address === null ? null : '[' + serializeIPv6(address) + ']';
  }
  if (isOpaque) {
    if (forbiddenHost.test(input)) return null;
    return encodeString(input, sets.c0);
  }
  const domain = input.indexOf('%') === -1 ? input : percentDecodeToString(input);
  const ascii = toASCII(domain);
  if (ascii === undefined || ascii === '' || forbiddenDomain.test(ascii)) return null;
  if (endsInNumber(ascii)) return parseIPv4(ascii);
  return ascii;
}

module.exports = { parseHost };
