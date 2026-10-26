'use strict';

// The .env syntax of Node's Dotenv::ParseContent (src/node_dotenv.cc).

const {
  StringPrototypeCharAt,
  StringPrototypeIndexOf,
  StringPrototypeReplaceAll,
  StringPrototypeSlice,
  StringPrototypeStartsWith,
} = primordials;

function trim(str) {
  let start = 0;
  let end = str.length;
  while (start < end && (str[start] === ' ' || str[start] === '\t')) start++;
  while (end > start && (str[end - 1] === ' ' || str[end - 1] === '\t')) end--;
  return StringPrototypeSlice(str, start, end);
}

function parse(input) {
  const result = { __proto__: null };
  let content = StringPrototypeReplaceAll(input, '\r\n', '\n');
  content = StringPrototypeReplaceAll(content, '\r', '\n');
  let pos = 0;
  const end = content.length;

  while (pos < end) {
    let lineEnd = StringPrototypeIndexOf(content, '\n', pos);
    if (lineEnd === -1) lineEnd = end;
    let line = trim(StringPrototypeSlice(content, pos, lineEnd));
    if (line === '' || line[0] === '#') {
      pos = lineEnd + 1;
      continue;
    }
    const eq = StringPrototypeIndexOf(line, '=');
    if (eq === -1) {
      pos = lineEnd + 1;
      continue;
    }
    let key = trim(StringPrototypeSlice(line, 0, eq));
    if (StringPrototypeStartsWith(key, 'export ')) {
      key = trim(StringPrototypeSlice(key, 7));
    }
    if (key === '') {
      pos = lineEnd + 1;
      continue;
    }
    const valueStart = pos + StringPrototypeIndexOf(content, '=', pos) - pos + 1;
    let rest = StringPrototypeSlice(content, valueStart);
    let lead = 0;
    while (rest[lead] === ' ' || rest[lead] === '\t') lead++;
    rest = StringPrototypeSlice(rest, lead);
    const quote = StringPrototypeCharAt(rest, 0);
    let value;
    let consumed;
    if (quote === '"' || quote === "'" || quote === '`') {
      const close = StringPrototypeIndexOf(rest, quote, 1);
      if (close !== -1) {
        value = StringPrototypeSlice(rest, 1, close);
        if (quote === '"') value = StringPrototypeReplaceAll(value, '\\n', '\n');
        const after = StringPrototypeIndexOf(rest, '\n', close);
        consumed = after === -1 ? rest.length : after;
        result[key] = value;
        pos = valueStart + lead + consumed + 1;
        continue;
      }
    }
    let nl = StringPrototypeIndexOf(rest, '\n');
    if (nl === -1) nl = rest.length;
    value = StringPrototypeSlice(rest, 0, nl);
    const hash = StringPrototypeIndexOf(value, '#');
    if (hash !== -1) value = StringPrototypeSlice(value, 0, hash);
    result[key] = trim(value);
    pos = valueStart + lead + nl + 1;
  }
  return result;
}

module.exports = { parse };
