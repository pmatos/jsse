import { n, f, C } from "./a";
import { nothing } from "./a";
import "./does-not-exist";

interface Shape { kind: "circle" | "square"; size: number }
export function area(s: Shape): number {
    switch (s.kind) {
        case "circle":
            return s.size;
        case "triangle":
            return 0;
    }
}
export const arr: string[] = [1, 2, 3];
export const tuple: [number, string] = [1, 2, 3];
export const call = f(1, 2);
export const inst = new C(1);
export const maybe: string = undefined;
const x: { a: number } = { a: 1, b: 2 };
function h(a: number, b?: string, c: boolean): void {}
export default class {}
export default class {}
