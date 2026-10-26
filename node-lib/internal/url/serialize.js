'use strict';

// URL record serialization plus the component offsets of ada's
// url_aggregator, which internal/url.js slices href with.

const OMITTED = 4294967295;
// ada::scheme::type
const schemeTypes = {
  __proto__: null, http: 0, https: 2, ws: 3, ftp: 4, wss: 5, file: 6,
};

const components = new Uint32Array(9);

function serializePath(url) {
  if (typeof url.path === 'string') return url.path;
  let out = '';
  for (let i = 0; i < url.path.length; i++) out += '/' + url.path[i];
  return out;
}

function serialize(url, excludeFragment) {
  let out = url.scheme + ':';
  const protocolEnd = out.length;
  let usernameEnd = protocolEnd;
  let hostStart = protocolEnd;
  let hostEnd = protocolEnd;
  if (url.host !== null) {
    out += '//';
    usernameEnd = hostStart = out.length;
    if (url.username !== '' || url.password !== '') {
      out += url.username;
      usernameEnd = out.length;
      if (url.password !== '') out += ':' + url.password;
      hostStart = out.length;
      out += '@';
    }
    out += url.host;
    hostEnd = out.length;
    if (url.port !== null) out += ':' + url.port;
  } else if (typeof url.path !== 'string' && url.path.length > 1 && url.path[0] === '') {
    out += '/.';
  }
  const pathnameStart = out.length;
  out += serializePath(url);
  let searchStart = OMITTED;
  if (url.query !== null) {
    searchStart = out.length;
    out += '?' + url.query;
  }
  let hashStart = OMITTED;
  if (!excludeFragment && url.fragment !== null) {
    hashStart = out.length;
    out += '#' + url.fragment;
  }
  components[0] = protocolEnd;
  components[1] = usernameEnd;
  components[2] = hostStart;
  components[3] = hostEnd;
  components[4] = url.port === null ? OMITTED : url.port;
  components[5] = pathnameStart;
  components[6] = searchStart;
  components[7] = hashStart;
  components[8] = url.scheme in schemeTypes ? schemeTypes[url.scheme] : 1;
  return out;
}

module.exports = { OMITTED, components, serialize };
