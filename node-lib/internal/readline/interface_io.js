'use strict';

const {
  DateNow,
  RegExpPrototypeSymbolSplit,
  StringPrototypeCodePointAt,
  StringPrototypeEndsWith,
  StringPrototypeIncludes,
  StringPrototypeSlice,
  StringPrototypeSplit,
} = primordials;

const { codes: { ERR_USE_AFTER_CLOSE } } = require('internal/errors');
const { clearScreenDown, cursorTo, moveCursor } = require('internal/readline/callbacks');
const { lineEnding, kMultilinePrompt, kBeforeEdit, kDecoder, kGetDisplayPos, kInsertString, kLine_buffer, kNormalWrite, kOnLine, kSetLine, kPrompt, kRefreshLine, kSawReturnAt, kSetRawMode, kTtyWrite, kIsMultiline, kWriteToOutput, kColorize } = require('internal/readline/interface_state');

module.exports = {
  [kRefreshLine]() {
    // line length
    const line = this[kPrompt] + this.line;
    const dispPos = this[kGetDisplayPos](line);
    const lineCols = dispPos.cols;
    const lineRows = dispPos.rows;

    // cursor position
    const cursorPos = this.getCursorPos();

    // First move to the bottom of the current line, based on cursor pos
    const prevRows = this.prevRows || 0;
    if (prevRows > 0) {
      moveCursor(this.output, 0, -prevRows);
    }

    // Cursor to left edge.
    cursorTo(this.output, 0);
    // erase data
    clearScreenDown(this.output);

    if (this[kIsMultiline]) {
      const lines = StringPrototypeSplit(this.line, '\n');
      // Write first line with normal prompt
      this[kWriteToOutput](this[kPrompt] +
                           (this[kColorize]?.(lines[0]) ?? lines[0]));

      // For continuation lines, add the "|" prefix
      for (let i = 1; i < lines.length; i++) {
        this[kWriteToOutput](`\n${kMultilinePrompt.description}` +
                             (this[kColorize]?.(lines[i]) ?? lines[i]));
      }
    } else {
      // Write the prompt and the current buffer content.
      this[kWriteToOutput](this[kPrompt] +
                           (this[kColorize]?.(this.line) ?? this.line));
    }

    // Force terminal to allocate a new line
    if (lineCols === 0) {
      this[kWriteToOutput](' ');
    }

    // Move cursor to original position.
    cursorTo(this.output, cursorPos.cols);

    const diff = lineRows - cursorPos.rows;
    if (diff > 0) {
      moveCursor(this.output, 0, -diff);
    }

    this.prevRows = cursorPos.rows;
  },

  /**
   * Closes the `readline.Interface` instance.
   * @returns {void}
   */
  close() {
    if (this.closed) return;
    this.pause();
    if (this.terminal) {
      this[kSetRawMode](false);
    }
    this.closed = true;
    this.emit('close');
  },

  /**
   * Pauses the `input` stream.
   * @returns {void | Interface}
   */
  pause() {
    if (this.closed) {
      throw new ERR_USE_AFTER_CLOSE('readline');
    }
    if (this.paused) return;
    this.input.pause();
    this.paused = true;
    this.emit('pause');
    return this;
  },

  /**
   * Resumes the `input` stream if paused.
   * @returns {void | Interface}
   */
  resume() {
    if (this.closed) {
      throw new ERR_USE_AFTER_CLOSE('readline');
    }
    if (!this.paused) return;
    this.input.resume();
    this.paused = false;
    this.emit('resume');
    return this;
  },

  /**
   * Writes either `data` or a `key` sequence identified by
   * `key` to the `output`.
   * @param {string} d
   * @param {{
   *   ctrl?: boolean;
   *   meta?: boolean;
   *   shift?: boolean;
   *   name?: string;
   *   }} [key]
   * @returns {void}
   */
  write(d, key) {
    if (this.closed) {
      throw new ERR_USE_AFTER_CLOSE('readline');
    }
    if (this.paused) this.resume();
    if (this.terminal) {
      this[kTtyWrite](d, key);
    } else {
      this[kNormalWrite](d);
    }
  },

  [kNormalWrite](b) {
    if (b === undefined) {
      return;
    }
    let string = this[kDecoder].write(b);
    if (
      this[kSawReturnAt] &&
      DateNow() - this[kSawReturnAt] <= this.crlfDelay
    ) {
      if (StringPrototypeCodePointAt(string) === 10) string = StringPrototypeSlice(string, 1);
      this[kSawReturnAt] = 0;
    }

    if (!string) {
      return;
    }

    // Split the new string chunk, not the entire line buffer: a single
    // split pass avoids allocating a match object per line ending.
    // When the chunk contains none of the rare line endings, a plain
    // string split is much cheaper than the regular expression.
    const lines =
      StringPrototypeIncludes(string, '\r') ||
      StringPrototypeIncludes(string, '\u2028') ||
      StringPrototypeIncludes(string, '\u2029') ?
        RegExpPrototypeSymbolSplit(lineEnding, string) :
        StringPrototypeSplit(string, '\n');
    const lastIndex = lines.length - 1;
    if (lastIndex === 0) {
      // No line endings this time, save what we have for next time.
      if (this[kLine_buffer]) {
        this[kLine_buffer] += string;
      } else {
        this[kLine_buffer] = string;
      }
      return;
    }

    this[kSawReturnAt] = StringPrototypeEndsWith(string, '\r') ?
      DateNow() :
      0;

    let first = lines[0];
    if (this[kLine_buffer]) {
      first = this[kLine_buffer] + first;
    }
    // Either '' or (conceivably) the unfinished portion of the next line
    this[kLine_buffer] = lines[lastIndex];
    this[kOnLine](first);
    for (let i = 1; i < lastIndex; i++) {
      this[kOnLine](lines[i]);
    }
  },

  [kInsertString](c) {
    this[kBeforeEdit](this.line, this.cursor);
    if (!this.isCompletionEnabled) {
      if (this.cursor < this.line.length) {
        const beg = StringPrototypeSlice(this.line, 0, this.cursor);
        const end = StringPrototypeSlice(
          this.line,
          this.cursor,
          this.line.length,
        );
        this.line = beg + c + end;
      } else {
        this.line += c;
      }
      this.cursor += c.length;
      if (this[kColorize] === undefined) {
        this[kWriteToOutput](c);
      } else {
        this[kRefreshLine]();
      }
      return;
    }
    if (this.cursor < this.line.length) {
      const beg = StringPrototypeSlice(this.line, 0, this.cursor);
      const end = StringPrototypeSlice(
        this.line,
        this.cursor,
        this.line.length,
      );
      this[kSetLine](beg + c + end);
      this.cursor += c.length;
      this[kRefreshLine]();
    } else {
      const oldPos = this.getCursorPos();
      this.line += c;
      this.cursor += c.length;
      const newPos = this.getCursorPos();

      if (oldPos.rows < newPos.rows || this[kColorize] !== undefined) {
        this[kRefreshLine]();
      } else {
        this[kWriteToOutput](c);
      }
    }
  },

};
