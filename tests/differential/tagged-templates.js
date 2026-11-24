function tag(strings, ...values) {
  return JSON.stringify({ strings, raw: strings.raw, values, frozen: Object.isFrozen(strings) });
}
show(tag`a${1}b${"two"}c`);
show(tag`\n\x41${0}`, String.raw`C:\new\table${1}`);
const cache = [];
function same(s) { cache.push(s); }
for (let i = 0; i < 2; i++) same`x`;
show(cache[0] === cache[1]);
show(tag`\unicode bad escape`);
