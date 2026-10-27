interface Point { x: number; y: { z: string } }
declare const p: Point;
const q: { x: string; y: { z: number } } = p;
