import { Counter, makeId } from "../lib.js";
import type { Options } from "../types.js";

const opts: Options = { name: "esm" };
export const id = makeId(opts, 1);
export const counter = new Counter("e");
export const top = await Promise.resolve(1);
