// The global TextDecoder for the Node prelude embedded in `--node` (#1527),
// shaped after Node's lib/internal/encoding.js: a class holding its state in
// symbol-keyed slots, Node's argument validation, error codes and inspect
// output. The decoders are the ones node-buffer-shim.js leaves in
// `globalThis.__node_decoders`. internal/errors, internal/validators and
// internal/util/inspect are loaded through the handoff that
// node-builtin-loader.js completes, only by a call that throws or inspects.

(function () {
  "use strict";

  var decoders = globalThis.__node_decoders;
  if (decoders === undefined) return;
  delete globalThis.__node_decoders;

  var handoffKey = Symbol.for("jsse.node.textdecoder");
  var handoff = { require: undefined };
  Object.defineProperty(globalThis, handoffKey, {
    value: handoff,
    configurable: true,
  });

  function internal(id) {
    return handoff.require(id);
  }

  var uncurryThis = Function.prototype.bind.bind(Function.prototype.call);
  var U8 = Uint8Array;
  var u8Set = uncurryThis(U8.prototype.set);
  var u8Subarray = uncurryThis(U8.prototype.subarray);
  var charCodeAt = uncurryThis(String.prototype.charCodeAt);
  var strSlice = uncurryThis(String.prototype.slice);
  var toLowerCase = uncurryThis(String.prototype.toLowerCase);
  var getOwnPropertyDescriptor = Object.getOwnPropertyDescriptor;
  var getPrototypeOf = Object.getPrototypeOf;

  function getter(proto, name) {
    return uncurryThis(getOwnPropertyDescriptor(proto, name).get);
  }
  var TypedArrayProto = getPrototypeOf(U8).prototype;
  var taTag = getter(TypedArrayProto, Symbol.toStringTag);
  var taBuffer = getter(TypedArrayProto, "buffer");
  var taOffset = getter(TypedArrayProto, "byteOffset");
  var taBytes = getter(TypedArrayProto, "byteLength");
  var dvBuffer = getter(DataView.prototype, "buffer");
  var dvOffset = getter(DataView.prototype, "byteOffset");
  var dvBytes = getter(DataView.prototype, "byteLength");
  var abBytes = getter(ArrayBuffer.prototype, "byteLength");
  var sabBytes = getter(SharedArrayBuffer.prototype, "byteLength");

  var kDecoder = Symbol("decoder");
  var kFlags = Symbol("flags");
  var kEncoding = Symbol("encoding");
  var kIgnoreBOM = Symbol("kIgnoreBOM");
  var kFatal = Symbol("kFatal");
  var kUTF8FastPath = Symbol("kUTF8FastPath");
  var kHandle = Symbol("handle");
  var kSingleByte = Symbol("single-byte");
  var kChunk = Symbol("chunk");
  var kBOMSeen = Symbol("BOM seen");
  var inspectCustom = Symbol.for("nodejs.util.inspect.custom");

  // Node's kValidateObjectAllowObjectsAndNull: nullable | array | function.
  var ALLOW_OBJECTS_AND_NULL = 7;
  var FLAGS_FATAL = 0x2;
  var FLAGS_IGNORE_BOM = 0x4;

  var empty = new U8(0);
  var kEmptyObject = Object.freeze({ __proto__: null });

  var LABELS = Object.create(null);
  [
    ["utf-8", "unicode-1-1-utf-8 unicode11utf8 unicode20utf8 utf-8 utf8 x-unicode20utf8"],
    [
      "windows-1252",
      "ansi_x3.4-1968 ascii cp1252 cp819 csisolatin1 ibm819 iso-8859-1 iso-ir-100 " +
        "iso8859-1 iso88591 iso_8859-1 iso_8859-1:1987 l1 latin1 us-ascii windows-1252 x-cp1252",
    ],
    ["utf-16le", "csunicode iso-10646-ucs-2 ucs-2 unicode unicodefeff utf-16 utf-16le"],
    ["utf-16be", "unicodefffe utf-16be"],
  ].forEach(function (entry) {
    entry[1].split(" ").forEach(function (label) {
      LABELS[label] = entry[0];
    });
  });

  function isAsciiSpace(c) {
    return c === 0x09 || c === 0x0a || c === 0x0c || c === 0x0d || c === 0x20;
  }

  function getEncodingFromLabel(label) {
    var enc = LABELS[label];
    if (enc !== undefined) return enc;
    var start = 0;
    var end = label.length;
    while (start < end && isAsciiSpace(charCodeAt(label, start))) start++;
    while (end > start && isAsciiSpace(charCodeAt(label, end - 1))) end--;
    return LABELS[toLowerCase(strSlice(label, start, end))];
  }

  function validateDecoder(obj) {
    if (obj == null || obj[kDecoder] !== true) {
      throw new (internal("internal/errors").codes.ERR_INVALID_THIS)("TextDecoder");
    }
  }

  // Node's native decode paths throw a plain TypeError carrying the code.
  function nativeArgError(message) {
    var e = new TypeError(message);
    e.code = "ERR_INVALID_ARG_TYPE";
    return e;
  }

  function isBranded(get, value) {
    try {
      get(value);
      return true;
    } catch (e) {
      return false;
    }
  }

  // The bytes of an ArrayBuffer, SharedArrayBuffer or view, read through the
  // intrinsic accessors; a detached or out-of-bounds source is empty.
  function toBytes(input) {
    if (input === null || typeof input !== "object") return undefined;
    var buffer = input;
    var offset = 0;
    var length;
    try {
      if (taTag(input) !== undefined) {
        buffer = taBuffer(input);
        offset = taOffset(input);
        length = taBytes(input);
      } else if (isBranded(dvBuffer, input)) {
        buffer = dvBuffer(input);
        offset = dvOffset(input);
        length = dvBytes(input);
      } else if (isBranded(abBytes, input)) {
        length = abBytes(input);
      } else if (isBranded(sabBytes, input)) {
        length = sabBytes(input);
      } else {
        return undefined;
      }
      return new U8(buffer, offset, length);
    } catch (e) {
      return empty;
    }
  }

  var SHARED_LIST =
    "The \"list\" argument must be an instance of SharedArrayBuffer, ArrayBuffer or ArrayBufferView.";
  var SHARED_INPUT =
    "The \"input\" argument must be an instance of SharedArrayBuffer, ArrayBuffer or ArrayBufferView.";

  function badInput(enc, utf8Binding, input) {
    if (enc === "utf-16le" || enc === "utf-16be") {
      return nativeArgError(SHARED_INPUT);
    }
    if (enc === "utf-8" && utf8Binding) return nativeArgError(SHARED_LIST);
    return new (internal("internal/errors").codes.ERR_INVALID_ARG_TYPE)(
      "input",
      ["ArrayBuffer", "ArrayBufferView"],
      input
    );
  }

  function getConstructorOf(obj) {
    while (obj) {
      var descriptor = getOwnPropertyDescriptor(obj, "constructor");
      if (
        descriptor !== undefined &&
        typeof descriptor.value === "function" &&
        descriptor.value.name !== ""
      ) {
        return descriptor.value;
      }
      obj = getPrototypeOf(obj);
    }
    return null;
  }

  function checkOptions(options) {
    if (options === null) return;
    var type = typeof options;
    if (type !== "object" && type !== "function") {
      internal("internal/validators").validateObject(
        options,
        "options",
        ALLOW_OBJECTS_AND_NULL
      );
    }
  }

  class TextDecoder {
    constructor(encoding = "utf-8", options = kEmptyObject) {
      encoding = `${encoding}`;
      checkOptions(options);
      var enc = getEncodingFromLabel(encoding);
      if (enc === undefined) {
        throw new (internal("internal/errors").codes.ERR_ENCODING_NOT_SUPPORTED)(
          encoding
        );
      }
      var flags = 0;
      if (options !== null) {
        flags |= options.fatal ? FLAGS_FATAL : 0;
        flags |= options.ignoreBOM ? FLAGS_IGNORE_BOM : 0;
      }
      this[kDecoder] = true;
      this[kFlags] = flags;
      this[kEncoding] = enc;
      this[kIgnoreBOM] = (flags & FLAGS_IGNORE_BOM) !== 0;
      this[kFatal] = (flags & FLAGS_FATAL) !== 0;
      this[kUTF8FastPath] = enc === "utf-8";
      this[kHandle] = undefined;
      this[kSingleByte] = enc === "windows-1252" ? decoders.latin : undefined;
      this[kChunk] = null;
      this[kBOMSeen] = false;
    }

    decode(input = empty, options = kEmptyObject) {
      validateDecoder(this);
      checkOptions(options);
      var stream = options?.stream;
      var enc = this[kEncoding];
      var pending = this[kChunk];
      var bytes = toBytes(input);
      if (bytes === undefined) {
        throw badInput(enc, this[kUTF8FastPath] && !stream && !pending, input);
      }
      if (this[kSingleByte]) return this[kSingleByte](bytes);
      var ignoreBOM = this[kIgnoreBOM] || this[kBOMSeen];
      if (!stream) this[kBOMSeen] = false;
      if (pending) {
        var joined = new U8(pending.length + bytes.length);
        u8Set(joined, pending, 0);
        u8Set(joined, bytes, pending.length);
        bytes = joined;
        this[kChunk] = null;
      }
      var fatal = this[kFatal];
      var result = this[kUTF8FastPath]
        ? decoders.utf8(bytes, fatal, ignoreBOM, stream)
        : decoders.utf16(bytes, enc === "utf-16be", fatal, ignoreBOM, stream);
      if (stream) {
        var tail = result.pending;
        if (bytes.length - tail > 0) this[kBOMSeen] = true;
        if (tail > 0) {
          this[kChunk] = new U8(u8Subarray(bytes, bytes.length - tail));
        }
      }
      return result.out;
    }
  }

  var shared = {
    get encoding() {
      validateDecoder(this);
      return this[kEncoding];
    },
    get fatal() {
      validateDecoder(this);
      return (this[kFlags] & FLAGS_FATAL) === FLAGS_FATAL;
    },
    get ignoreBOM() {
      validateDecoder(this);
      return (this[kFlags] & FLAGS_IGNORE_BOM) === FLAGS_IGNORE_BOM;
    },
    [inspectCustom](depth, opts) {
      validateDecoder(this);
      if (typeof depth === "number" && depth < 0) return this;
      var constructor = getConstructorOf(this) || TextDecoder;
      var obj = { __proto__: { constructor: constructor } };
      obj.encoding = this.encoding;
      obj.fatal = this.fatal;
      obj.ignoreBOM = this.ignoreBOM;
      if (opts.showHidden) {
        obj[kFlags] = this[kFlags];
        obj[kHandle] = this[kHandle];
      }
      return `${constructor.name} ${internal("internal/util/inspect").inspect(obj)}`;
    },
  };
  var descriptors = Object.getOwnPropertyDescriptors(shared);
  Object.values(descriptors).forEach(function (d) {
    Object.setPrototypeOf(d, null);
  });
  descriptors[inspectCustom].enumerable = false;

  Object.defineProperties(TextDecoder.prototype, {
    decode: { __proto__: null, enumerable: true },
    ...descriptors,
    [Symbol.toStringTag]: {
      __proto__: null,
      configurable: true,
      value: "TextDecoder",
    },
  });

  globalThis.TextDecoder = TextDecoder;
})();
