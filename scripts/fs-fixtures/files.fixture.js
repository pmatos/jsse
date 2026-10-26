const fs = typeof require === "function" ? require("fs") : __node_builtins.fs;
const D = process.env.FS_ORACLE_DIR;
if (!D) throw new Error("FS_ORACLE_DIR is not set");
const norm = (s) => String(s).split(D).join("<D>");
function show(v) {
  if (v === undefined) return "undefined";
  if (Buffer.isBuffer(v)) return "buf:" + v.toString("hex");
  if (typeof v === "string") return JSON.stringify(v);
  if (Array.isArray(v)) return "[" + v.map(show).join(",") + "]";
  return String(typeof v === "object" ? JSON.stringify(v) : v);
}
function t(label, fn) {
  try {
    console.log(label, "=>", norm(show(fn())));
  } catch (e) {
    console.log(label, "!!", e.name, e.code, e.errno, e.syscall, norm(e.path), norm(e.dest), norm(e.message));
  }
}
process.noDeprecation = true;
const p = (name) => D + "/" + name;
// #1217: readFileSync/writeFileSync/appendFileSync/existsSync and descriptors.
t("write utf8", () => fs.writeFileSync(p("a.txt"), "héllo wörld €\n"));
t("read buffer", () => fs.readFileSync(p("a.txt")));
t("read utf8", () => fs.readFileSync(p("a.txt"), "utf8"));
t("read {encoding}", () => fs.readFileSync(p("a.txt"), { encoding: "utf-8" }));
t("read hex", () => fs.readFileSync(p("a.txt"), "hex"));
t("read base64", () => fs.readFileSync(p("a.txt"), "base64"));
t("read latin1", () => fs.readFileSync(p("a.txt"), "latin1"));
t("read ucs2 odd", () => fs.readFileSync(p("a.txt"), "ucs2").length);
t("read buffer enc", () => fs.readFileSync(p("a.txt"), { encoding: "buffer" }));
t("read Buffer path", () => fs.readFileSync(Buffer.from(p("a.txt")), "utf8"));
t("read Uint8Array path", () => fs.readFileSync(new Uint8Array(Buffer.from(p("a.txt"))), "utf8"));
t("read URL path", () => {
  const u = typeof URL === "function"
    ? new URL("file://" + p("a.txt"))
    : { href: "file://" + p("a.txt"), protocol: "file:", hostname: "", pathname: p("a.txt") };
  return fs.readFileSync(u, "utf8");
});
t("write buffer", () => fs.writeFileSync(p("b.bin"), Buffer.from([0, 1, 2, 255, 254])));
t("read b.bin", () => fs.readFileSync(p("b.bin")));
t("write typed array", () => fs.writeFileSync(p("c.bin"), new Uint16Array([0x0102, 0x0304])));
t("read c.bin", () => fs.readFileSync(p("c.bin")));
t("write DataView", () => fs.writeFileSync(p("d.bin"), new DataView(new Uint8Array([9, 8, 7]).buffer)));
t("read d.bin", () => fs.readFileSync(p("d.bin")));
t("write hex enc", () => fs.writeFileSync(p("e.bin"), "00ff10", "hex"));
t("read e.bin", () => fs.readFileSync(p("e.bin")));
t("write {encoding:base64}", () => fs.writeFileSync(p("f.bin"), "aGk=", { encoding: "base64" }));
t("read f.bin", () => fs.readFileSync(p("f.bin"), "utf8"));
t("write empty", () => fs.writeFileSync(p("empty"), ""));
t("read empty", () => fs.readFileSync(p("empty")));
t("write flag wx exists", () => fs.writeFileSync(p("a.txt"), "x", { flag: "wx" }));
t("write flag wx new", () => fs.writeFileSync(p("g.txt"), "x", { flag: "wx" }));
t("append", () => { fs.appendFileSync(p("g.txt"), "yz"); fs.appendFileSync(p("g.txt"), Buffer.from("!")); return fs.readFileSync(p("g.txt"), "utf8"); });
t("append new", () => { fs.appendFileSync(p("h.txt"), "new"); return fs.readFileSync(p("h.txt"), "utf8"); });
t("write mode", () => { fs.writeFileSync(p("m.txt"), "m", { mode: 0o600 }); return (fs.statSync(p("m.txt")).mode & 0o777).toString(8); });
t("write mode string", () => { fs.writeFileSync(p("m2.txt"), "m", { mode: "640" }); return (fs.statSync(p("m2.txt")).mode & 0o777).toString(8); });
t("write default mode", () => { fs.writeFileSync(p("m3.txt"), "m"); return (fs.statSync(p("m3.txt")).mode & 0o777).toString(8); });
t("exists", () => [fs.existsSync(p("a.txt")), fs.existsSync(p("nope")), fs.existsSync(D), fs.existsSync(), fs.existsSync(null), fs.existsSync(42), fs.existsSync("a\0b")]);
t("exists Buffer", () => fs.existsSync(Buffer.from(p("a.txt"))));
t("open/read/close", () => {
  const fd = fs.openSync(p("a.txt"), "r");
  const buf = Buffer.alloc(5);
  const n = fs.readSync(fd, buf, 0, 5, null);
  const n2 = fs.readSync(fd, buf, 0, 5, 0);
  const n3 = fs.readSync(fd, buf, { position: 1, length: 2 });
  fs.closeSync(fd);
  return [typeof fd, n, n2, n3, buf.toString("hex")];
});
t("readSync null options", () => {
  const fd = fs.openSync(p("a.txt"), "r");
  const buf = Buffer.alloc(5);
  const n = fs.readSync(fd, buf, null);
  const n2 = fs.readSync(fd, buf, null, 5, null);
  fs.closeSync(fd);
  return [n, n2, buf.toString("hex")];
});
t("readSync positioned", () => {
  const fd = fs.openSync(p("a.txt"));
  const b = Buffer.alloc(4);
  const n = fs.readSync(fd, b, 1, 3, 7);
  fs.closeSync(fd);
  return [n, b.toString("hex")];
});
t("readSync eof", () => {
  const fd = fs.openSync(p("a.txt"));
  const n = fs.readSync(fd, Buffer.alloc(8), 0, 8, 1000);
  fs.closeSync(fd);
  return n;
});
t("open/write/close", () => {
  const fd = fs.openSync(p("w.txt"), "w", 0o644);
  const r = [fs.writeSync(fd, "abc"), fs.writeSync(fd, Buffer.from("defg"), 1, 2), fs.writeSync(fd, "Z", 0), fs.writeSync(fd, "é", null, "latin1")];
  fs.closeSync(fd);
  return [r, fs.readFileSync(p("w.txt"), "hex")];
});
t("writeSync buffer position", () => {
  const fd = fs.openSync(p("w2.txt"), "w+");
  fs.writeSync(fd, Buffer.from("0123456789"));
  fs.writeSync(fd, Buffer.from("AB"), 0, 2, 4);
  const out = fs.readFileSync(fd, "utf8");
  fs.writeSync(fd, new Uint8Array([67]), { position: 0 });
  fs.closeSync(fd);
  return [out, fs.readFileSync(p("w2.txt"), "utf8")];
});
t("readFileSync fd", () => {
  const fd = fs.openSync(p("a.txt"));
  fs.readSync(fd, Buffer.alloc(3), 0, 3, null);
  const rest = fs.readFileSync(fd, "utf8");
  fs.closeSync(fd);
  return rest;
});
t("writeFileSync fd", () => {
  const fd = fs.openSync(p("wf.txt"), "w");
  fs.writeFileSync(fd, "via fd");
  fs.closeSync(fd);
  return fs.readFileSync(p("wf.txt"), "utf8");
});
t("append O_APPEND flags", () => {
  const fd = fs.openSync(p("ap.txt"), fs.constants.O_CREAT | fs.constants.O_WRONLY | fs.constants.O_APPEND, 0o644);
  fs.writeSync(fd, "1");
  fs.writeSync(fd, "2");
  fs.closeSync(fd);
  return fs.readFileSync(p("ap.txt"), "utf8");
});
t("big roundtrip", () => {
  const data = Buffer.alloc(3 * 1024 * 1024 + 7);
  for (let i = 0; i < data.length; i += 4099) data[i] = i & 255;
  fs.writeFileSync(p("big.bin"), data);
  const back = fs.readFileSync(p("big.bin"));
  return [back.length, back.equals(data)];
});
t("constants", () => [fs.constants.O_RDONLY, fs.constants.O_WRONLY, fs.constants.O_RDWR, fs.constants.S_IFMT, fs.constants.S_IFDIR, fs.constants.F_OK, fs.constants.R_OK, fs.constants.W_OK, fs.constants.X_OK]);
