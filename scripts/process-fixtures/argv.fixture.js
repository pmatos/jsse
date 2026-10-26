// RUN: 0
// RUN: 0 a b c
// RUN: 0 a -- b --version
// RUN: 0 --version --help -e x --node --no-bytecode
// RUN: 0 -- --
// RUN: 0 ./rel/../x.txt --prelude=p
function show(label, value) {
  console.log(label + " " + JSON.stringify(value));
}
var argv = process.argv;
show("argv.slice(2)", argv.slice(2));
show("length", argv.length);
show("types", argv.map(function (a) { return typeof a; }));
show("argv[0] === execPath", argv[0] === process.execPath);
show("execPath absolute", process.execPath.charAt(0) === "/");
show("argv[1] basename", argv[1].split("/").pop());
show("argv[1] absolute", argv[1].charAt(0) === "/");
show("argv0 type", typeof process.argv0);
show("isArray", Array.isArray(argv));
