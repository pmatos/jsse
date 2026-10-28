import { Counter } from "./lib.js";

const c = new Counter("c");
export = { c, run: () => c.inc() };
