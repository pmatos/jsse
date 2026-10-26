'use strict';

// The public pieces of Node v26.8.2 lib/internal/url.js (MIT) outside the two
// classes: file URL conversion, domainTo*, urlToHttpOptions and the protocol
// tables url.parse uses. The Buffer-returning path variants, URLPattern and
// URL.createObjectURL are not embedded.

const {
  Number,
  StringPrototypeCharAt,
  StringPrototypeCharCodeAt,
  StringPrototypeCodePointAt,
  StringPrototypeIncludes,
  StringPrototypeIndexOf,
  StringPrototypeReplace,
  StringPrototypeSlice,
  StringPrototypeStartsWith,
  decodeURIComponent,
} = primordials;

const {
  codes: {
    ERR_INVALID_ARG_TYPE,
    ERR_INVALID_ARG_VALUE,
    ERR_INVALID_FILE_URL_HOST,
    ERR_INVALID_FILE_URL_PATH,
    ERR_INVALID_URL_SCHEME,
    ERR_MISSING_ARGS,
  },
} = require('internal/errors');
const {
  CHAR_BACKWARD_SLASH,
  CHAR_FORWARD_SLASH,
  CHAR_LOWERCASE_A,
  CHAR_LOWERCASE_Z,
} = require('internal/constants');
const { isWindows } = require('internal/util');
const path = require('path');
const bindingUrl = require('internal/url/binding');
const { URL, kCreateURLFromPosixPathSymbol, kCreateURLFromWindowsPathSymbol } =
  require('internal/url/url');
const { URLSearchParams } = require('internal/url/params');
const { encodeStr } = require('internal/querystring');
const { hooks, kEmptyObject, validateObjectAllowObjects } = require('internal/url/util');

const FORWARD_SLASH = /\//g;

const unsafeProtocol = new Set([
  'javascript',
  'javascript:',
]);
const hostlessProtocol = new Set([
  'javascript',
  'javascript:',
]);
const slashedProtocol = new Set([
  'http',
  'http:',
  'https',
  'https:',
  'ftp',
  'ftp:',
  'gopher',
  'gopher:',
  'file',
  'file:',
  'ws',
  'ws:',
  'wss',
  'wss:',
]);

function isURL(self) {
  return Boolean(self?.href && self.protocol && self.auth === undefined && self.path === undefined);
}

function domainToASCII(domain) {
  if (arguments.length < 1)
    throw new ERR_MISSING_ARGS('domain');

  return bindingUrl.domainToASCII(`${domain}`);
}

function domainToUnicode(domain) {
  if (arguments.length < 1)
    throw new ERR_MISSING_ARGS('domain');

  return bindingUrl.domainToUnicode(`${domain}`);
}

/**
 * Utility function that converts a URL object into an ordinary options object
 * as expected by the `http.request` and `https.request` APIs.
 * @param {URL} url
 * @returns {Record<string, unknown>}
 */
function urlToHttpOptions(url) {
  validateObjectAllowObjects(url, 'url');
  const { hostname, pathname, port, username, password, search } = url;
  const options = {
    __proto__: null,
    ...url, // In case the url object was extended by the user.
    protocol: url.protocol,
    hostname: hostname && hostname[0] === '[' ?
      StringPrototypeSlice(hostname, 1, -1) :
      hostname,
    hash: url.hash,
    search: search,
    pathname: pathname,
    path: `${pathname || ''}${search || ''}`,
    href: url.href,
  };
  if (port !== '') {
    options.port = Number(port);
  }
  if (username || password) {
    options.auth = `${decodeURIComponent(username)}:${decodeURIComponent(password)}`;
  }
  return options;
}

function getPathFromURLWin32(url) {
  const hostname = url.hostname;
  let pathname = url.pathname;
  for (let n = 0; n < pathname.length; n++) {
    if (pathname[n] === '%') {
      const third = StringPrototypeCodePointAt(pathname, n + 2) | 0x20;
      if ((pathname[n + 1] === '2' && third === 102) || // 2f 2F /
          (pathname[n + 1] === '5' && third === 99)) {  // 5c 5C \
        throw new ERR_INVALID_FILE_URL_PATH(
          'must not include encoded \\ or / characters', url,
        );
      }
    }
  }
  pathname = StringPrototypeReplace(pathname, FORWARD_SLASH, '\\');
  if (StringPrototypeIncludes(pathname, '%')) {
    pathname = decodeURIComponent(pathname);
  }
  if (hostname !== '') {
    return `\\\\${domainToUnicode(hostname)}${pathname}`;
  }
  const letter = StringPrototypeCodePointAt(pathname, 1) | 0x20;
  const sep = StringPrototypeCharAt(pathname, 2);
  if (letter < CHAR_LOWERCASE_A || letter > CHAR_LOWERCASE_Z ||   // a..z A..Z
      (sep !== ':')) {
    throw new ERR_INVALID_FILE_URL_PATH('must be absolute', url);
  }
  return StringPrototypeSlice(pathname, 1);
}

function getPathFromURLPosix(url) {
  if (url.hostname !== '') {
    throw new ERR_INVALID_FILE_URL_HOST(process.platform);
  }
  const pathname = url.pathname;
  for (let n = 0; n < pathname.length; n++) {
    if (pathname[n] === '%') {
      const third = StringPrototypeCodePointAt(pathname, n + 2) | 0x20;
      if (pathname[n + 1] === '2' && third === 102) {
        throw new ERR_INVALID_FILE_URL_PATH(
          'must not include encoded / characters',
          url,
        );
      }
    }
  }
  return StringPrototypeIncludes(pathname, '%') ? decodeURIComponent(pathname) : pathname;
}

function fileURLToPath(path, options = kEmptyObject) {
  const windows = options?.windows;
  if (typeof path === 'string')
    path = new URL(path);
  else if (!isURL(path))
    throw new ERR_INVALID_ARG_TYPE('path', ['string', 'URL'], path);
  if (path.protocol !== 'file:')
    throw new ERR_INVALID_URL_SCHEME('file');
  return (windows ?? isWindows) ? getPathFromURLWin32(path) : getPathFromURLPosix(path);
}

function pathToFileURL(filepath, options = kEmptyObject) {
  const windows = options?.windows ?? isWindows;
  const isUNC = windows && StringPrototypeStartsWith(filepath, '\\\\');
  let resolved = isUNC ?
    filepath :
    (windows ? path.win32.resolve(filepath) : path.posix.resolve(filepath));
  if (isUNC || (windows && StringPrototypeStartsWith(resolved, '\\\\'))) {
    // Ref: https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation
    const isExtendedUNC = StringPrototypeStartsWith(resolved, '\\\\?\\UNC\\');
    const prefixLength = isExtendedUNC ? 8 : 2;
    const hostnameEndIndex = StringPrototypeIndexOf(resolved, '\\', prefixLength);
    if (hostnameEndIndex === -1) {
      throw new ERR_INVALID_ARG_VALUE(
        'path',
        resolved,
        'Missing UNC resource path',
      );
    }
    if (hostnameEndIndex === 2) {
      throw new ERR_INVALID_ARG_VALUE(
        'path',
        resolved,
        'Empty UNC servername',
      );
    }
    const hostname = StringPrototypeSlice(resolved, prefixLength, hostnameEndIndex);
    return new URL(StringPrototypeSlice(resolved, hostnameEndIndex), hostname, kCreateURLFromWindowsPathSymbol);
  }
  const filePathLast = StringPrototypeCharCodeAt(filepath,
                                                 filepath.length - 1);
  if ((filePathLast === CHAR_FORWARD_SLASH ||
       ((windows ?? isWindows) && filePathLast === CHAR_BACKWARD_SLASH)) &&
      resolved[resolved.length - 1] !== path.sep)
    resolved += '/';

  return new URL(resolved, undefined, windows ? kCreateURLFromWindowsPathSymbol : kCreateURLFromPosixPathSymbol);
}

function toPathIfFileURL(fileURLOrPath) {
  if (!isURL(fileURLOrPath))
    return fileURLOrPath;
  return fileURLToPath(fileURLOrPath);
}


module.exports = {
  fileURLToPath,
  pathToFileURL,
  toPathIfFileURL,
  URL,
  URLSearchParams,
  URLParse: URL.parse,
  domainToASCII,
  domainToUnicode,
  urlToHttpOptions,
  encodeStr,
  isURL,
  isURLInstance: hooks.isURLInstance,
  getURLHref: hooks.getURLHref,
  unsafeProtocol,
  hostlessProtocol,
  slashedProtocol,
};
