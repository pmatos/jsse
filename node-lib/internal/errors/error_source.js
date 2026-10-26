'use strict';

// Node's lib/internal/errors/error_source.js (v26.8.2) reads the failing call
// site from V8's structured stack and tokenizes the source line with acorn.
// Here the site is parsed out of `error.stack` and a small tokenizer stands in
// for acorn; only the token kinds getFirstExpression looks at are told apart.

const {
  ArrayPrototypeIncludes,
  ArrayPrototypePush,
  RegExpPrototypeExec,
  StringPrototypeIndexOf,
  StringPrototypeSlice,
  StringPrototypeSplit,
} = primordials;

const kKeywords = [
  'break', 'case', 'catch', 'continue', 'debugger', 'default', 'do', 'else',
  'finally', 'for', 'function', 'if', 'return', 'switch', 'throw', 'try',
  'var', 'const', 'while', 'with', 'new', 'this', 'super', 'class', 'extends',
  'export', 'import', 'null', 'true', 'false', 'in', 'instanceof', 'typeof',
  'void', 'delete',
];
const kValueEnders = ['name', 'num', 'string', ')', ']', '}', 'this', 'super',
                      'null', 'true', 'false', '`'];
const kPunctuators = [
  '>>>=', '...', '===', '!==', '**=', '<<=', '>>=', '>>>', '&&=', '||=', '??=',
  '=>', '==', '!=', '<=', '>=', '&&', '||', '??', '?.', '++', '--', '+=', '-=',
  '*=', '/=', '%=', '&=', '|=', '^=', '<<', '>>', '**',
];
const identStart = /[\p{ID_Start}$_\\]/u;
const identPart = /[\p{ID_Continue}$‌‍\\]/u;

function skipTrivia(code, i) {
  while (i < code.length) {
    const c = code[i];
    if (c === ' ' || c === '\t' || c === '\n' || c === '\r' || c === '\v' ||
        c === '\f' || c === ' ' || c === '﻿' || c === ' ' ||
        c === ' ') {
      i++;
    } else if (c === '/' && code[i + 1] === '/') {
      i = code.length;
    } else if (c === '/' && code[i + 1] === '*') {
      const end = StringPrototypeIndexOf(code, '*/', i + 2);
      if (end === -1) return code.length;
      i = end + 2;
    } else {
      break;
    }
  }
  return i;
}

function scanQuoted(code, i, quote) {
  for (i++; i < code.length; i++) {
    if (code[i] === '\\') i++;
    else if (code[i] === quote) return i + 1;
  }
  return -1;
}

function scanTemplate(code, i, tokens, braces) {
  for (; i < code.length; i++) {
    if (code[i] === '\\') {
      i++;
    } else if (code[i] === '`') {
      ArrayPrototypePush(tokens, { start: i, label: '`' });
      return i + 1;
    } else if (code[i] === '$' && code[i + 1] === '{') {
      ArrayPrototypePush(tokens, { start: i, label: '${' });
      ArrayPrototypePush(braces, 'template');
      return i + 2;
    }
  }
  return -1;
}

function scanRegExp(code, i) {
  let inClass = false;
  for (i++; i < code.length; i++) {
    const c = code[i];
    if (c === '\\') i++;
    else if (c === '[') inClass = true;
    else if (c === ']') inClass = false;
    else if (c === '/' && !inClass) {
      i++;
      while (i < code.length && RegExpPrototypeExec(identPart, code[i]) !== null) i++;
      return i;
    }
  }
  return -1;
}

function tokenize(code) {
  const tokens = [];
  const braces = [];
  let i = 0;
  while ((i = skipTrivia(code, i)) < code.length) {
    const c = code[i];
    const start = i;
    let label;
    if (RegExpPrototypeExec(identStart, c) !== null) {
      while (i < code.length && RegExpPrototypeExec(identPart, code[i]) !== null) i++;
      const word = StringPrototypeSlice(code, start, i);
      label = ArrayPrototypeIncludes(kKeywords, word) ? word : 'name';
    } else if (c === '#') {
      i++;
      while (i < code.length && RegExpPrototypeExec(identPart, code[i]) !== null) i++;
      label = 'privateId';
    } else if ((c >= '0' && c <= '9') || (c === '.' && code[i + 1] >= '0' && code[i + 1] <= '9')) {
      while (i < code.length && RegExpPrototypeExec(/[\w.]/, code[i]) !== null) {
        const sign = (code[i] === 'e' || code[i] === 'E') && (code[i + 1] === '+' || code[i + 1] === '-');
        i += sign && !/^0[xX]/.test(StringPrototypeSlice(code, start, i)) ? 2 : 1;
      }
      label = 'num';
    } else if (c === '"' || c === "'") {
      i = scanQuoted(code, i, c);
      if (i === -1) return tokens;
      label = 'string';
    } else if (c === '`') {
      ArrayPrototypePush(tokens, { start, label: '`' });
      i = scanTemplate(code, i + 1, tokens, braces);
      if (i === -1) return tokens;
      continue;
    } else if (c === '}' && braces.length > 0 && braces[braces.length - 1] === 'template') {
      braces.length--;
      ArrayPrototypePush(tokens, { start, label: '}' });
      i = scanTemplate(code, i + 1, tokens, braces);
      if (i === -1) return tokens;
      continue;
    } else if (c === '/' && !ArrayPrototypeIncludes(kValueEnders, tokens.length > 0 ? tokens[tokens.length - 1].label : '')) {
      i = scanRegExp(code, i);
      if (i === -1) return tokens;
      label = 'regexp';
    } else {
      label = c;
      i++;
      for (const p of kPunctuators) {
        if (StringPrototypeSlice(code, start, start + p.length) === p &&
            !(p === '?.' && code[start + 2] >= '0' && code[start + 2] <= '9')) {
          label = p;
          i = start + p.length;
          break;
        }
      }
      if (c === '{') ArrayPrototypePush(braces, '{');
      else if (c === '}' && braces.length > 0) braces.length--;
    }
    ArrayPrototypePush(tokens, { start, label });
  }
  return tokens;
}

const fileLines = { __proto__: null };

function readSourceLine(file, lineNumber) {
  let lines = fileLines[file];
  if (lines === undefined) {
    try {
      lines = StringPrototypeSplit(require('fs').readFileSync(file, 'utf8'), '\n');
    } catch {
      lines = null;
    }
    fileLines[file] = lines;
  }
  if (lines === null) return undefined;
  const line = lines[lineNumber - 1];
  return line === undefined ? undefined : line.endsWith('\r') ? StringPrototypeSlice(line, 0, -1) : line;
}

// The first frame of `error.stack` is the call site; `captureStackTrace(err, fn)`
// has already dropped the frames above `fn`.
function getErrorSourcePositions(error) {
  const stack = error.stack;
  if (typeof stack !== 'string') return {};
  const frame = RegExpPrototypeExec(/^ {4}at .*$/m, stack);
  if (frame === null) return {};
  const match = RegExpPrototypeExec(/^ {4}at (?:.*? \()?(.+?):(\d+):(\d+)\)?$/, frame[0]);
  if (match === null) return {};
  let file = match[1];
  if (StringPrototypeSlice(file, 0, 7) === 'file://') file = decodeURIComponent(StringPrototypeSlice(file, 7));
  const lineNumber = +match[2];
  return {
    sourceLine: readSourceLine(file, lineNumber),
    scriptResourceName: file,
    lineNumber,
    startColumn: +match[3] - 1,
  };
}

function getErrorSourceLocation(error) {
  const { sourceLine, startColumn } = getErrorSourcePositions(error);
  if (!sourceLine) return;
  return { sourceLine, startColumn };
}

const memberAccessTokens = ['.', '?.', '[', ']'];
const memberNameTokens = ['name', 'string', 'num'];

function getFirstExpression(code, startColumn) {
  let lastToken;
  let firstMemberAccessNameToken;
  let terminatingCol;
  let parenLvl = 0;
  for (const token of tokenize(code)) {
    if (token.start < startColumn) {
      if (token.label === ';') {
        firstMemberAccessNameToken = null;
        continue;
      }
      if (ArrayPrototypeIncludes(memberAccessTokens, token.label) && lastToken?.label === 'name') {
        firstMemberAccessNameToken ??= lastToken;
      } else if (!ArrayPrototypeIncludes(memberAccessTokens, token.label) &&
                 !ArrayPrototypeIncludes(memberNameTokens, token.label)) {
        firstMemberAccessNameToken = null;
      }
      lastToken = token;
      continue;
    }
    if (token.label === '(') {
      parenLvl++;
      continue;
    }
    if (token.label === ')') {
      parenLvl--;
      if (parenLvl === 0) {
        terminatingCol = token.start + 1;
        break;
      }
      continue;
    }
    if (token.label === ';') {
      terminatingCol = token;
      break;
    }
  }
  const start = firstMemberAccessNameToken?.start ?? startColumn;
  return StringPrototypeSlice(code, start, terminatingCol);
}

function getErrorSourceExpression(error) {
  const loc = getErrorSourceLocation(error);
  if (loc === undefined) return;
  return getFirstExpression(loc.sourceLine, loc.startColumn);
}

module.exports = {
  getErrorSourceLocation,
  getErrorSourceExpression,
};
