export interface Job<T> {
    name: string;
    run(): Promise<T>;
}

export async function runAll<T>(jobs: Job<T>[]): Promise<Map<string, T>> {
    const results = await Promise.all(jobs.map(async (j) => [j.name, await j.run()] as const));
    return new Map(results);
}

export function* range(start: number, end: number, step = 1): Generator<number, void, undefined> {
    for (let i = start; i < end; i += step) yield i;
}

export async function* ticks(n: number): AsyncGenerator<number> {
    for (const i of range(0, n)) {
        await Promise.resolve();
        yield i;
    }
}

export async function collect<T>(it: AsyncIterable<T>): Promise<T[]> {
    const out: T[] = [];
    for await (const v of it) out.push(v);
    return out;
}

export class Queue<T> implements Iterable<T> {
    private items: T[] = [];
    push(...xs: T[]): this {
        this.items.push(...xs);
        return this;
    }
    [Symbol.iterator](): Iterator<T> {
        return this.items[Symbol.iterator]();
    }
}
