export function parse(input: string): number[] {
    return input
        .split(/\s+/)
        .filter(Boolean)
        .map((t) => Number(t));
}
