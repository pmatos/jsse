// The counter module.
import type { Id, Options } from "./types.js";

/** Severity levels. */
export enum Level {
    Low,
    High = 5,
}

export const enum Flag {
    A = 1,
    B = 2,
}

export namespace Util {
    export const version = "1";
    export function tag(id: Id): string {
        return `#${id}`;
    }
}

/** A counter. */
export class Counter {
    private n = 0;
    static created = 0;
    static {
        Counter.created += 1;
    }
    constructor(
        public readonly name: string,
        protected step: number = 1,
    ) {}
    /* bumps the count */
    inc(): number {
        this.n += this.step;
        return this.n;
    }
    get value(): number {
        return this.n ?? 0;
    }
}

export class Named extends Counter {
    label = "named";
    override inc(): number {
        return super.inc() * 2;
    }
}

/** @internal */
export const hidden = 1;

export function makeId<T extends Id>(opts: Options, ...rest: T[]): string {
    const { name, ...others } = opts;
    const first = rest[0]?.toString() ?? "none";
    return [name, first, ...rest].join("-") + 2 ** 3 + Object.keys(others).length + (Flag.A | Flag.B);
}

export default class Greeter {
    greet(who: string): string {
        return `hi ${who}`;
    }
}
