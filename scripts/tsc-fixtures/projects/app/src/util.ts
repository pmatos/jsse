export type Mutable<T> = { -readonly [K in keyof T]: T[K] };
export type Getters<T> = { [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K] };
export type ElementOf<T> = T extends readonly (infer U)[] ? U : never;

export function pick<T extends object, K extends keyof T>(obj: T, ...keys: K[]): Pick<T, K> {
    const out = {} as Pick<T, K>;
    for (const k of keys) out[k] = obj[k];
    return out;
}

export function group<T, K extends string>(items: readonly T[], key: (t: T) => K): Record<K, T[]> {
    const out = {} as Record<K, T[]>;
    for (const item of items) {
        (out[key(item)] ??= []).push(item);
    }
    return out;
}

export function format(value: number): string;
export function format(value: string, width: number): string;
export function format(value: number | string, width = 0): string {
    const s = typeof value === "number" ? value.toFixed(1) : value;
    return s.padStart(width);
}

export function makeGetters<T extends object>(obj: T): Getters<T> {
    const out: Record<string, () => unknown> = {};
    for (const k of Object.keys(obj) as (keyof T & string)[]) {
        out["get" + k[0]!.toUpperCase() + k.slice(1)] = () => obj[k];
    }
    return out as Getters<T>;
}

export const enum Flags {
    None = 0,
    A = 1 << 0,
    B = 1 << 1,
    Both = A | B,
}
