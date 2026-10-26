import { a, setA } from './cycle-a.js';
export const b = 'b0';
export const getA = () => a;
globalThis.log.push('cycle-b runs');
