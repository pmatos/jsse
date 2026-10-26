'use strict';

// URL and its URLContext, from Node v26.8.2 lib/internal/url.js (MIT). The
// parser behind it is internal/url/binding; URLSearchParams is in
// internal/url/params and shares private state through `hooks`.

const {
  ObjectDefineProperties,
  StringPrototypeSlice,
  StringPrototypeToWellFormed,
  Symbol,
  SymbolToStringTag,
} = primordials;

const { inspect } = require('internal/util/inspect');
const {
  codes: { ERR_INVALID_URL, ERR_MISSING_ARGS },
} = require('internal/errors');
const bindingUrl = require('internal/url/binding');
const { URLSearchParams } = require('internal/url/params');
const {
  getConstructorOf,
  hooks,
  kEnumerableProperty,
  kInspect,
} = require('internal/url/util');

const contextForInspect = Symbol('context');

const updateActions = {
  kProtocol: 0,
  kHost: 1,
  kHostname: 2,
  kPort: 3,
  kUsername: 4,
  kPassword: 5,
  kPathname: 6,
  kSearch: 7,
  kHash: 8,
  kHref: 9,
};
/**
 * Copy href and the latest `urlComponents` snapshot into a URLContext.
 * Property assignment order matches the historical URLContext fields so
 * `util.inspect(..., { showHidden: true })` stays stable.
 * @param {object} ctx
 * @param {string} href
 */
function setURLContextFromBinding(ctx, href) {
  const c = bindingUrl.urlComponents;
  ctx.href = href;
  ctx.protocol_end = c[0];
  ctx.username_end = c[1];
  ctx.host_start = c[2];
  ctx.host_end = c[3];
  ctx.pathname_start = c[5];
  ctx.search_start = c[6];
  ctx.hash_start = c[7];
  ctx.port = c[4];
  ctx.scheme_type = c[8];
}

// Refs: https://url.spec.whatwg.org/#concept-url
class URLContext {
  static #omitted = 4294967295;

  /**
   * @param {string} [href] Parsed href. When omitted, create an empty context
   *   (used by `URL.parse` on invalid input). When provided, `bindingUrl.parse`
   *   / `update` has just written `urlComponents`.
   */
  constructor(href) {
    if (href === undefined) {
      this.href = '';
      this.protocol_end = 0;
      this.username_end = 0;
      this.host_start = 0;
      this.host_end = 0;
      this.pathname_start = 0;
      this.search_start = 0;
      this.hash_start = 0;
      this.port = 0;
      this.scheme_type = 1;
      return;
    }
    setURLContextFromBinding(this, href);
  }

  get hasPort() {
    return this.port !== URLContext.#omitted;
  }

  get hasSearch() {
    return this.search_start !== URLContext.#omitted;
  }

  get hasHash() {
    return this.hash_start !== URLContext.#omitted;
  }
}

/**
 * A unique symbol used as a private identifier to safely invoke the URL constructor
 * with a special parsing behavior. When passed as the third argument to the URL
 * constructor, it signals that the constructor should not throw an exception
 * for invalid URL inputs.
 */
const kParseURLSymbol = Symbol('kParseURL');
const kCreateURLFromPosixPathSymbol = Symbol('kCreateURLFromPosixPath');
const kCreateURLFromWindowsPathSymbol = Symbol('kCreateURLFromWindowsPath');

class URL {
  #context;
  #searchParams;
  #searchParamsModified;

  static {
    hooks.isURLInstance = (value) => typeof value === 'object' && value !== null && #context in value;

    hooks.getURLHref = (value) => {
      value.#ensureSearchParamsUpdated();
      return value.#context.href;
    };

    hooks.setURLSearchParamsModified = (obj) => {
      obj.#searchParamsModified = true;

      if (obj.#context.hasSearch) {
        obj.#updateContext(bindingUrl.update(obj.#context.href, updateActions.kSearch, ''));
      }
    };
  }

  constructor(input, base = undefined, parseSymbol = undefined) {
    if (arguments.length === 0) {
      throw new ERR_MISSING_ARGS('url');
    }

    if (typeof input !== 'string') {
      input = `${input}`;
    }

    if (base !== undefined && typeof base !== 'string') {
      base = `${base}`;
    }

    let href;
    if (arguments.length < 3) {
      href = bindingUrl.parse(input, base, true);
    } else {
      const raiseException = parseSymbol !== kParseURLSymbol;
      const interpretAsWindowsPath = parseSymbol === kCreateURLFromWindowsPathSymbol;
      const pathToFileURL = interpretAsWindowsPath || (parseSymbol === kCreateURLFromPosixPathSymbol);
      href = pathToFileURL ?
        bindingUrl.pathToFileURL(input, interpretAsWindowsPath, base) :
        bindingUrl.parse(input, base, raiseException);
    }

    this.#context = href ? new URLContext(href) : new URLContext();
  }

  static parse(input, base = undefined) {
    if (arguments.length === 0) {
      throw new ERR_MISSING_ARGS('url');
    }
    const parsedURLObject = new URL(input, base, kParseURLSymbol);
    return parsedURLObject.href ? parsedURLObject : null;
  }

  [kInspect](depth, opts) {
    if (typeof depth === 'number' && depth < 0)
      return this;

    const constructor = getConstructorOf(this) || URL;
    const obj = { __proto__: { constructor } };

    obj.href = this.href;
    obj.origin = this.origin;
    obj.protocol = this.protocol;
    obj.username = this.username;
    obj.password = this.password;
    obj.host = this.host;
    obj.hostname = this.hostname;
    obj.port = this.port;
    obj.pathname = this.pathname;
    obj.search = this.search;
    obj.searchParams = this.searchParams;
    obj.hash = this.hash;

    if (opts.showHidden) {
      obj[contextForInspect] = this.#context;
    }

    return `${constructor.name} ${inspect(obj, opts)}`;
  }

  #getSearchFromContext() {
    if (!this.#context.hasSearch) return '';
    let endsAt = this.#context.href.length;
    if (this.#context.hasHash) endsAt = this.#context.hash_start;
    if (endsAt - this.#context.search_start <= 1) return '';
    return StringPrototypeSlice(this.#context.href, this.#context.search_start, endsAt);
  }

  #getSearchFromParams() {
    if (!this.#searchParams?.size) return '';
    return `?${this.#searchParams}`;
  }

  #ensureSearchParamsUpdated() {
    if (this.#searchParamsModified) {
      this.#searchParamsModified = false;
      this.#updateContext(bindingUrl.update(this.#context.href, updateActions.kSearch, this.#getSearchFromParams()));
    }
  }

  /**
   * Update the internal context state for URL.
   * @param {string} href New href string from `bindingUrl.update`.
   * @param {boolean} [shouldUpdateSearchParams] If the update has potential to update search params (href/search).
   */
  #updateContext(href, shouldUpdateSearchParams = false) {
    const previousSearch = shouldUpdateSearchParams && this.#searchParams &&
      (this.#searchParamsModified ? this.#getSearchFromParams() : this.#getSearchFromContext());

    setURLContextFromBinding(this.#context, href);

    if (this.#searchParams) {
      if (shouldUpdateSearchParams) {
        const currentSearch = this.#getSearchFromContext();
        if (previousSearch !== currentSearch) {
          hooks.setURLSearchParams(this.#searchParams, currentSearch);
          this.#searchParamsModified = false;
        }
      }

      this.#ensureSearchParamsUpdated();
    }
  }

  toString() {
    this.#ensureSearchParamsUpdated();
    return this.#context.href;
  }

  get href() {
    this.#ensureSearchParamsUpdated();
    return this.#context.href;
  }

  set href(value) {
    value = `${value}`;
    const href = bindingUrl.update(this.#context.href, updateActions.kHref, value);
    if (!href) { throw new ERR_INVALID_URL(value); }
    this.#updateContext(href, true);
  }

  get origin() {
    const protocol = StringPrototypeSlice(this.#context.href, 0, this.#context.protocol_end);

    if (this.#context.scheme_type !== 1) {
      if (this.#context.scheme_type === 6) {
        return 'null';
      }
      return `${protocol}//${this.host}`;
    }

    if (protocol === 'blob:') {
      const path = this.pathname;
      if (path.length > 0) {
        try {
          const out = new URL(path);
          if (out.#context.scheme_type === 0 || out.#context.scheme_type === 2) {
            return `${out.protocol}//${out.host}`;
          }
        } catch {
        }
      }
    }

    return 'null';
  }

  get protocol() {
    return StringPrototypeSlice(this.#context.href, 0, this.#context.protocol_end);
  }

  set protocol(value) {
    const href = bindingUrl.update(this.#context.href, updateActions.kProtocol, `${value}`);
    if (href) {
      this.#updateContext(href);
    }
  }

  get username() {
    if (this.#context.protocol_end + 2 < this.#context.username_end) {
      return StringPrototypeSlice(this.#context.href, this.#context.protocol_end + 2, this.#context.username_end);
    }
    return '';
  }

  set username(value) {
    const href = bindingUrl.update(this.#context.href, updateActions.kUsername, `${value}`);
    if (href) {
      this.#updateContext(href);
    }
  }

  get password() {
    if (this.#context.host_start - this.#context.username_end > 0) {
      return StringPrototypeSlice(this.#context.href, this.#context.username_end + 1, this.#context.host_start);
    }
    return '';
  }

  set password(value) {
    const href = bindingUrl.update(this.#context.href, updateActions.kPassword, `${value}`);
    if (href) {
      this.#updateContext(href);
    }
  }

  get host() {
    let startsAt = this.#context.host_start;
    if (this.#context.href[startsAt] === '@') {
      startsAt++;
    }
    if (startsAt === this.#context.host_end) {
      return '';
    }
    return StringPrototypeSlice(this.#context.href, startsAt, this.#context.pathname_start);
  }

  set host(value) {
    const href = bindingUrl.update(this.#context.href, updateActions.kHost, `${value}`);
    if (href) {
      this.#updateContext(href);
    }
  }

  get hostname() {
    let startsAt = this.#context.host_start;
    if (this.#context.href[startsAt] === '@') {
      startsAt++;
    }
    return StringPrototypeSlice(this.#context.href, startsAt, this.#context.host_end);
  }

  set hostname(value) {
    const href = bindingUrl.update(this.#context.href, updateActions.kHostname, `${value}`);
    if (href) {
      this.#updateContext(href);
    }
  }

  get port() {
    if (this.#context.hasPort) {
      return `${this.#context.port}`;
    }
    return '';
  }

  set port(value) {
    const href = bindingUrl.update(this.#context.href, updateActions.kPort, `${value}`);
    if (href) {
      this.#updateContext(href);
    }
  }

  get pathname() {
    let endsAt;
    if (this.#context.hasSearch) {
      endsAt = this.#context.search_start;
    } else if (this.#context.hasHash) {
      endsAt = this.#context.hash_start;
    }
    return StringPrototypeSlice(this.#context.href, this.#context.pathname_start, endsAt);
  }

  set pathname(value) {
    const href = bindingUrl.update(this.#context.href, updateActions.kPathname, `${value}`);
    if (href) {
      this.#updateContext(href);
    }
  }

  get search() {
    this.#ensureSearchParamsUpdated();
    return this.#getSearchFromContext();
  }

  set search(value) {
    const href = bindingUrl.update(this.#context.href, updateActions.kSearch, StringPrototypeToWellFormed(`${value}`));
    if (href) {
      this.#updateContext(href, true);
    }
  }

  get searchParams() {
    if (this.#searchParams == null) {
      this.#searchParams = new URLSearchParams(this.#getSearchFromContext());
      hooks.setURLSearchParamsContext(this.#searchParams, this);
      this.#searchParamsModified = false;
    }
    return this.#searchParams;
  }

  get hash() {
    if (!this.#context.hasHash || (this.#context.href.length - this.#context.hash_start <= 1)) {
      return '';
    }
    return StringPrototypeSlice(this.#context.href, this.#context.hash_start);
  }

  set hash(value) {
    const href = bindingUrl.update(this.#context.href, updateActions.kHash, `${value}`);
    if (href) {
      this.#updateContext(href);
    }
  }

  toJSON() {
    this.#ensureSearchParamsUpdated();
    return this.#context.href;
  }

  static canParse(url, base = undefined) {
    if (arguments.length === 0) {
      throw new ERR_MISSING_ARGS('url');
    }

    if (typeof url !== 'string') {
      url = `${url}`;
    }

    if (base !== undefined) {
      return bindingUrl.canParse(url, typeof base === 'string' ? base : `${base}`);
    }

    return bindingUrl.canParse(url);
  }
}

ObjectDefineProperties(URL.prototype, {
  [SymbolToStringTag]: { __proto__: null, configurable: true, value: 'URL' },
  toString: kEnumerableProperty,
  href: kEnumerableProperty,
  origin: kEnumerableProperty,
  protocol: kEnumerableProperty,
  username: kEnumerableProperty,
  password: kEnumerableProperty,
  host: kEnumerableProperty,
  hostname: kEnumerableProperty,
  port: kEnumerableProperty,
  pathname: kEnumerableProperty,
  search: kEnumerableProperty,
  searchParams: kEnumerableProperty,
  hash: kEnumerableProperty,
  toJSON: kEnumerableProperty,
});

ObjectDefineProperties(URL, {
  canParse: {
    __proto__: null,
    configurable: true,
    writable: true,
    enumerable: true,
  },
  parse: {
    __proto__: null,
    configurable: true,
    writable: true,
    enumerable: true,
  },
});


module.exports = {
  URL,
  kCreateURLFromPosixPathSymbol,
  kCreateURLFromWindowsPathSymbol,
  kParseURLSymbol,
  updateActions,
};
