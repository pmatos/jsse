import { Circle, Color, Rect, colorName, type Point } from "./shapes";
import { Flags, format, group, makeGetters, pick } from "./util";
import { Queue, collect, runAll, ticks } from "./tasks";

const origin: Point = { x: 1, y: 2 };
const shapes = [new Circle(origin, 2), new Rect(origin, 3, 4)];

export const summary = shapes.map((s) => s.describe());
export const byName = group(shapes, (s) => s.name);
export const picked = pick({ a: 1, b: "two", c: true }, "a", "c");
export const getters = makeGetters({ width: 10, height: 20 });
export const label = format(3.14159) + format("x", 4);
export const colors = [Color.Red, Color.Green, Color.Blue].map(colorName);
export const flags = Flags.Both & ~Flags.A;

export async function main(): Promise<string> {
    const results = await runAll([
        { name: "one", run: async () => 1 },
        { name: "two", run: () => Promise.resolve(2) },
    ]);
    const t = await collect(ticks(3));
    const q = new Queue<number>().push(1, 2, 3);
    const [first, ...rest] = [...q];
    return JSON.stringify({ results: [...results], t, first, rest, deep: origin?.x });
}
