// The entry point.
import Greeter, { Counter, Level, Named, Util, makeId } from "./lib.js";
import * as lib from "./lib.js";
import type { Id, Options, Shape } from "./types.js";

export { Counter } from "./lib.js";
export type { Shape };

async function* gen(): AsyncGenerator<number> {
    yield 1;
    yield 2;
}

function* sync(): Generator<number> {
    yield* [1, 2];
}

/** Runs everything. */
export async function run(opts: Options, shape?: Shape): Promise<string> {
    const c = new Named(opts.name, 2);
    let total = 0;
    for await (const x of gen()) {
        total += x + c.inc();
    }
    const dyn = await import("./lib.js");
    const level: Level = Level.High;
    return new Greeter().greet(makeId<Id>(opts, 1, "b")) + Util.tag(total) + level + shape?.area() + dyn.Level.Low + lib.Flag.A;
}

export const spread = [...sync()];
