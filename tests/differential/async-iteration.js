async function* agen() {
  yield 1;
  await null;
  yield 2;
  throw new TypeError("agen failed");
}
(async () => {
  const seen = [];
  try {
    for await (const v of agen()) seen.push(v);
  } catch (e) {
    seen.push(e.name + ": " + e.message);
  }
  show(seen);
  for await (const v of [Promise.resolve("a"), "b"]) show("for-await", v);
  const results = await Array.fromAsync(agen()).catch((e) => e.message);
  show(results);
  const { promise, resolve } = Promise.withResolvers();
  setTimeout(() => resolve("resolved later"), 1);
  show(await promise);
})();
