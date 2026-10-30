import { task } from "hereby";
import { absent } from "./lib.mjs";
export const t = task({ name: "t", run: () => absent });
