// Microtask and timer ordering must match the job-queue semantics exactly.
const log = [];
setTimeout(() => log.push("timeout 0"), 0);
Promise.resolve().then(() => log.push("micro 1")).then(() => log.push("micro 3"));
Promise.resolve().then(() => log.push("micro 2"));
(async () => {
  log.push("async start");
  await null;
  log.push("after await 1");
  await new Promise((r) => setTimeout(r, 0));
  log.push("after timer await");
})();
const thenable = { then(res) { log.push("thenable then"); res("t"); } };
Promise.resolve(thenable).then((v) => log.push("thenable resolved " + v));
Promise.all([1, Promise.resolve(2)]).then((v) => log.push("all " + v));
Promise.allSettled([Promise.reject(new Error("no")), 1]).then((r) => log.push("settled " + r.map((x) => x.status)));
Promise.any([Promise.reject(1), Promise.resolve("any")]).then((v) => log.push(v));
Promise.race([new Promise(() => {}), Promise.resolve("race")]).then((v) => log.push(v));
log.push("sync end");
setTimeout(() => show(log.join("\n")), 10);
