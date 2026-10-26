'use strict';

// The WHATWG basic URL parser (https://url.spec.whatwg.org/#concept-basic-url-parser)
// over URL records: { scheme, username, password, host, port, path, query,
// fragment }. `host` is the serialized host or null, `path` is an array of
// segments or, for an opaque path, a string.

const { encodeCodePoint, encodeString, sets } = require('internal/url/encode');
const { parseHost } = require('internal/url/host');

const EOF = -1;
const defaultPorts = {
  __proto__: null, ftp: 21, file: null, http: 80, https: 443, ws: 80, wss: 443,
};

const isSpecial = (scheme) => scheme in defaultPorts;
const isAlpha = (c) => (c >= 65 && c <= 90) || (c >= 97 && c <= 122);
const isDigit = (c) => c >= 48 && c <= 57;
const isDriveLetter = (s) => /^[a-zA-Z][:|]$/.test(s);
const isNormalizedDriveLetter = (s) => /^[a-zA-Z]:$/.test(s);
const isDoubleDot = (s) => /^(\.|%2e)(\.|%2e)$/i.test(s);
const isSingleDot = (s) => /^(\.|%2e)$/i.test(s);

function startsWithDriveLetter(cps, p) {
  return cps.length - p >= 2 && isAlpha(cps[p]) &&
    (cps[p + 1] === 58 || cps[p + 1] === 124) &&
    (cps.length - p === 2 || cps[p + 2] === 47 || cps[p + 2] === 92 ||
     cps[p + 2] === 63 || cps[p + 2] === 35);
}

function shortenPath(url) {
  const { path } = url;
  if (url.scheme === 'file' && path.length === 1 && isNormalizedDriveLetter(path[0])) return;
  path.pop();
}

function hasCredentials(url) {
  return url.username !== '' || url.password !== '';
}

function newURL() {
  return {
    scheme: '', username: '', password: '', host: null, port: null,
    path: [], query: null, fragment: null,
  };
}

function stripOpaqueTrailingSpaces(url) {
  if (typeof url.path !== 'string' || url.fragment !== null || url.query !== null) return;
  url.path = url.path.replace(/ +$/, '');
}

function basicParse(input, base, url, stateOverride) {
  if (url === undefined) {
    url = newURL();
    input = input.replace(/^[\0-\x20]+|[\0-\x20]+$/g, '');
  }
  input = input.replace(/[\t\n\r]/g, '');

  const cps = [];
  for (let i = 0; i < input.length; i++) {
    const cp = input.codePointAt(i);
    if (cp > 0xffff) i++;
    cps.push(cp);
  }

  let state = stateOverride || 'scheme start';
  let buffer = '';
  let atSignSeen = false;
  let insideBrackets = false;
  let passwordTokenSeen = false;
  let pointer = 0;

  for (;; pointer++) {
    const c = pointer < cps.length ? cps[pointer] : EOF;
    const sp = isSpecial(url.scheme);
    const slashLike = c === 47 || (sp && c === 92);

    switch (state) {
      case 'scheme start':
        if (isAlpha(c)) {
          buffer += String.fromCharCode(c | 0x20);
          state = 'scheme';
        } else if (!stateOverride) {
          state = 'no scheme';
          pointer--;
        } else {
          return null;
        }
        break;

      case 'scheme':
        if (isAlpha(c) || isDigit(c) || c === 43 || c === 45 || c === 46) {
          buffer += String.fromCharCode(isAlpha(c) ? c | 0x20 : c);
        } else if (c === 58) {
          if (stateOverride) {
            if (isSpecial(url.scheme) !== isSpecial(buffer)) return url;
            if ((hasCredentials(url) || url.port !== null) && buffer === 'file') return url;
            if (url.scheme === 'file' && url.host === '') return url;
          }
          url.scheme = buffer;
          if (stateOverride) {
            if (url.port === defaultPorts[url.scheme]) url.port = null;
            return url;
          }
          buffer = '';
          if (url.scheme === 'file') {
            state = 'file';
          } else if (isSpecial(url.scheme) && base !== null && base.scheme === url.scheme) {
            state = 'special relative or authority';
          } else if (isSpecial(url.scheme)) {
            state = 'special authority slashes';
          } else if (cps[pointer + 1] === 47) {
            state = 'path or authority';
            pointer++;
          } else {
            url.path = '';
            state = 'opaque path';
          }
        } else if (!stateOverride) {
          buffer = '';
          state = 'no scheme';
          pointer = -1;
        } else {
          return null;
        }
        break;

      case 'no scheme':
        if (base === null || (typeof base.path === 'string' && c !== 35)) return null;
        if (typeof base.path === 'string' && c === 35) {
          url.scheme = base.scheme;
          url.path = base.path;
          url.query = base.query;
          url.fragment = '';
          state = 'fragment';
        } else {
          state = base.scheme !== 'file' ? 'relative' : 'file';
          pointer--;
        }
        break;

      case 'special relative or authority':
        if (c === 47 && cps[pointer + 1] === 47) {
          state = 'special authority ignore slashes';
          pointer++;
        } else {
          state = 'relative';
          pointer--;
        }
        break;

      case 'path or authority':
        if (c === 47) {
          state = 'authority';
        } else {
          state = 'path';
          pointer--;
        }
        break;

      case 'relative':
        url.scheme = base.scheme;
        if (c === 47) {
          state = 'relative slash';
        } else if (isSpecial(url.scheme) && c === 92) {
          state = 'relative slash';
        } else {
          url.username = base.username;
          url.password = base.password;
          url.host = base.host;
          url.port = base.port;
          url.path = base.path.slice();
          url.query = base.query;
          if (c === 63) {
            url.query = '';
            state = 'query';
          } else if (c === 35) {
            url.fragment = '';
            state = 'fragment';
          } else if (c !== EOF) {
            url.query = null;
            shortenPath(url);
            state = 'path';
            pointer--;
          }
        }
        break;

      case 'relative slash':
        if (sp && (c === 47 || c === 92)) {
          state = 'special authority ignore slashes';
        } else if (c === 47) {
          state = 'authority';
        } else {
          url.username = base.username;
          url.password = base.password;
          url.host = base.host;
          url.port = base.port;
          state = 'path';
          pointer--;
        }
        break;

      case 'special authority slashes':
        if (c === 47 && cps[pointer + 1] === 47) {
          state = 'special authority ignore slashes';
          pointer++;
        } else {
          state = 'special authority ignore slashes';
          pointer--;
        }
        break;

      case 'special authority ignore slashes':
        if (c !== 47 && c !== 92) {
          state = 'authority';
          pointer--;
        }
        break;

      case 'authority':
        if (c === 64) {
          if (atSignSeen) buffer = '%40' + buffer;
          atSignSeen = true;
          for (const ch of buffer) {
            if (ch === ':' && !passwordTokenSeen) {
              passwordTokenSeen = true;
              continue;
            }
            const encoded = encodeCodePoint(ch.codePointAt(0), sets.userinfo);
            if (passwordTokenSeen) url.password += encoded;
            else url.username += encoded;
          }
          buffer = '';
        } else if (c === EOF || c === 47 || c === 63 || c === 35 || (sp && c === 92)) {
          if (atSignSeen && buffer === '') return null;
          pointer -= Array.from(buffer).length + 1;
          buffer = '';
          state = 'host';
        } else {
          buffer += String.fromCodePoint(c);
        }
        break;

      case 'host':
      case 'hostname':
        if (stateOverride && url.scheme === 'file') {
          pointer--;
          state = 'file host';
        } else if (c === 58 && !insideBrackets) {
          if (buffer === '') return null;
          if (stateOverride === 'hostname') return null;
          const host = parseHost(buffer, !sp);
          if (host === null) return null;
          url.host = host;
          buffer = '';
          state = 'port';
        } else if (c === EOF || c === 47 || c === 63 || c === 35 || (sp && c === 92)) {
          pointer--;
          if (sp && buffer === '') return null;
          if (stateOverride && buffer === '' && (hasCredentials(url) || url.port !== null)) {
            return null;
          }
          const host = parseHost(buffer, !sp);
          if (host === null) return null;
          url.host = host;
          buffer = '';
          state = 'path start';
          if (stateOverride) return url;
        } else {
          if (c === 91) insideBrackets = true;
          if (c === 93) insideBrackets = false;
          buffer += String.fromCodePoint(c);
        }
        break;

      case 'port':
        if (isDigit(c)) {
          buffer += String.fromCharCode(c);
        } else if (c === EOF || c === 47 || c === 63 || c === 35 || (sp && c === 92) || stateOverride) {
          if (buffer !== '') {
            const port = parseInt(buffer, 10);
            if (port > 65535) return null;
            url.port = port === defaultPorts[url.scheme] ? null : port;
            buffer = '';
            if (stateOverride) return url;
          }
          if (stateOverride) return url;
          state = 'path start';
          pointer--;
        } else {
          return null;
        }
        break;

      case 'file':
        url.scheme = 'file';
        url.host = '';
        if (c === 47 || c === 92) {
          state = 'file slash';
        } else if (base !== null && base.scheme === 'file') {
          url.host = base.host;
          url.path = base.path.slice();
          url.query = base.query;
          if (c === 63) {
            url.query = '';
            state = 'query';
          } else if (c === 35) {
            url.fragment = '';
            state = 'fragment';
          } else if (c !== EOF) {
            url.query = null;
            if (!startsWithDriveLetter(cps, pointer)) shortenPath(url);
            else url.path = [];
            state = 'path';
            pointer--;
          }
        } else {
          state = 'path';
          pointer--;
        }
        break;

      case 'file slash':
        if (c === 47 || c === 92) {
          state = 'file host';
        } else {
          if (base !== null && base.scheme === 'file') {
            url.host = base.host;
            if (!startsWithDriveLetter(cps, pointer) &&
                typeof base.path !== 'string' && base.path.length > 0 &&
                isNormalizedDriveLetter(base.path[0])) {
              url.path.push(base.path[0]);
            }
          }
          state = 'path';
          pointer--;
        }
        break;

      case 'file host':
        if (c === EOF || c === 47 || c === 92 || c === 63 || c === 35) {
          pointer--;
          if (!stateOverride && isDriveLetter(buffer)) {
            state = 'path';
          } else if (buffer === '') {
            url.host = '';
            if (stateOverride) return url;
            state = 'path start';
          } else {
            let host = parseHost(buffer, !sp);
            if (host === null) return null;
            if (host === 'localhost') host = '';
            url.host = host;
            if (stateOverride) return url;
            buffer = '';
            state = 'path start';
          }
        } else {
          buffer += String.fromCodePoint(c);
        }
        break;

      case 'path start':
        if (sp) {
          state = 'path';
          if (c !== 47 && c !== 92) pointer--;
        } else if (!stateOverride && c === 63) {
          url.query = '';
          state = 'query';
        } else if (!stateOverride && c === 35) {
          url.fragment = '';
          state = 'fragment';
        } else if (c !== EOF) {
          state = 'path';
          if (c !== 47) pointer--;
        } else if (stateOverride && url.host === null) {
          url.path.push('');
        }
        break;

      case 'path':
        if (c === EOF || slashLike || (!stateOverride && (c === 63 || c === 35))) {
          if (isDoubleDot(buffer)) {
            shortenPath(url);
            if (!slashLike) url.path.push('');
          } else if (isSingleDot(buffer) && !slashLike) {
            url.path.push('');
          } else if (!isSingleDot(buffer)) {
            if (url.scheme === 'file' && url.path.length === 0 && isDriveLetter(buffer)) {
              buffer = buffer[0] + ':';
            }
            url.path.push(buffer);
          }
          buffer = '';
          if (c === 63) {
            url.query = '';
            state = 'query';
          } else if (c === 35) {
            url.fragment = '';
            state = 'fragment';
          }
        } else {
          buffer += encodeCodePoint(c, sets.path);
        }
        break;

      case 'opaque path':
        if (c === 63) {
          url.query = '';
          state = 'query';
        } else if (c === 35) {
          url.fragment = '';
          state = 'fragment';
        } else if (c === 32 && (cps[pointer + 1] === 63 || cps[pointer + 1] === 35)) {
          url.path += '%20';
        } else if (c !== EOF) {
          url.path += encodeCodePoint(c, sets.c0);
        }
        break;

      case 'query':
        if ((!stateOverride && c === 35) || c === EOF) {
          url.query += encodeString(buffer, sp ? sets.specialQuery : sets.query);
          buffer = '';
          if (c === 35) {
            url.fragment = '';
            state = 'fragment';
          }
        } else {
          buffer += String.fromCodePoint(c);
        }
        break;

      case 'fragment':
        if (c !== EOF) url.fragment += encodeCodePoint(c, sets.fragment);
        break;
    }

    if (pointer >= cps.length) break;
  }
  return url;
}

module.exports = {
  basicParse, defaultPorts, hasCredentials, isSpecial, newURL, stripOpaqueTrailingSpaces,
};
