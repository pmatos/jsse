/** Anything with an area. */
export interface Shape {
    area(): number;
}

export type Id = string | number;

export interface Options {
    name: string;
    level?: number;
}
