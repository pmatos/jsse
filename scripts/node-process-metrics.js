// Node host-compat `process` metrics (issue #1227): platform, arch, pid, ppid,
// title, version(s), release, features, config, uptime, hrtime, memoryUsage,
// cpuUsage, threadCpuUsage, resourceUsage and umask. Loaded AFTER
// node-process-shim.js (see node_prelude.rs). The queries are the native
// `__host_process` binding, which stays a global only until the builtin loader
// takes it private as `internal/host/process`; the argument checks live in
// node-lib/internal/process/per_thread.js and load on first use. A no-op unless
// the floor primitives exist. See docs/adr/2026-10-26-1930-node-host-perf.md.

(function () {
  "use strict";

  if (
    typeof __host_process === "undefined" ||
    typeof globalThis.process !== "object" ||
    globalThis.process === null
  ) {
    return;
  }

  var host = __host_process;
  var proc = globalThis.process;
  var info = host.info();

  // The Node release whose lib/ the embedded modules are derived from.
  var NODE_VERSION = "26.8.2";

  function perThread() {
    return host.load("internal/process/per_thread");
  }

  function define(name, value, writable, configurable) {
    Object.defineProperty(proc, name, {
      value: value,
      writable: writable,
      enumerable: true,
      configurable: configurable,
    });
  }

  function deepFreeze(o) {
    Object.keys(o).forEach(function (k) {
      if (typeof o[k] === "object" && o[k] !== null) deepFreeze(o[k]);
    });
    return Object.freeze(o);
  }

  define("platform", info[2], false, true);
  define("arch", info[3], false, true);
  define("pid", info[0], false, true);
  define("ppid", info[1], true, true);
  define("version", "v" + NODE_VERSION, false, true);
  define("versions", { node: NODE_VERSION, jsse: info[5] }, false, true);
  define(
    "release",
    {
      name: "node",
      sourceUrl:
        "https://nodejs.org/download/release/v" + NODE_VERSION +
        "/node-v" + NODE_VERSION + ".tar.gz",
      headersUrl:
        "https://nodejs.org/download/release/v" + NODE_VERSION +
        "/node-v" + NODE_VERSION + "-headers.tar.gz",
    },
    false,
    true
  );
  define(
    "features",
    {
      inspector: false,
      debug: false,
      uv: false,
      ipv6: false,
      tls_alpn: false,
      tls_sni: false,
      tls_ocsp: false,
      tls: false,
      openssl_is_boringssl: false,
      cached_builtins: false,
      require_module: true,
      quic: false,
      typescript: false,
    },
    false,
    false
  );
  define(
    "config",
    deepFreeze({
      variables: {
        host_arch: info[3],
        napi_build_version: "0",
        node_module_version: 0,
        node_shared: false,
        node_use_openssl: false,
        target_arch: info[3],
      },
      target_defaults: {
        cflags: [],
        default_configuration: "Release",
        defines: [],
        include_dirs: [],
        libraries: [],
      },
    }),
    false,
    true
  );

  define("title", info[4], true, true);

  var baseHrtime = proc.hrtime;
  var hrtimeBigint = baseHrtime.bigint;
  Object.defineProperty(hrtimeBigint, "name", { value: "hrtimeBigInt" });
  var hrtime = function hrtime(time) {
    if (time !== undefined) perThread().validateHrtime(time);
    return baseHrtime(time);
  };
  hrtime.bigint = hrtimeBigint;
  proc.hrtime = hrtime;

  proc.uptime = function uptime() {
    return Number(hrtimeBigint()) / 1e9;
  };
  proc.cpuUsage = function cpuUsage(prevValue) {
    return perThread().cpuUsage(prevValue);
  };
  proc.threadCpuUsage = function threadCpuUsage(prevValue) {
    return perThread().cpuUsage(prevValue);
  };
  proc.resourceUsage = function resourceUsage() {
    return perThread().resourceUsage();
  };
  var memoryUsage = function memoryUsage() {
    return perThread().memoryUsage();
  };
  memoryUsage.rss = function rss() {
    return host.rss();
  };
  proc.memoryUsage = memoryUsage;
  proc.umask = function wrappedUmask(mask) {
    return perThread().umask(mask);
  };
  if (proc.execArgv === undefined) proc.execArgv = [];
  if (proc.debugPort === undefined) proc.debugPort = 9229;
})();
