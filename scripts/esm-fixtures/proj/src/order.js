import './order-a.js';
import cjs from './order-cjs.cjs';
import './tla-delay.js';
import './order-b.js';
globalThis.log.push('order body ' + cjs.fromCjs);
