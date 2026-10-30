import { createRequire } from "node:module";
import { task } from "hereby";

const require = createRequire(import.meta.url);
Error.stackTraceLimit = 50;
export const t = task({ name: "t", run: () => {} });
require("./helper.cjs");
