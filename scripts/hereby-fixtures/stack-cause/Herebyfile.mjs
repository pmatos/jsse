import { task } from "hereby";
export const t = task({ name: "t", run: () => {} });
function inner() {
  throw new RangeError("inner");
}
try {
  inner();
} catch (e) {
  throw new Error("outer", { cause: e });
}
