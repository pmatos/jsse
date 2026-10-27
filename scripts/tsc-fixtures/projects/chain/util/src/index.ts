import { greet, Named, Counter } from "../../core/src/index";

export function shout(n: Named): string {
    return greet(n).toUpperCase();
}
export const counter = new Counter();
