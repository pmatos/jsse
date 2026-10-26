import { b, getA } from './cycle-b.js';
export let a = 'a0';
export function setA(v) { a = v; }
export const seenFromB = () => b;
globalThis.log.push('cycle-a sees b=' + b + ' getA=' + getA());
