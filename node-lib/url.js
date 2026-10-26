// Copyright Joyent, Inc. and other Node contributors.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to permit
// persons to whom the Software is furnished to do so, subject to the
// following conditions:
//
// The above copyright notice and this permission notice shall be included
// in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN
// NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM,
// DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
// OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE
// USE OR OTHER DEALINGS IN THE SOFTWARE.

'use strict';

// The public `url` module: Node v26.8.2 lib/url.js (MIT) with the WHATWG
// parser in JavaScript (internal/url/binding) instead of ada.

const {
  Boolean,
  Int8Array,
  StringPrototypeCharCodeAt,
  StringPrototypeIndexOf,
  StringPrototypeReplaceAll,
  StringPrototypeSlice,
} = primordials;

const { encodeStr, hexTable } = require('internal/querystring');
const querystring = require('querystring');
const { codes: { ERR_INVALID_ARG_TYPE } } = require('internal/errors');
const { validateObject, validateString } = require('internal/url/util');
const {
  URL,
  URLSearchParams,
  domainToASCII,
  domainToUnicode,
  fileURLToPath,
  pathToFileURL: _pathToFileURL,
  urlToHttpOptions,
  slashedProtocol,
} = require('internal/url');
const bindingUrl = require('internal/url/binding');
const { Url, isIpv6Hostname, urlParse } = require('internal/url/legacy_parse');
const { urlResolve, urlResolveObject } = require('internal/url/legacy_resolve');
const {
  CHAR_FORWARD_SLASH,
  CHAR_HASH,
  CHAR_QUESTION_MARK,
} = require('internal/constants');

function urlFormat(urlObject, options) {
  if (typeof urlObject === 'string') {
    urlObject = urlParse(urlObject);
  } else if (typeof urlObject !== 'object' || urlObject === null) {
    throw new ERR_INVALID_ARG_TYPE('urlObject',
                                   ['Object', 'string'], urlObject);
  } else if (urlObject instanceof URL) {
    let fragment = true;
    let unicode = false;
    let search = true;
    let auth = true;

    if (options) {
      validateObject(options, 'options');

      if (options.fragment != null) {
        fragment = Boolean(options.fragment);
      }

      if (options.unicode != null) {
        unicode = Boolean(options.unicode);
      }

      if (options.search != null) {
        search = Boolean(options.search);
      }

      if (options.auth != null) {
        auth = Boolean(options.auth);
      }
    }

    return bindingUrl.format(urlObject.href, fragment, unicode, search, auth);
  }

  return Url.prototype.format.call(urlObject);
}

const noEscapeAuth = new Int8Array([
  0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x00 - 0x0F
  0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 0x10 - 0x1F
  0, 1, 0, 0, 0, 0, 0, 1, 1, 1, 1, 0, 0, 1, 1, 0, // 0x20 - 0x2F
  1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, // 0x30 - 0x3F
  0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 0x40 - 0x4F
  1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 1, // 0x50 - 0x5F
  0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, // 0x60 - 0x6F
  1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 1, 0, // 0x70 - 0x7F
]);

Url.prototype.format = function format() {
  let auth = this.auth || '';
  if (auth) {
    auth = encodeStr(auth, noEscapeAuth, hexTable);
    auth += '@';
  }

  let protocol = this.protocol || '';
  if (protocol && StringPrototypeCharCodeAt(protocol, protocol.length - 1) !== 58 /* : */) {
    protocol += ':';
  }

  let pathname = this.pathname || '';
  let hash = this.hash || '';
  let host = '';
  let query = '';

  if (this.host) {
    host = auth + this.host;
  } else if (this.hostname) {
    host = auth + (
      StringPrototypeIndexOf(this.hostname, ':') !== -1 && !isIpv6Hostname(this.hostname) ?
        '[' + this.hostname + ']' :
        this.hostname
    );
    if (this.port) {
      host += ':' + this.port;
    }
  }

  if (this.query !== null && typeof this.query === 'object') {
    query = querystring.stringify(this.query);
  }
  let search = this.search || (query && ('?' + query)) || '';

  if (StringPrototypeIndexOf(pathname, '#') !== -1 || StringPrototypeIndexOf(pathname, '?') !== -1) {
    let newPathname = '';
    let lastPos = 0;
    const len = pathname.length;
    for (let i = 0; i < len; i++) {
      const code = StringPrototypeCharCodeAt(pathname, i);
      if (code === CHAR_HASH || code === CHAR_QUESTION_MARK) {
        if (i > lastPos) {
          newPathname += StringPrototypeSlice(pathname, lastPos, i);
        }
        newPathname += (code === CHAR_HASH ? '%23' : '%3F');
        lastPos = i + 1;
      }
    }
    if (lastPos < len) {
      newPathname += StringPrototypeSlice(pathname, lastPos);
    }
    pathname = newPathname;
  }

  if (this.slashes || slashedProtocol.has(protocol)) {
    if (this.slashes || host) {
      if (pathname && StringPrototypeCharCodeAt(pathname, 0) !== CHAR_FORWARD_SLASH)
        pathname = '/' + pathname;
      host = '//' + host;
    } else if (protocol.length >= 4 &&
               StringPrototypeCharCodeAt(protocol, 0) === 102/* f */ &&
               StringPrototypeCharCodeAt(protocol, 1) === 105/* i */ &&
               StringPrototypeCharCodeAt(protocol, 2) === 108/* l */ &&
               StringPrototypeCharCodeAt(protocol, 3) === 101/* e */) {
      host = '//';
    }
  }

  if (StringPrototypeIndexOf(search, '#') !== -1) {
    search = StringPrototypeReplaceAll(search, '#', '%23');
  }

  if (hash && StringPrototypeCharCodeAt(hash, 0) !== CHAR_HASH) {
    hash = '#' + hash;
  }
  if (search && StringPrototypeCharCodeAt(search, 0) !== CHAR_QUESTION_MARK) {
    search = '?' + search;
  }

  return protocol + host + pathname + search + hash;
};


function pathToFileURL(path, options) {
  validateString(path, 'path');

  return _pathToFileURL(path, options);
}

module.exports = {
  Url,
  parse: urlParse,
  resolve: urlResolve,
  resolveObject: urlResolveObject,
  format: urlFormat,

  URL,
  URLSearchParams,
  domainToASCII,
  domainToUnicode,

  pathToFileURL,
  fileURLToPath,
  urlToHttpOptions,
};
