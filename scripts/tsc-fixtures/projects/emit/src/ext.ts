import { Level, makeId } from "./lib.ts";
import type { Id } from "./types.ts";

export * from "./lib.ts";
export { Level as L } from "./lib.ts";

export const id: Id = makeId({ name: "x" });
export const load = () => import("./lib.ts");
export const level = Level.Low;
