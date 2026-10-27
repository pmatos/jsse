// RUN: 1
async function work() {
  throw new Error("async failure");
}
work();
