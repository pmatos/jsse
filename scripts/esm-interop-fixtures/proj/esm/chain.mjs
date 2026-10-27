import viaCjs from '../cjs/needs-esm.cjs';
import * as direct from './plain.mjs';
export const same = viaCjs === direct || viaCjs.a === direct.a;
export const viaCjsKeys = Object.keys(viaCjs);
