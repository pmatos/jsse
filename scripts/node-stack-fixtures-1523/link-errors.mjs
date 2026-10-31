// Link errors of `export ... from` and star exports carry Node's decorated
// source line and read `at async` (issue #1524); a module that failed to load
// is tried again by the next import() while one that failed to link or to
// evaluate keeps its error.
Error.stackTraceLimit = 100;
const frames = (e) =>
  e.stack
    .split("\n")
    .filter((line) => !line.includes("node:"))
    .join("\n");

async function attempt(load) {
  try {
    await load();
  } catch (e) {
    return e;
  }
}

const loads = {
  "reexport-missing": () => import("./reexport-missing.mjs"),
  "reexport-renamed": () => import("./reexport-renamed.mjs"),
  "reexport-chain": () => import("./reexport-chain.mjs"),
  "reexport-spaced": () => import("./reexport-spaced.mjs"),
  "reexport-second": () => import("./reexport-second.mjs"),
  "star-import": () => import("./star-import.mjs"),
  "star-reexport": () => import("./star-reexport.mjs"),
  "link-and-load": () => import("./link-and-load.mjs"),
  "parse-error": () => import("./parse-error.mjs"),
  "missing-dependency": () => import("./missing-dependency.mjs"),
  "throws": () => import("./throws.mjs"),
};

for (const name of [
  "reexport-missing",
  "reexport-renamed",
  "reexport-chain",
  "reexport-spaced",
  "reexport-second",
  "star-import",
  "star-reexport",
  "link-and-load",
]) {
  console.log("## " + name + "\n" + frames(await attempt(loads[name])));
}

async function again(load) {
  return attempt(load);
}
for (const name of ["parse-error", "missing-dependency", "reexport-missing", "throws"]) {
  const first = await attempt(loads[name]);
  const second = await again(loads[name]);
  console.log("## twice: " + name);
  console.log("same error object:", first === second);
  console.log("same stack:", first.stack === second.stack);
  console.log(frames(second));
}
