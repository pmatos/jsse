import { task } from "hereby";

export const t = task({ name: "t", run: () => {} });

async function discover() {
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 2));
  throw new Error("discovery failed");
}

async function setup() {
  await discover();
}

await setup();
