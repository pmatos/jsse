'use strict';

// The JavaScript stand-in for Node's `internalBinding('url')` (ada): the
// functions internal/url.js and url.js call, over the WHATWG parser in
// internal/url/parser. `parse` and `update` also fill `urlComponents` with
// the offsets of ada's url_aggregator.

const { encodeString, sets } = require('internal/url/encode');
const { toUnicode } = require('internal/url/domain');
const { basicParse, stripOpaqueTrailingSpaces } = require('internal/url/parser');
const { components, serialize } = require('internal/url/serialize');
const { ERR_INVALID_URL } = require('internal/errors').codes;

const kProtocol = 0;
const kHost = 1;
const kHostname = 2;
const kPort = 3;
const kUsername = 4;
const kPassword = 5;
const kPathname = 6;
const kSearch = 7;
const kHash = 8;
const kHref = 9;

function parseRecord(input, base) {
  let baseRecord = null;
  if (base !== undefined) {
    baseRecord = basicParse(base.toWellFormed(), null);
    if (baseRecord === null) return null;
  }
  return basicParse(input.toWellFormed(), baseRecord);
}

function parse(input, base, raiseException) {
  const url = parseRecord(input, base);
  if (url === null) {
    if (raiseException) {
      throw new ERR_INVALID_URL(input.toWellFormed(), base?.toWellFormed());
    }
    return undefined;
  }
  return serialize(url);
}

function canParse(input, base) {
  return parseRecord(input, base) !== null;
}

function cannotHaveCredentialsOrPort(url) {
  return url.host === null || url.host === '' || url.scheme === 'file';
}

function applySetter(url, action, value) {
  switch (action) {
    case kProtocol:
      return basicParse(value + ':', null, url, 'scheme start');
    case kHost:
    case kHostname:
      if (typeof url.path === 'string') return false;
      return basicParse(value, null, url, action === kHost ? 'host' : 'hostname');
    case kPort:
      if (cannotHaveCredentialsOrPort(url)) return false;
      if (value === '') {
        url.port = null;
        return url;
      }
      return basicParse(value, null, url, 'port');
    case kUsername:
    case kPassword:
      if (cannotHaveCredentialsOrPort(url)) return false;
      url[action === kUsername ? 'username' : 'password'] = encodeString(value, sets.userinfo);
      return url;
    case kPathname:
      if (typeof url.path === 'string') return false;
      url.path = [];
      return basicParse(value, null, url, 'path start');
    case kSearch:
      if (value === '') {
        url.query = null;
        stripOpaqueTrailingSpaces(url);
        return url;
      }
      url.query = '';
      return basicParse(value[0] === '?' ? value.slice(1) : value, null, url, 'query');
    case kHash:
      if (value === '') {
        url.fragment = null;
        stripOpaqueTrailingSpaces(url);
        return url;
      }
      url.fragment = '';
      return basicParse(value[0] === '#' ? value.slice(1) : value, null, url, 'fragment');
  }
  return false;
}

function update(href, action, value) {
  value = value.toWellFormed();
  if (action === kHref) {
    const url = basicParse(value, null);
    return url === null ? false : serialize(url);
  }
  const url = basicParse(href, null);
  // false: the setter's guard rejected the call. null: a state-override parse
  // failed, which per the URL Standard keeps what it had already changed.
  if (applySetter(url, action, value) === false) return false;
  return serialize(url);
}

function format(href, fragment, unicode, search, auth) {
  const url = basicParse(href, null);
  if (!fragment) url.fragment = null;
  if (unicode && url.host !== null) url.host = toUnicode(url.host);
  if (!search) url.query = null;
  if (!auth) {
    url.username = '';
    url.password = '';
  }
  return serialize(url);
}

function hostnameOf(domain) {
  if (domain === '') return '';
  const url = basicParse('ws://x', null);
  if (!applySetter(url, kHostname, domain.toWellFormed())) return '';
  return url.host;
}

function domainToASCII(domain) {
  return hostnameOf(domain);
}

function domainToUnicode(domain) {
  const host = hostnameOf(domain);
  return host === '' ? '' : toUnicode(host);
}

const posixUnsafe = /[\t\n\r "#%?[\\\]^|~]/g;
const windowsUnsafe = /[\t\n\r "#%?[\]^|~]/g;

function escapeChar(c) {
  return '%' + c.charCodeAt(0).toString(16).toUpperCase().padStart(2, '0');
}

function pathToFileURL(path, windows, hostname) {
  const url = basicParse('file://', null);
  if (hostname !== undefined && !applySetter(url, kHostname, hostname)) {
    throw new ERR_INVALID_URL(path, hostname);
  }
  path = path.toWellFormed().replace(windows ? windowsUnsafe : posixUnsafe, escapeChar);
  url.path = [];
  basicParse(path, null, url, 'path start');
  return serialize(url);
}

module.exports = {
  canParse,
  domainToASCII,
  domainToUnicode,
  format,
  parse,
  pathToFileURL,
  update,
  urlComponents: components,
};
