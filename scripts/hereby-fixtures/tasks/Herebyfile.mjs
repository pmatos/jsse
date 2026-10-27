import { task } from "hereby";

const prepare = task({
  name: "prepare",
  description: "Writes a line",
  run: () => {
    console.log("preparing");
  },
});

export const build = task({
  name: "build",
  description: "Builds after prepare, with a description long enough to wrap onto a second line of the task listing",
  dependencies: [prepare],
  run: async () => {
    await new Promise((resolve) => setTimeout(resolve, 5));
    console.log("building");
  },
});

export const test = task({
  name: "test",
  description: "Depends on build",
  dependencies: [build, prepare],
  run: () => {
    console.log("testing");
  },
});

export const failAsync = task({
  name: "fail-async",
  description: "Rejects with an Error",
  dependencies: [prepare],
  run: async () => {
    await Promise.resolve();
    throw new Error("async boom");
  },
});

export const failSync = task({
  name: "fail-sync",
  run: () => {
    throw new TypeError("sync boom");
  },
});

export const failString = task({
  name: "fail-string",
  run: () => {
    throw "plain string";
  },
});

export const failBoth = task({
  name: "fail-both",
  dependencies: [failSync, failAsync],
  run: () => {
    console.log("unreachable");
  },
});

export const noRun = task({
  name: "group",
  description: "Only dependencies",
  dependencies: [build, test],
});

export default test;
