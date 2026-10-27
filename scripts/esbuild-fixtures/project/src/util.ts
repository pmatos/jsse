export interface Greeting {
    text: string;
}

export function greet(name: string): Greeting {
    return { text: `hello ${name}` };
}

export class Counter {
    value: number;
    constructor(private readonly start: number) {
        this.value = start;
    }
    tick(): void {
        this.value += 1;
    }
}

export function unused(): number {
    return 42;
}
