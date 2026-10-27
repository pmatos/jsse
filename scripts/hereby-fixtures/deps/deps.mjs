import fs from "fs";
import os from "os";
import path from "path";
import { glob, globSync, Glob } from "glob";
import chokidar from "chokidar";
import pc from "picocolors";
import { CancelToken, CancelError } from "@esfx/canceltoken";
import which from "which";
import minimist from "minimist";

const log = (...args) => console.log(...args);
const root = fs.mkdtempSync(path.join(os.tmpdir(), "hereby-deps-"));
const sub = (name) => path.join(root, name);
for (const f of ["a.ts", "b.ts", "c.js", "sub/d.ts", "sub/deep/e.ts", "sub/deep/f.json", "node_modules/g.ts"]) {
  fs.mkdirSync(path.dirname(sub(f)), { recursive: true });
  fs.writeFileSync(sub(f), f);
}

log("glob.sync", globSync("**/*.ts", { cwd: root, ignore: "node_modules/**" }).sort());
log("glob async", (await glob("**/*.{ts,json}", { cwd: root, ignore: ["node_modules/**"] })).sort());
log("glob dot/mark", (await glob("*", { cwd: root, mark: true })).sort());
log("glob absolute", (await glob("sub/*.ts", { cwd: root, absolute: true })).map((p) => path.relative(root, p)));
log("glob nodir", (await glob("sub/**", { cwd: root, nodir: true })).sort());
const streamed = [];
for await (const entry of new Glob("**/*.js", { cwd: root }).iterate()) streamed.push(entry);
log("glob iterate", streamed.sort());
log("glob withFileTypes", (await glob("sub/*", { cwd: root, withFileTypes: true })).map((e) => `${e.name}:${e.isDirectory()}`).sort());

log("picocolors.isColorSupported", pc.isColorSupported);
log("picocolors.green", JSON.stringify(pc.green("x")), JSON.stringify(pc.createColors(true).red("y")), JSON.stringify(pc.createColors(false).red("y")));
log("tty", process.stdout.isTTY, process.stderr.isTTY, typeof process.stdout.getColorDepth, typeof process.stdout.hasColors);
log("columns", process.stdout.columns);
log("env", process.env.FORCE_COLOR, process.env.NO_COLOR, process.env.TERM, process.env.CI);

const source = CancelToken.source();
const token = source.token;
log("cancel before", token.signaled, token.canBeSignaled);
const waiter = new Promise((resolve) => token.subscribe(() => resolve("subscribed callback")));
source.cancel();
log("cancel after", token.signaled, await waiter);
try {
  token.throwIfSignaled();
} catch (e) {
  log("cancel error", e instanceof CancelError, e.name, e.message);
}
log("never", CancelToken.none.canBeSignaled, CancelToken.canceled.signaled);

log("which node", path.basename(which.sync("node")));
log("which missing", which.sync("definitely-not-a-command-xyz", { nothrow: true }));
log("minimist", JSON.stringify(minimist(["--a", "1", "-b", "--no-c", "rest", "--d=x"])));

const watchDir = sub("watched");
fs.mkdirSync(watchDir);
const watcher = chokidar.watch(watchDir, { ignoreInitial: true });
const events = [];
let waiters = [];
watcher.on("all", (name, p) => {
  const event = `${name}:${path.relative(watchDir, p)}`;
  events.push(event);
  waiters = waiters.filter((w) => !w(event));
});
const waitFor = (wanted) =>
  new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("timed out waiting for " + wanted)), 20000);
    waiters.push((event) => {
      if (event !== wanted) return false;
      clearTimeout(timer);
      resolve();
      return true;
    });
  });
await new Promise((resolve) => watcher.once("ready", resolve));
log("watched", JSON.stringify(Object.keys(watcher.getWatched()).map((p) => path.relative(root, p))));
const added = waitFor("add:one.txt");
fs.writeFileSync(path.join(watchDir, "one.txt"), "1");
await added;
await new Promise((r) => setTimeout(r, 100));
const changed = waitFor("change:one.txt");
fs.writeFileSync(path.join(watchDir, "one.txt"), "22");
await changed;
const removed = waitFor("unlink:one.txt");
fs.rmSync(path.join(watchDir, "one.txt"));
await removed;
const dirAdded = waitFor("addDir:nested");
fs.mkdirSync(path.join(watchDir, "nested"));
await dirAdded;
await watcher.close();
log("chokidar events", [...new Set(events)].sort());
fs.rmSync(root, { recursive: true, force: true });
