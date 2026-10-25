// HTML host globals btoa/atob (jsse#1188): Latin-1 only on the way in, the
// Infra "forgiving-base64" decode on the way out, InvalidCharacterError
// (code 5) for anything outside either.

function assert(cond, msg) {
  if (!cond) throw new Error(msg);
}

function throwsDom(fn, name, code, msg) {
  try {
    fn();
  } catch (e) {
    assert(e instanceof Error, msg + ": must be an Error");
    assert(e.name === name && e.code === code, msg + ": got " + e.name + "/" + e.code);
    return;
  }
  throw new Error(msg + ": did not throw");
}

function throwsType(fn, msg) {
  try {
    fn();
  } catch (e) {
    assert(e instanceof TypeError, msg + ": must be a TypeError, got " + e);
    return;
  }
  throw new Error(msg + ": did not throw");
}

assert(typeof btoa === "function" && btoa.length === 1, "btoa shape");
assert(typeof atob === "function" && atob.length === 1, "atob shape");

assert(btoa("") === "", "empty");
assert(btoa("f") === "Zg==", "one byte pads twice");
assert(btoa("fo") === "Zm8=", "two bytes pad once");
assert(btoa("foo") === "Zm9v", "three bytes need no padding");
assert(btoa("\xff\xfe\x00") === "//4A", "Latin-1 range is byte-exact");
assert(btoa(undefined) === "dW5kZWZpbmVk", "arguments go through ToString");
assert(btoa(12) === "MTI=", "numbers go through ToString");
throwsDom(() => btoa("Ā"), "InvalidCharacterError", 5, "btoa above U+00FF");
throwsDom(() => btoa("a\u{1F600}"), "InvalidCharacterError", 5, "btoa astral");
throwsType(() => btoa(), "btoa without an argument");
throwsType(() => btoa(Symbol()), "btoa of a Symbol");

assert(atob("") === "", "atob empty");
assert(atob("Zg==") === "f", "padded");
assert(atob("Zg") === "f", "unpadded");
assert(atob("Zm8") === "fo", "unpadded two bytes");
assert(atob("//4A") === "\xff\xfe\x00", "bytes above 0x7F");
assert(atob(" Z\tm\n9\rv\f") === "foo", "ASCII whitespace is stripped");
assert(atob("YR==") === "a", "non-zero trailing bits are discarded");
assert(atob("YQ==") === "a" && atob("YWI=") === "ab", "padding on an aligned length");
assert(atob("YWJj ZGVm") === "abcdef", "whitespace between groups");
throwsDom(() => atob("\v"), "InvalidCharacterError", 5, "VT is not whitespace");
throwsDom(() => atob("YQ="), "InvalidCharacterError", 5, "= on an unaligned length");
throwsDom(() => atob("a"), "InvalidCharacterError", 5, "length % 4 == 1");
throwsDom(() => atob("YQ==YQ=="), "InvalidCharacterError", 5, "= inside the data");
throwsDom(() => atob("===="), "InvalidCharacterError", 5, "only padding");
throwsDom(() => atob("YQ==="), "InvalidCharacterError", 5, "too much padding");
throwsDom(() => atob("a=bc"), "InvalidCharacterError", 5, "= before data");
throwsDom(() => atob("-_"), "InvalidCharacterError", 5, "URL alphabet");
throwsDom(() => atob("éAAA"), "InvalidCharacterError", 5, "non-ASCII");
throwsType(() => atob(), "atob without an argument");

let all = "";
for (let i = 0; i < 256; i++) all += String.fromCharCode(i);
assert(atob(btoa(all)) === all, "all 256 byte values round-trip");
