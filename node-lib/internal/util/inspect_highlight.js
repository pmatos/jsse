'use strict';

let inspect;

function init(deps) {
  inspect = deps.inspect;
}


const highlightRegExpColors = ['green', 'red', 'yellow', 'cyan', 'magenta'];

/**
 * Colorize a JavaScript RegExp pattern per ECMAScript grammar.
 * This is a tolerant single-pass highlighter using heuristics in some cases.
 * It supports: groups (named/unnamed, lookaround), assertions, alternation,
 * quantifiers, escapes (incl. Unicode properties), character classes and
 * backreferences.
 * @param {string} regexpString
 * @returns {string}
 */
function highlightRegExp(regexpString) {
  let out = '';
  let i = 0;
  let depth = 0;
  let inClass = false;

  // TODO(BridgeAR): Add group type tracking. That allows to increase the depth
  // in case the same type is next to each other.
  // let groupType = 0;

  // Verify palette and update cache if user changed colors
  const paletteNames = highlightRegExp.colors?.length > 0 ?
    highlightRegExp.colors :
    highlightRegExpColors;

  const palette = paletteNames.reduce((acc, name) => {
    const color = inspect.colors[name];
    if (color) acc.push([`\u001b[${color[0]}m`, `\u001b[${color[1]}m`]);
    return acc;
  }, []);

  function writeGroup(start, end, decreaseDepth = 1) {
    let seq = '';
    i++;
    // Only checking for the closing delimiter is a fast heuristic for regular
    // expressions without the u or v flag. A safer check would verify that the
    // read characters are all alphanumeric.
    while (i < regexpString.length && regexpString[i] !== end) {
      seq += regexpString[i++];
    }
    if (i < regexpString.length) {
      depth -= decreaseDepth;
      write(start);
      writeDepth(seq, 1, 1);
      write(end);
      depth += decreaseDepth;
    } else {
      // The group is not closed which would lead to mistakes in the output.
      // This is a workaround to prevent output from being corrupted.
      writeDepth(start, 1, -seq.length);
    }
  }

  const write = (str) => {
    const idx = depth % palette.length;
    // Safeguard against bugs in the implementation.
    const color = palette[idx] ?? palette[0];
    out += color[0] + str + color[1];
    return idx;
  };

  function writeDepth(str, incDepth, incI) {
    depth += incDepth;
    write(str);
    depth -= incDepth;
    i += incI;
  }

  // Opening '/'
  write('/');
  depth++;
  i = 1;

  // Parse pattern until next unescaped '/'
  while (i < regexpString.length) {
    const ch = regexpString[i];

    if (inClass) {
      if (ch === '\\') {
        let seq = '\\';
        i++;
        if (i < regexpString.length) {
          seq += regexpString[i++];
          const next = seq[1];
          if (next === 'u' && regexpString[i] === '{') {
            writeGroup(`${seq}{`, '}', 0);
            continue;
          } else if ((next === 'p' || next === 'P') && regexpString[i] === '{') {
            writeGroup(`${seq}{`, '}', 0);
            continue;
          } else if (seq[1] === 'x') {
            seq += regexpString.slice(i, i + 2);
            i += 2;
          }
        }
        write(seq);
      } else if (ch === ']') {
        depth--;
        write(']');
        i++;
        inClass = false;
      } else if (ch === '-' && regexpString[i - 1] !== '[' &&
                 i + 1 < regexpString.length && regexpString[i + 1] !== ']') {
        writeDepth('-', 1, 1);
      } else {
        write(ch);
        i++;
      }
    } else if (ch === '[') {
      // Enter class
      write('[');
      depth++;
      i++;
      inClass = true;
    } else if (ch === '(') {
      write('(');
      depth++;
      i++;
      if (i < regexpString.length && regexpString[i] === '?') {
        // Assertions and named groups
        i++;
        const a = i < regexpString.length ? regexpString[i] : '';
        if (a === ':' || a === '=' || a === '!') {
          writeDepth(`?${a}`, -1, 1);
        } else {
          const b = i + 1 < regexpString.length ? regexpString[i + 1] : '';
          if (a === '<' && (b === '=' || b === '!')) {
            writeDepth(`?<${b}`, -1, 2);
          } else if (a === '<') {
            // Named capture: write '?<name>' as a single colored token
            i++; // consume '<'
            const start = i;
            while (i < regexpString.length && regexpString[i] !== '>') {
              i++;
            }
            const name = regexpString.slice(start, i);
            if (i < regexpString.length && regexpString[i] === '>') {
              depth--;
              write('?<');
              writeDepth(name, 1, 0);
              write('>');
              depth++;
              i++;
            } else {
              writeDepth('?<', -1, 0);
              write(name);
            }
          } else {
            write('?');
          }
        }
      }
    } else if (ch === ')') {
      depth--;
      write(')');
      i++;
    } else if (ch === '\\') {
      let seq = '\\';
      i++;
      if (i < regexpString.length) {
        seq += regexpString[i++];
        const next = seq[1];
        if (i < regexpString.length) {
          if (next === 'u' && regexpString[i] === '{') {
            writeGroup(`${seq}{`, '}', 0);
            continue;
          } else if (next === 'x') {
            seq += regexpString.slice(i, i + 2);
            i += 2;
          } else if (next >= '0' && next <= '9') {
            while (i < regexpString.length && regexpString[i] >= '0' && regexpString[i] <= '9') {
              seq += regexpString[i++];
            }
          } else if (next === 'k' && regexpString[i] === '<') {
            writeGroup(`${seq}<`, '>');
            continue;
          } else if ((next === 'p' || next === 'P') && regexpString[i] === '{') {
            // Unicode properties
            writeGroup(`${seq}{`, '}', 0);
            continue;
          }
        }
      }
      writeDepth(seq, 1, 0);
    } else if (ch === '|' || ch === '+' || ch === '*' || ch === '?' || ch === ',' || ch === '^' || ch === '$') {
      writeDepth(ch, 3, 1);
    } else if (ch === '{') {
      i++;
      let digits = '';
      while (i < regexpString.length && regexpString[i] >= '0' && regexpString[i] <= '9') {
        digits += regexpString[i++];
      }
      if (digits) {
        write('{');
        depth++;
        writeDepth(digits, 1, 0);
      }
      if (i < regexpString.length) {
        if (regexpString[i] === ',') {
          if (!digits) {
            write('{');
            depth++;
          }
          write(',');
          i++;
        } else if (!digits) {
          depth += 1;
          write('{');
          depth -= 1;
          continue;
        }
      }
      let digits2 = '';
      while (i < regexpString.length && regexpString[i] >= '0' && regexpString[i] <= '9') {
        digits2 += regexpString[i++];
      }
      if (digits2) {
        writeDepth(digits2, 1, 0);
      }
      if (i < regexpString.length && regexpString[i] === '}') {
        depth--;
        write('}');
        i++;
      }
      if (i < regexpString.length && regexpString[i] === '?') {
        writeDepth('?', 3, 1);
      }
    } else if (ch === '.') {
      writeDepth(ch, 2, 1);
    } else if (ch === '/') {
      // Stop at closing delimiter (unescaped, outside of character class)
      break;
    } else {
      writeDepth(ch, 1, 1);
    }
  }

  // Closing delimiter and flags
  writeDepth('/', -1, 1);
  if (i < regexpString.length) {
    write(regexpString.slice(i));
  }
  return out;
}

module.exports = {
  init,
  highlightRegExp,
};
