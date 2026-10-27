export interface Named {
    name: string;
}
export function greet(n: Named): string {
    return "hello " + n.name;
}
export const VERSION = 3;
export { Counter } from "./extra";
