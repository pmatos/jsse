// Tests URI encoding/decoding edge cases.
// Spec: ECMAScript 2024, sec-uri-handling-functions

// Basic encodeURIComponent
if (encodeURIComponent("hello world") !== "hello%20world") {
  throw new Test262Error('encodeURIComponent("hello world") failed');
}

// Reserved characters in encodeURIComponent (should encode all)
var reserved = ";,/?:@&=+$#";
for (var i = 0; i < reserved.length; i++) {
  var ch = reserved[i];
  var encoded = encodeURIComponent(ch);
  if (encoded === ch) {
    throw new Test262Error('encodeURIComponent should encode ' + JSON.stringify(ch));
  }
}

// encodeURI preserves reserved characters
var uriReserved = ";,/?:@&=+$#";
for (var i = 0; i < uriReserved.length; i++) {
  var ch = uriReserved[i];
  var encoded = encodeURI(ch);
  if (encoded !== ch) {
    throw new Test262Error('encodeURI should preserve ' + JSON.stringify(ch) + ', got: ' + encoded);
  }
}

// Surrogate pair encoding
var emoji = "\uD83D\uDE00"; // U+1F600 😀
var encoded = encodeURIComponent(emoji);
if (encoded !== "%F0%9F%98%80") {
  throw new Test262Error('surrogate pair encoding failed, got: ' + encoded);
}
if (decodeURIComponent(encoded) !== emoji) {
  throw new Test262Error('surrogate pair round-trip failed');
}

// URIError on lone surrogates
try {
  encodeURIComponent("\uD800");
  throw new Test262Error('lone high surrogate should throw URIError');
} catch (e) {
  if (!(e instanceof URIError)) {
    throw new Test262Error('lone high surrogate should throw URIError, got: ' + e);
  }
}

try {
  encodeURIComponent("\uDC00");
  throw new Test262Error('lone low surrogate should throw URIError');
} catch (e) {
  if (!(e instanceof URIError)) {
    throw new Test262Error('lone low surrogate should throw URIError, got: ' + e);
  }
}

// URIError on malformed percent sequences in decode
try {
  decodeURIComponent("%");
  throw new Test262Error('malformed percent should throw URIError');
} catch (e) {
  if (!(e instanceof URIError)) {
    throw new Test262Error('malformed percent should throw URIError, got: ' + e);
  }
}

try {
  decodeURIComponent("%G0");
  throw new Test262Error('invalid hex in percent should throw URIError');
} catch (e) {
  if (!(e instanceof URIError)) {
    throw new Test262Error('invalid hex should throw URIError, got: ' + e);
  }
}

// Round-trip for various characters
var testChars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_.!~*'()";
for (var i = 0; i < testChars.length; i++) {
  var ch = testChars[i];
  if (encodeURIComponent(ch) !== ch) {
    throw new Test262Error('unreserved character ' + JSON.stringify(ch) + ' should not be encoded');
  }
}
