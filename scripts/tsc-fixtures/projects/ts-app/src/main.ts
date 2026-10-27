import { Stack } from "./stack";
import { parse } from "./parse";

const s = new Stack<number>();
for (const tok of parse("1 2 3")) s.push(tok);
export const top = s.peek();
export const size = s.size;
export function lookup(table: Record<string, number>, key: string): number {
    return table[key] ?? -1;
}
export interface Opts { name?: string; level: number }
export const o: Opts = { level: 1, name: undefined };
