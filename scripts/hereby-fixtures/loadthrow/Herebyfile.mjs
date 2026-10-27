import { task } from "hereby";

function explode() {
  throw new RangeError("load boom");
}

export const t = task({ name: "t", run: () => {} });
explode();
