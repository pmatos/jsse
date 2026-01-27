# Phase 3: Lexer (Lexical Grammar)

**Spec Reference:** §12 (ECMAScript Language: Lexical Grammar)

## Goal
Implement a complete lexer that tokenizes ECMAScript source text according to the spec.

## Tasks

### 3.1 Source Text (§12.1, §11)
- [ ] Unicode code point handling
- [ ] UTF-16 encoding/decoding
- [ ] Source text normalization
- [ ] Hashbang comments (`#!`)

### 3.2 White Space (§12.2)
- [ ] TAB (U+0009), VT (U+000B), FF (U+000C), SP (U+0020)
- [ ] NBSP (U+00A0), ZWNBSP (U+FEFF)
- [ ] USP (Unicode Space_Separator category)

### 3.3 Line Terminators (§12.3)
- [ ] LF (U+000A), CR (U+000D), LS (U+2028), PS (U+2029)
- [ ] CR+LF as single line terminator

### 3.4 Comments (§12.4)
- [ ] Single-line comments (`//`)
- [ ] Multi-line comments (`/* */`)
- [ ] Multi-line comments containing line terminators produce LineTerminator tokens
- [ ] Hashbang comments

### 3.5 Tokens (§12.5–12.9)
- [ ] IdentifierName (§12.7)
  - [ ] Unicode ID_Start / ID_Continue
  - [ ] Unicode escape sequences in identifiers (`\uXXXX`, `\u{XXXXX}`)
  - [ ] Reserved words (§12.7.1): `await`, `break`, `case`, `catch`, `class`, `const`, `continue`, `debugger`, `default`, `delete`, `do`, `else`, `enum`, `export`, `extends`, `false`, `finally`, `for`, `function`, `if`, `import`, `in`, `instanceof`, `new`, `null`, `of`, `return`, `super`, `switch`, `this`, `throw`, `true`, `try`, `typeof`, `var`, `void`, `while`, `with`, `yield`
  - [ ] Strict mode additional reserved words: `let`, `static`, `implements`, `interface`, `package`, `private`, `protected`, `public`
  - [ ] `await` and `yield` context-sensitive keywords
- [ ] Punctuators (§12.8): `{`, `}`, `(`, `)`, `[`, `]`, `.`, `...`, `;`, `,`, `<`, `>`, `<=`, `>=`, `==`, `!=`, `===`, `!==`, `+`, `-`, `*`, `%`, `**`, `++`, `--`, `<<`, `>>`, `>>>`, `&`, `|`, `^`, `!`, `~`, `&&`, `||`, `??`, `?`, `?.`, `:`, `=`, `+=`, `-=`, `*=`, `%=`, `**=`, `<<=`, `>>=`, `>>>=`, `&=`, `|=`, `^=`, `&&=`, `||=`, `??=`, `=>`, `/`, `/=`
- [ ] NumericLiteral (§12.9.3)
  - [ ] DecimalLiteral (integer, float, exponential)
  - [ ] DecimalBigIntegerLiteral
  - [ ] NonDecimalIntegerLiteral (binary `0b`, octal `0o`, hex `0x`)
  - [ ] NonDecimalIntegerLiteral + BigInt suffix
  - [ ] Numeric separators (`_`)
  - [ ] Legacy octal (in non-strict mode)
- [ ] StringLiteral (§12.9.4)
  - [ ] Single and double quoted strings
  - [ ] Escape sequences: `\\`, `\'`, `\"`, `\b`, `\f`, `\n`, `\r`, `\t`, `\v`, `\0`
  - [ ] Hex escape `\xHH`
  - [ ] Unicode escape `\uHHHH` and `\u{HHHH}`
  - [ ] Line continuation (`\` + line terminator)
  - [ ] Legacy octal escapes (non-strict only)
- [ ] Template Literal tokens (§12.9.6)
  - [ ] NoSubstitutionTemplate
  - [ ] TemplateHead / TemplateMiddle / TemplateTail
  - [ ] Template escape sequences (tagged template raw)
  - [ ] Illegal escape sequences in tagged templates (undefined raw value)
- [ ] RegularExpressionLiteral (§12.9.5)
  - [ ] RegExp body and flags
  - [ ] Disambiguation from division operator (lexer context)

### 3.6 Automatic Semicolon Insertion (§12.10)
- [ ] ASI Rule 1: offending token on new line
- [ ] ASI Rule 2: end of input stream
- [ ] ASI Rule 3: restricted productions (`return`, `throw`, `yield`, `break`, `continue`, postfix `++`/`--`, arrow `=>`)
- [ ] No ASI in `for` header, empty statement

### 3.7 Lexer Context Management
- [ ] Input element goal switching: `InputElementDiv` vs `InputElementRegExp` vs `InputElementRegExpOrTemplateTail` vs `InputElementTemplateTail`
- [ ] Lexer-parser integration interface

## test262 Tests
- `test262/test/language/white-space/` — 67 tests
- `test262/test/language/line-terminators/` — 41 tests
- `test262/test/language/comments/` — 52 tests
- `test262/test/language/identifiers/` — 268 tests
- `test262/test/language/keywords/` — 25 tests
- `test262/test/language/reserved-words/` — 27 tests
- `test262/test/language/future-reserved-words/` — 55 tests
- `test262/test/language/punctuators/` — 11 tests
- `test262/test/language/literals/` — 534 tests (numeric, string, boolean, null, bigint, regexp)
- `test262/test/language/asi/` — 102 tests
- `test262/test/language/source-text/` — 1 test
