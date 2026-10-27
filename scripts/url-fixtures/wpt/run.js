// Runs the vendored web-platform-tests URL data (see README.md) the way the
// WPT harness files do and prints one line per failing vector plus a digest.
// Node and `jsse --node` must print the same bytes, which are pinned in
// expected.output.
"use strict";
const fs = require("fs");
const path = require("path");
const crypto = require("crypto");

const dir = __dirname;
const manifest = JSON.parse(fs.readFileSync(path.join(dir, "manifest.json"), "utf8"));
const sha256 = (buf) => crypto.createHash("sha256").update(buf).digest("hex");

function load(file) {
  const parts = file.parts.map((p) => {
    const buf = fs.readFileSync(path.join(dir, p.name));
    if (sha256(buf) !== p.sha256) throw new Error("bad sha256 for " + p.name);
    return buf;
  });
  const whole = Buffer.concat(parts);
  if (whole.length !== file.bytes || sha256(whole) !== file.sha256) {
    throw new Error("bad sha256 for " + file.name);
  }
  return JSON.parse(whole.toString("utf8"));
}
const data = {};
for (const file of manifest.files) data[file.name] = load(file);

let hash = 0x811c9dc5;
const lines = [];
const counts = {};
function emit(line) {
  lines.push(line);
  for (let i = 0; i < line.length; i++) {
    hash = Math.imul(hash ^ line.charCodeAt(i), 0x01000193) >>> 0;
  }
  hash = Math.imul(hash ^ 10, 0x01000193) >>> 0;
}

function describe(e) {
  return e && e.name ? e.name + (e.code ? " " + e.code : "") : String(e);
}
function check(suite, label, fn) {
  const c = (counts[suite] ||= { total: 0, failed: 0 });
  c.total++;
  let problem = null;
  try {
    problem = fn();
  } catch (e) {
    problem = "threw " + describe(e);
  }
  if (problem) {
    c.failed++;
    emit("FAIL " + suite + " " + JSON.stringify(label) + ": " + problem);
  }
}
function same(field, got, want) {
  return got === want ? null : field + " " + JSON.stringify(got) + " != " + JSON.stringify(want);
}
function mustThrowTypeError(fn) {
  try {
    fn();
  } catch (e) {
    return e instanceof TypeError ? null : "threw " + describe(e) + ", not TypeError";
  }
  return "did not throw";
}
function firstProblem(pairs) {
  for (const [field, got, want] of pairs) {
    const p = same(field, got, want);
    if (p) return p;
  }
  return null;
}

const FIELDS = ["href", "protocol", "username", "password", "host", "hostname", "port", "pathname", "search", "hash"];

function constructorSuite(name, tests) {
  tests.forEach((t, i) => {
    if (typeof t === "string") return;
    const base = t.base !== null ? t.base : undefined;
    const label = i + " " + t.input + " | " + t.base;
    check(name + "/constructor", label, () => {
      if (t.failure) return mustThrowTypeError(() => new URL(t.input, base));
      const url = new URL(t.input, base);
      const pairs = FIELDS.map((f) => [f, url[f], t[f]]);
      if ("origin" in t) pairs.push(["origin", url.origin, t.origin]);
      if ("searchParams" in t) pairs.push(["searchParams", url.searchParams.toString(), t.searchParams]);
      return firstProblem(pairs);
    });
    check(name + "/canParse", label, () => {
      return same("canParse", URL.canParse(t.input, base), !t.failure);
    });
    check(name + "/parse", label, () => {
      const url = URL.parse(t.input, base);
      if (t.failure) return same("parse", url, null);
      return url === null ? "parse returned null" : same("href", url.href, t.href);
    });
    if (base !== undefined) {
      check(name + "/urlBase", label, () => {
        let baseUrl;
        try {
          baseUrl = new URL(base);
        } catch (e) {
          return null;
        }
        if (t.failure) return mustThrowTypeError(() => new URL(t.input, baseUrl));
        return same("href", new URL(t.input, baseUrl).href, t.href);
      });
    }
  });
}

function settersSuite(name, tests) {
  for (const attribute of Object.keys(tests)) {
    if (attribute === "comment") continue;
    tests[attribute].forEach((t, i) => {
      const label = attribute + " " + i + " " + t.comment;
      check(name, label, () => {
        const url = new URL(t.href);
        url[attribute] = t.new_value;
        return firstProblem(Object.keys(t.expected).map((k) => [k, url[k], t.expected[k]]));
      });
    });
  }
}

function toasciiSuite(name, tests) {
  tests.forEach((t, i) => {
    // An empty expected host is no host at all, which https:// URLs reject.
    if (typeof t === "string" || t.output === "") return;
    const label = i + " " + t.input;
    check(name + "/constructor", label, () => {
      if (t.output === null) return mustThrowTypeError(() => new URL("https://" + t.input + "/x"));
      const url = new URL("https://" + t.input + "/x");
      return firstProblem([
        ["host", url.host, t.output],
        ["hostname", url.hostname, t.output],
        ["pathname", url.pathname, "/x"],
        ["href", url.href, "https://" + t.output + "/x"],
      ]);
    });
    for (const attribute of ["host", "hostname"]) {
      check(name + "/" + attribute, label, () => {
        const url = new URL("https://x/x");
        url[attribute] = t.input;
        return same(attribute, url[attribute], t.output === null ? "x" : t.output);
      });
    }
  });
}

constructorSuite("urltestdata", data["urltestdata.json"]);
constructorSuite("urltestdata-javascript-only", data["urltestdata-javascript-only.json"]);
settersSuite("setters", data["setters_tests.json"]);
toasciiSuite("toascii", data["toascii.json"]);
toasciiSuite("IdnaTestV2", data["IdnaTestV2.json"]);

for (const suite of Object.keys(counts).sort()) {
  emit(suite + ": " + counts[suite].total + " checks, " + counts[suite].failed + " failed");
}
for (const line of lines) console.log(line);
console.log("wpt-digest " + hash.toString(16).padStart(8, "0"));
