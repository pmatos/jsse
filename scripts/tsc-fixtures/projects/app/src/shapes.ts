export interface Point {
    readonly x: number;
    readonly y: number;
}

export enum Color {
    Red,
    Green = "green",
    Blue = 4,
}

export abstract class Shape<T extends Point = Point> {
    static counter = 1;
    #id = Shape.counter++;

    constructor(
        protected readonly origin: T,
        public name: string = "shape",
    ) {}

    abstract area(): number;

    get id(): number {
        return this.#id;
    }

    describe(): string {
        return `${this.name}#${this.#id} at (${this.origin.x}, ${this.origin.y}) area=${this.area().toFixed(2)}`;
    }
}

export class Circle extends Shape {
    constructor(origin: Point, private radius: number) {
        super(origin, "circle");
    }

    area(): number {
        return Math.PI * this.radius ** 2;
    }
}

export class Rect extends Shape<Point> {
    constructor(origin: Point, public w: number, public h: number) {
        super(origin, "rect");
    }

    area(): number {
        return this.w * this.h;
    }
}

export function colorName(c: Color): string {
    switch (c) {
        case Color.Red:
            return "red";
        case Color.Green:
            return "green";
        default:
            return Color[c] ?? "unknown";
    }
}
