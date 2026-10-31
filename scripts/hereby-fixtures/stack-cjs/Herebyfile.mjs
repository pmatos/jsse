import { createRequire } from "node:module";
import { task } from "hereby";

const require = createRequire(import.meta.url);
export const t = task({ name: "t", run: () => {} });
require("./helper.cjs");
