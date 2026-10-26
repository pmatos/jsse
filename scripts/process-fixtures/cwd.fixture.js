// CWD: tmp
// RUN: 0
function show(label, value) {
  console.log(label + " " + JSON.stringify(value));
}
function attempt(label, fn) {
  try {
    show(label, fn());
  } catch (e) {
    var here = process.cwd();
    show(label, [
      "throws", e.name, e.code, e.errno === undefined ? null : e.errno,
      e.syscall === undefined ? null : e.syscall,
      e.path === undefined ? null : e.path === here,
      e.dest === undefined ? null : e.dest,
      e.message.split(here).join("<cwd>"),
    ]);
  }
}
function base() { return process.cwd().split("/").pop(); }
show("typeof", typeof process.cwd());
show("absolute", process.cwd().charAt(0) === "/");
show("returns undefined", process.chdir("sub"));
show("after chdir", base());
process.chdir("..");
show("after chdir ..", base() !== "sub");
attempt("enoent", function () { process.chdir("nonexistent-dir"); });
attempt("enotdir", function () { process.chdir("file.txt"); });
attempt("empty", function () { process.chdir(""); });
attempt("number", function () { process.chdir(1); });
attempt("undefined", function () { process.chdir(); });
process.chdir("sub/inner");
show("nested", base());
process.chdir("/");
show("root", process.cwd());
