import { task } from "hereby";
import { configure } from "./lib/configure.mjs";

export const t = task({ name: "t", run: () => {} });
configure({ mode: "strict" });
