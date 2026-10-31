// V8 reports the latest emitted source position for an error made while
// import() loads a module: for a specifier that is a variable or a property
// read, that is the load (issue #1524), not the call or the `await` in front.
Error.stackTraceLimit = 100;
const show = (label) => (e) => {
  const lines = e.stack.split("\n").filter((line) => !line.includes("node:"));
  console.log("## " + label + "\n" + lines.join("\n"));
};
const obj = { s: "./nope4.mjs", deep: { s: "./nope12.mjs" } };
async function variable(spec) { await import(spec); }
async function literal() { await import("./nope2.mjs"); }
async function member() { await import(obj.s); }
async function deepMember() { await import(obj.deep.s); }
async function sync(spec) { import(spec).catch(show("not awaited")); }
async function declared(spec) { const m = await import(spec); }
async function kept(spec) { const m = import(spec); await m.catch(show("kept promise")); }
async function returned(spec) { return await import(spec); }
async function globalMember() { await import(globalThis.spec); }
globalThis.spec = "./nope11.mjs";
await variable("./nope1.mjs").catch(show("variable"));
await literal().catch(show("literal"));
await member().catch(show("member"));
await deepMember().catch(show("nested member"));
await sync("./nope6.mjs");
await new Promise((r) => setTimeout(r, 20));
await declared("./nope7.mjs").catch(show("const m = await"));
await kept("./nope8.mjs");
await returned("./nope9.mjs").catch(show("return await"));
await globalMember().catch(show("global member"));
