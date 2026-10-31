export let counter = 0;
export function bump() { counter += 1; }
export const b = "b";
export const a = "a";
export const B = "B";
export const ab = "ab";
export const _ = "_";
const accented = "accented";
export { accented as "\u00e9", accented as "a b", accented as "\u{10000}", accented as "\uFFFD" };
export default "dflt";
export * from "./module-namespace-export-name-lookup-star_FIXTURE.mjs";
export * as star from "./module-namespace-export-name-lookup-star_FIXTURE.mjs";
