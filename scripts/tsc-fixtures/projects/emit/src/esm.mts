import { Counter } from "./lib.js";

export const c = new Counter("m");
export const here = import.meta.url;

export default function main(): number {
    return c.inc();
}
