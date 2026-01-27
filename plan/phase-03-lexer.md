# Phase 3: Lexer (Lexical Grammar)

**Spec Reference:** §12 (ECMAScript Language: Lexical Grammar)

## Goal
Implement a complete lexer that tokenizes ECMAScript source text according to the spec.

## Tasks

### 3.1 Source Text (§12.1, §11)
- [x] Unicode code point handling
- [x] UTF-16 encoding/decoding
- [x] Source text normalization (N/A: spec doesn't require NFC normalization, UTF-8 input handled natively by Rust)
- [x] Hashbang comments (`#!`)

### 3.2 White Space (§12.2)
- [x] TAB (U+0009), VT (U+000B), FF (U+000C), SP (U+0020)
- [x] NBSP (U+00A0), ZWNBSP (U+FEFF)
- [x] USP (Unicode Space_Separator category)

### 3.3 Line Terminators (§12.3)
- [x] LF (U+000A), CR (U+000D), LS (U+2028), PS (U+2029)
- [x] CR+LF as single line terminator

### 3.4 Comments (§12.4)
- [x] Single-line comments (`//`)
- [x] Multi-line comments (`/* */`)
- [x] Multi-line comments containing line terminators produce LineTerminator tokens
- [x] Hashbang comments

### 3.5 Tokens (§12.5–12.9)
- [x] IdentifierName (§12.7)
  - [x] Unicode ID_Start / ID_Continue
  - [x] Unicode escape sequences in identifiers (`\uXXXX`, `\u{XXXXX}`)
  - [x] Reserved words (§12.7.1): `await`, `break`, `case`, `catch`, `class`, `const`, `continue`, `debugger`, `default`, `delete`, `do`, `else`, `enum`, `export`, `extends`, `false`, `finally`, `for`, `function`, `if`, `import`, `in`, `instanceof`, `new`, `null`, `of`, `return`, `super`, `switch`, `this`, `throw`, `true`, `try`, `typeof`, `var`, `void`, `while`, `with`, `yield`
  - [x] Strict mode additional reserved words: `implements`, `interface`, `package`, `private`, `protected`, `public` (`let` and `static` already handled)
  - [ ] `await` and `yield` context-sensitive keywords
- [x] Punctuators (§12.8): all standard punctuators
- [x] NumericLiteral (§12.9.3)
  - [x] DecimalLiteral (integer, float, exponential)
  - [x] DecimalBigIntegerLiteral
  - [x] NonDecimalIntegerLiteral (binary `0b`, octal `0o`, hex `0x`)
  - [x] NonDecimalIntegerLiteral + BigInt suffix
  - [x] Numeric separators (`_`)
  - [x] Legacy octal (in non-strict mode)
- [x] StringLiteral (§12.9.4)
  - [x] Single and double quoted strings
  - [x] Escape sequences: `\\`, `\'`, `\"`, `\b`, `\f`, `\n`, `\r`, `\t`, `\v`, `\0`
  - [x] Hex escape `\xHH`
  - [x] Unicode escape `\uHHHH` and `\u{HHHH}`
  - [x] Line continuation (`\` + line terminator)
  - [ ] Legacy octal escapes (non-strict only)
- [x] Template Literal tokens (§12.9.6)
  - [x] NoSubstitutionTemplate
  - [x] TemplateHead / TemplateMiddle / TemplateTail
  - [x] Template escape sequences (tagged template raw)
  - [ ] Illegal escape sequences in tagged templates (undefined raw value)
- [x] RegularExpressionLiteral (§12.9.5)
  - [x] RegExp body and flags
  - [x] Disambiguation from division operator (lexer context)

### 3.6 Automatic Semicolon Insertion (§12.10)
- [x] ASI Rule 1: offending token on new line (implemented in parser)
- [x] ASI Rule 2: end of input stream (implemented in parser)
- [x] ASI Rule 3: restricted productions (`return`, `throw`, `yield`, `break`, `continue`, postfix `++`/`--`, arrow `=>`) (implemented in parser)
- [x] No ASI in `for` header, empty statement (implemented in parser)

### 3.7 Lexer Context Management
- [x] Input element goal switching: separate `lex_regex()` and `read_template_continuation()` methods
- [x] Lexer-parser integration interface

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
