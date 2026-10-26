"use strict";
// Deterministic probe of the `os` module, run unchanged by Node and by
// `jsse --node` (scripts/run-node-os-oracle.sh diffs the two outputs).
const os = require("os");
const scenario = process.argv[2] || "default";

function emit(label, value) {
  console.log(label + " " + JSON.stringify(value));
}

function failure(fn) {
  try {
    fn();
    return "no throw";
  } catch (e) {
    const info = e.info;
    return [e.name, e.code, e.message, e instanceof Error, Object.keys(e), info && Object.keys(info), e.errno, e.syscall, String(e)];
  }
}

function descriptors(object) {
  const out = {};
  for (const key of Reflect.ownKeys(object)) {
    const d = Object.getOwnPropertyDescriptor(object, key);
    out[String(key)] = [d.writable, d.enumerable, d.configurable, "value" in d ? d.value : "accessor"];
  }
  return out;
}

if (scenario === "edit") {
  process.env.HOME = "/edited/home";
  emit("homedir-edited", os.homedir());
  delete process.env.HOME;
  emit("homedir-deleted-is-passwd", os.homedir() === os.userInfo().homedir);
  process.env.TMPDIR = "/edited/tmp/";
  emit("tmpdir-edited", os.tmpdir());
  process.env.TMPDIR = "";
  process.env.TMP = "";
  process.env.TEMP = "/edited/temp";
  emit("tmpdir-temp", os.tmpdir());
  process.exit(0);
}

emit("keys", Object.keys(os));
emit("own-keys", Reflect.ownKeys(os));
const surface = {};
for (const key of Object.keys(os)) {
  const v = os[key];
  if (typeof v !== "function") continue;
  surface[key] = [v.name, v.length, Reflect.ownKeys(v).map(String), typeof v[Symbol.toPrimitive], typeof v.prototype];
}
emit("functions", surface);
emit("platform", [os.platform(), `${os.platform}`, os.arch(), `${os.arch}`, os.type(), `${os.type}`, os.machine(), `${os.machine}`]);
emit("uname", [os.release(), os.version(), `${os.release}`, `${os.version}`]);
emit("hostname", [os.hostname(), `${os.hostname}`]);
emit("endianness", [os.endianness(), `${os.endianness}`]);
emit("eol-devnull", [os.EOL, os.devNull, descriptors(os).EOL, descriptors(os).devNull]);
emit("homedir", [os.homedir(), `${os.homedir}`]);
emit("tmpdir", [os.tmpdir(), `${os.tmpdir}`]);
emit("totalmem", [os.totalmem(), `${os.totalmem}`.length > 0]);
emit("parallelism", [os.availableParallelism(), `${os.availableParallelism}`]);
emit("userInfo", os.userInfo());
emit("userInfo-null", os.userInfo(null));
emit("userInfo-options", [os.userInfo({}), os.userInfo(5), os.userInfo("buffer")]);
const buf = os.userInfo({ encoding: "buffer" });
emit("userInfo-buffer", [buf.uid, buf.gid, Buffer.isBuffer(buf.username), Buffer.isBuffer(buf.homedir), buf.shell === null || Buffer.isBuffer(buf.shell), String(buf.username), String(buf.homedir), String(buf.shell)]);
emit("networkInterfaces", os.networkInterfaces());
emit("constants-shape", [Object.keys(os.constants), Object.getPrototypeOf(os.constants), descriptors(os.constants).UV_UDP_REUSEADDR, descriptors(os).constants]);
emit("constants-frozen", ["signals", "errno", "priority", "dlopen"].map((k) => [k, Object.isFrozen(os.constants[k]), Object.getPrototypeOf(os.constants[k]), Object.isExtensible(os.constants[k])]));
emit("constants-descriptors", { dlopen: descriptors(os.constants.dlopen), errno: descriptors(os.constants.errno), signals: descriptors(os.constants.signals), priority: descriptors(os.constants.priority), outer: descriptors(os.constants) });
emit("priority", [os.getPriority(), os.getPriority(0), os.getPriority(undefined)]);
emit("priority-errors", [
  failure(() => os.getPriority("a")),
  failure(() => os.getPriority(1.5)),
  failure(() => os.getPriority(2 ** 31)),
  failure(() => os.getPriority(99999999)),
  failure(() => os.setPriority("a")),
  failure(() => os.setPriority(1, 100)),
  failure(() => os.setPriority(0, 1.5)),
  failure(() => os.setPriority(0, -21)),
  failure(() => os.setPriority(99999999, 5)),
  failure(() => os.setPriority(2 ** 33)),
]);
const nice = os.getPriority();
os.setPriority(nice);
emit("priority-set-same", os.getPriority() === nice);

const mem = [os.freemem(), os.totalmem()];
emit("freemem-shape", [typeof mem[0], Number.isInteger(mem[0]), mem[0] > 0, mem[0] <= mem[1], `${os.freemem}` > 0]);
const up = os.uptime();
emit("uptime-shape", [typeof up, up > 0, Number.isFinite(up), `${os.uptime}` > 0]);
const avg = os.loadavg();
emit("loadavg-shape", [Array.isArray(avg), avg.length, avg.every((n) => typeof n === "number" && n >= 0), os.loadavg() !== avg]);
const cpus = os.cpus();
emit("cpus-count", [cpus.length, cpus.length === os.availableParallelism()]);
emit("cpus-shape", [
  cpus.map((c) => c.model),
  cpus.map((c) => Object.keys(c)),
  cpus.map((c) => typeof c.speed === "number" && c.speed >= 0),
  cpus.map((c) => Object.keys(c.times)),
  cpus.every((c) => Object.values(c.times).every((n) => Number.isInteger(n) && n >= 0)),
]);
