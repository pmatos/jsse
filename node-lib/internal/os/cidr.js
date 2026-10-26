'use strict';

// getCIDR and countBinaryOnes from Node v26.8.2 lib/internal/util.js (MIT; see
// the license text in internal/path/common).

const {
  NumberParseInt,
  NumberPrototypeToString,
  StringPrototypeIncludes,
  StringPrototypeSlice,
} = primordials;

// Returns the number of ones in the binary representation of the decimal
// number.
function countBinaryOnes(n) {
  // Count the number of bits set in parallel, which is faster than looping
  n = n - ((n >>> 1) & 0x55555555);
  n = (n & 0x33333333) + ((n >>> 2) & 0x33333333);
  return ((n + (n >>> 4) & 0xF0F0F0F) * 0x1010101) >>> 24;
}

function getCIDR(address, netmask, family) {
  let ones = 0;
  let split = '.';
  let range = 10;
  let groupLength = 8;
  let hasZeros = false;
  let lastPos = 0;

  if (family === 'IPv6') {
    split = ':';
    range = 16;
    groupLength = 16;
  }

  for (let i = 0; i < netmask.length; i++) {
    if (netmask[i] !== split) {
      if (i + 1 < netmask.length) {
        continue;
      }
      i++;
    }
    const part = StringPrototypeSlice(netmask, lastPos, i);
    lastPos = i + 1;
    if (part !== '') {
      if (hasZeros) {
        if (part !== '0') {
          return null;
        }
      } else {
        const binary = NumberParseInt(part, range);
        const binaryOnes = countBinaryOnes(binary);
        ones += binaryOnes;
        if (binaryOnes !== groupLength) {
          if (StringPrototypeIncludes(NumberPrototypeToString(binary, 2), '01')) {
            return null;
          }
          hasZeros = true;
        }
      }
    }
  }

  return `${address}/${ones}`;
}

module.exports = { getCIDR };
