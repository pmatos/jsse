// Errors made while import() loads a module carry the frames of the code that
// called import() (issue #1483): the call position, the calling function, the
// async chain, for resolution and syntax errors alike. Link errors are made
// in a later job and read `at async`.
Error.stackTraceLimit = 100;
const show = (label) => (e) => {
  const lines = e.stack.split("\n").filter((line) => !line.includes("node:"));
  console.log("## " + label + "\n" + lines.join("\n"));
};

function sync() {
  import("./missing-sync.mjs").catch(show("sync"));
}
sync();

async function awaited() {
  const m = await import("./missing-await.mjs");
}
async function outer() {
  await awaited();
}
await outer().catch(show("await, two async functions"));

async function withArguments() {
  return await import(String(new URL("./missing-args.mjs", import.meta.url)));
}
await withArguments().catch(show("a call in the specifier"));

async function chained() {
  await import("./missing-chain.mjs").catch((e) => {
    throw e;
  });
}
await chained().catch(show("await of a chain"));

async function dependency() {
  await import("./imports-missing.mjs");
}
await dependency().catch(show("missing dependency"));

function syncDependency() {
  import("./imports-missing-sync.mjs").catch(show("missing dependency, sync"));
}
syncDependency();
await new Promise((resolve) => setTimeout(resolve, 10));

async function syntax() {
  await import("./syntax-error.mjs");
}
await syntax().catch(show("syntax error"));

async function badExport() {
  await import("./bad-export.mjs");
}
await badExport().catch(show("missing export"));

async function badExportTabs() {
  await import("./bad-export-tabs.mjs");
}
await badExportTabs().catch(show("missing export, tabs"));

async function badDefault() {
  await import("./bad-default.mjs");
}
await badDefault().catch(show("missing default export"));
