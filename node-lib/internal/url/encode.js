'use strict';

// Percent-encode sets and UTF-8 helpers for the WHATWG URL parser
// (https://url.spec.whatwg.org/#percent-encoded-bytes).

const {
  ArrayPrototypeSlice,
  StringFromCharCode,
  StringFromCodePoint,
} = primordials;

function makeSet(parent, extra) {
  const table = parent ? ArrayPrototypeSlice(parent) : [];
  if (!parent) for (let i = 0; i < 128; i++) table[i] = false;
  for (let i = 0; i < extra.length; i++) table[extra.charCodeAt(i)] = true;
  return table;
}

const c0 = makeSet(null, '\x7f');
for (let i = 0; i < 0x20; i++) c0[i] = true;
const fragment = makeSet(c0, ' "<>`');
const query = makeSet(c0, ' "#<>');
const specialQuery = makeSet(query, "'");
const path = makeSet(query, '?^`{}');
const userinfo = makeSet(path, '/:;=@[\\]|');

const sets = { c0, fragment, query, specialQuery, path, userinfo };

const hex = '0123456789ABCDEF';

function pct(byte) {
  return '%' + hex[byte >> 4] + hex[byte & 15];
}

function encodeCodePoint(cp, set) {
  if (cp < 0x80) return set[cp] ? pct(cp) : StringFromCharCode(cp);
  if (cp < 0x800) return pct(0xc0 | (cp >> 6)) + pct(0x80 | (cp & 63));
  if (cp < 0x10000) {
    return pct(0xe0 | (cp >> 12)) + pct(0x80 | ((cp >> 6) & 63)) +
      pct(0x80 | (cp & 63));
  }
  return pct(0xf0 | (cp >> 18)) + pct(0x80 | ((cp >> 12) & 63)) +
    pct(0x80 | ((cp >> 6) & 63)) + pct(0x80 | (cp & 63));
}

function encodeString(str, set) {
  let out = '';
  for (let i = 0; i < str.length; i++) {
    const cp = str.codePointAt(i);
    if (cp > 0xffff) i++;
    out += encodeCodePoint(cp, set);
  }
  return out;
}

function hexValue(c) {
  if (c >= 48 && c <= 57) return c - 48;
  if (c >= 65 && c <= 70) return c - 55;
  if (c >= 97 && c <= 102) return c - 87;
  return -1;
}

function utf8Bytes(str) {
  const bytes = [];
  for (let i = 0; i < str.length; i++) {
    const cp = str.codePointAt(i);
    if (cp > 0xffff) i++;
    if (cp < 0x80) {
      bytes.push(cp);
    } else if (cp < 0x800) {
      bytes.push(0xc0 | (cp >> 6), 0x80 | (cp & 63));
    } else if (cp < 0x10000) {
      bytes.push(0xe0 | (cp >> 12), 0x80 | ((cp >> 6) & 63), 0x80 | (cp & 63));
    } else {
      bytes.push(0xf0 | (cp >> 18), 0x80 | ((cp >> 12) & 63),
                 0x80 | ((cp >> 6) & 63), 0x80 | (cp & 63));
    }
  }
  return bytes;
}

function percentDecodeBytes(bytes) {
  const out = [];
  for (let i = 0; i < bytes.length; i++) {
    const b = bytes[i];
    if (b === 37 && i + 2 < bytes.length) {
      const hi = hexValue(bytes[i + 1]);
      const lo = hexValue(bytes[i + 2]);
      if (hi >= 0 && lo >= 0) {
        out.push(hi * 16 + lo);
        i += 2;
        continue;
      }
    }
    out.push(b);
  }
  return out;
}

// UTF-8 decode without BOM handling; ill-formed sequences become U+FFFD with
// the Encoding Standard's maximal-subpart rule.
function utf8Decode(bytes) {
  let out = '';
  let needed = 0;
  let cp = 0;
  let seen = 0;
  let lower = 0x80;
  let upper = 0xbf;
  for (let i = 0; i < bytes.length; i++) {
    const b = bytes[i];
    if (needed === 0) {
      if (b <= 0x7f) {
        out += StringFromCharCode(b);
      } else if (b >= 0xc2 && b <= 0xdf) {
        needed = 1;
        cp = b & 0x1f;
      } else if (b >= 0xe0 && b <= 0xef) {
        if (b === 0xe0) lower = 0xa0;
        if (b === 0xed) upper = 0x9f;
        needed = 2;
        cp = b & 0xf;
      } else if (b >= 0xf0 && b <= 0xf4) {
        if (b === 0xf0) lower = 0x90;
        if (b === 0xf4) upper = 0x8f;
        needed = 3;
        cp = b & 7;
      } else {
        out += '�';
      }
      continue;
    }
    if (b < lower || b > upper) {
      cp = needed = seen = 0;
      lower = 0x80;
      upper = 0xbf;
      out += '�';
      i--;
      continue;
    }
    lower = 0x80;
    upper = 0xbf;
    cp = (cp << 6) | (b & 0x3f);
    if (++seen === needed) {
      out += StringFromCodePoint(cp);
      cp = needed = seen = 0;
    }
  }
  if (needed !== 0) out += '�';
  return out;
}

function percentDecodeToString(str) {
  return utf8Decode(percentDecodeBytes(utf8Bytes(str)));
}

module.exports = {
  encodeCodePoint,
  encodeString,
  hexValue,
  percentDecodeToString,
  sets,
  utf8Bytes,
};
