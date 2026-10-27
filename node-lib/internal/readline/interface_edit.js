'use strict';

const {
  ArrayFrom,
  ArrayPrototypeFilter,
  ArrayPrototypeJoin,
  ArrayPrototypeMap,
  ArrayPrototypePop,
  ArrayPrototypeReverse,
  ArrayPrototypeUnshift,
  MathFloor,
  MathMaxApply,
  RegExpPrototypeExec,
  StringPrototypeRepeat,
  StringPrototypeReplaceAll,
  StringPrototypeSlice,
  StringPrototypeSplit,
  StringPrototypeStartsWith,
} = primordials;

const { inspect, getStringWidth } = require('internal/util/inspect');
const { charLengthAt, charLengthLeft, commonPrefix } = require('internal/readline/utils');
const { clearScreenDown, moveCursor } = require('internal/readline/callbacks');
const { kMaxLengthOfKillRing, kMultilinePrompt, kAddHistory, kBeforeEdit, kDeleteLeft, kDeleteLineLeft, kDeleteLineRight, kDeleteRight, kDeleteWordLeft, kDeleteWordRight, kGetDisplayPos, kInsertString, kLine, kKillRing, kKillRingCursor, kMoveCursor, kOnLine, kSetLine, kPrompt, kPushToKillRing, kRedoStack, kRefreshLine, kTabComplete, kTabCompleter, kTtyWrite, kUndoStack, kWordLeft, kWordRight, kWriteToOutput, kYank, kYanking, kYankPop, kSavePreviousState, kRestorePreviousState, kPreviousLine, kPreviousCursor, kPreviousPrevRows, kAddNewLineOnTTY } = require('internal/readline/interface_state');

module.exports = {
  async [kTabComplete](lastKeypressWasTab) {
    this.pause();
    const string = StringPrototypeSlice(this.line, 0, this.cursor);
    let value;
    try {
      value = await this.completer(string);
    } catch (err) {
      this[kWriteToOutput](`Tab completion error: ${inspect(err)}`);
      return;
    } finally {
      this.resume();
    }
    this[kTabCompleter](lastKeypressWasTab, value);
  },

  [kTabCompleter](lastKeypressWasTab, { 0: completions, 1: completeOn }) {
    // Result and the text that was completed.

    if (!completions || completions.length === 0) {
      return;
    }

    // If there is a common prefix to all matches, then apply that portion.
    const prefix = commonPrefix(
      ArrayPrototypeFilter(completions, (e) => e !== ''),
    );
    if (StringPrototypeStartsWith(prefix, completeOn) &&
        prefix.length > completeOn.length) {
      this[kInsertString](StringPrototypeSlice(prefix, completeOn.length));
      return;
    } else if (!StringPrototypeStartsWith(completeOn, prefix)) {
      this[kSetLine](StringPrototypeSlice(this.line,
                                          0,
                                          this.cursor - completeOn.length) +
                  prefix +
                  StringPrototypeSlice(this.line,
                                       this.cursor,
                                       this.line.length));
      this.cursor = this.cursor - completeOn.length + prefix.length;
      this[kRefreshLine]();
      return;
    }

    if (!lastKeypressWasTab) {
      return;
    }

    this[kBeforeEdit](this.line, this.cursor);

    // Apply/show completions.
    const completionsWidth = ArrayPrototypeMap(completions, (e) =>
      getStringWidth(e),
    );
    const width = MathMaxApply(completionsWidth) + 2; // 2 space padding
    let maxColumns = MathFloor(this.columns / width) || 1;
    if (maxColumns === Infinity) {
      maxColumns = 1;
    }
    let output = '\r\n';
    let lineIndex = 0;
    let whitespace = 0;
    for (let i = 0; i < completions.length; i++) {
      const completion = completions[i];
      if (completion === '' || lineIndex === maxColumns) {
        output += '\r\n';
        lineIndex = 0;
        whitespace = 0;
      } else {
        output += StringPrototypeRepeat(' ', whitespace);
      }
      if (completion !== '') {
        output += completion;
        whitespace = width - completionsWidth[i];
        lineIndex++;
      } else {
        output += '\r\n';
      }
    }
    if (lineIndex !== 0) {
      output += '\r\n\r\n';
    }
    this[kWriteToOutput](output);
    this[kRefreshLine]();
  },

  [kWordLeft]() {
    if (this.cursor > 0) {
      // Reverse the string and match a word near beginning
      // to avoid quadratic time complexity
      const leading = StringPrototypeSlice(this.line, 0, this.cursor);
      const reversed = ArrayPrototypeJoin(
        ArrayPrototypeReverse(ArrayFrom(leading)),
        '',
      );
      const match = RegExpPrototypeExec(/^\s*(?:[^\w\s]+|\w+)?/, reversed);
      this[kMoveCursor](-match[0].length);
    }
  },

  [kWordRight]() {
    if (this.cursor < this.line.length) {
      const trailing = StringPrototypeSlice(this.line, this.cursor);
      const match = RegExpPrototypeExec(/^(?:\s+|[^\w\s]+|\w+)\s*/, trailing);
      this[kMoveCursor](match[0].length);
    }
  },

  [kDeleteLeft]() {
    if (this.cursor > 0 && this.line.length > 0) {
      this[kBeforeEdit](this.line, this.cursor);
      // The number of UTF-16 units comprising the character to the left
      const charSize = charLengthLeft(this.line, this.cursor);
      this.line =
        StringPrototypeSlice(this.line, 0, this.cursor - charSize) +
        StringPrototypeSlice(this.line, this.cursor, this.line.length);

      this.cursor -= charSize;
      this[kRefreshLine]();
    }
  },

  [kDeleteRight]() {
    if (this.cursor < this.line.length) {
      this[kBeforeEdit](this.line, this.cursor);
      // The number of UTF-16 units comprising the character to the left
      const charSize = charLengthAt(this.line, this.cursor);
      this.line =
        StringPrototypeSlice(this.line, 0, this.cursor) +
        StringPrototypeSlice(
          this.line,
          this.cursor + charSize,
          this.line.length,
        );
      this[kRefreshLine]();
    }
  },

  [kDeleteWordLeft]() {
    if (this.cursor > 0) {
      this[kBeforeEdit](this.line, this.cursor);
      // Reverse the string and match a word near beginning
      // to avoid quadratic time complexity
      let leading = StringPrototypeSlice(this.line, 0, this.cursor);
      const reversed = ArrayPrototypeJoin(
        ArrayPrototypeReverse(ArrayFrom(leading)),
        '',
      );
      const match = RegExpPrototypeExec(/^\s*(?:[^\w\s]+|\w+)?/, reversed);
      leading = StringPrototypeSlice(
        leading,
        0,
        leading.length - match[0].length,
      );
      this.line =
        leading +
        StringPrototypeSlice(this.line, this.cursor, this.line.length);
      this.cursor = leading.length;
      this[kRefreshLine]();
    }
  },

  [kDeleteWordRight]() {
    if (this.cursor < this.line.length) {
      this[kBeforeEdit](this.line, this.cursor);
      const trailing = StringPrototypeSlice(this.line, this.cursor);
      const match = RegExpPrototypeExec(/^(?:\s+|\W+|\w+)\s*/, trailing);
      this.line =
        StringPrototypeSlice(this.line, 0, this.cursor) +
        StringPrototypeSlice(trailing, match[0].length);
      this[kRefreshLine]();
    }
  },

  [kDeleteLineLeft]() {
    this[kBeforeEdit](this.line, this.cursor);
    const del = StringPrototypeSlice(this.line, 0, this.cursor);
    this[kSetLine](StringPrototypeSlice(this.line, this.cursor));
    this.cursor = 0;
    this[kPushToKillRing](del);
    this[kRefreshLine]();
  },

  [kDeleteLineRight]() {
    this[kBeforeEdit](this.line, this.cursor);
    const del = StringPrototypeSlice(this.line, this.cursor);
    this[kSetLine](StringPrototypeSlice(this.line, 0, this.cursor));
    this[kPushToKillRing](del);
    this[kRefreshLine]();
  },

  [kPushToKillRing](del) {
    if (!del || del === this[kKillRing][0]) return;
    ArrayPrototypeUnshift(this[kKillRing], del);
    this[kKillRingCursor] = 0;
    while (this[kKillRing].length > kMaxLengthOfKillRing)
      ArrayPrototypePop(this[kKillRing]);
  },

  [kYank]() {
    if (this[kKillRing].length > 0) {
      this[kYanking] = true;
      this[kInsertString](this[kKillRing][this[kKillRingCursor]]);
    }
  },

  [kYankPop]() {
    if (!this[kYanking]) {
      return;
    }
    if (this[kKillRing].length > 1) {
      const lastYank = this[kKillRing][this[kKillRingCursor]];
      this[kKillRingCursor]++;
      if (this[kKillRingCursor] >= this[kKillRing].length) {
        this[kKillRingCursor] = 0;
      }
      const currentYank = this[kKillRing][this[kKillRingCursor]];
      const head =
            StringPrototypeSlice(this.line, 0, this.cursor - lastYank.length);
      const tail =
            StringPrototypeSlice(this.line, this.cursor);
      this[kSetLine](head + currentYank + tail);
      this.cursor = head.length + currentYank.length;
      this[kRefreshLine]();
    }
  },

  [kSavePreviousState]() {
    this[kPreviousLine] = this.line;
    this[kPreviousCursor] = this.cursor;
    this[kPreviousPrevRows] = this.prevRows;
  },

  [kRestorePreviousState]() {
    this[kSetLine](this[kPreviousLine]);
    this.cursor = this[kPreviousCursor];
    this.prevRows = this[kPreviousPrevRows];
  },

  clearLine() {
    this[kMoveCursor](+Infinity);
    this[kWriteToOutput]('\r\n');
    this[kSetLine]('');
    this.cursor = 0;
    this.prevRows = 0;
  },

  [kLine]() {
    this[kSavePreviousState]();
    const line = this[kAddHistory]();
    this[kUndoStack] = [];
    this[kRedoStack] = [];
    this.clearLine();
    this[kOnLine](line);
  },


  // TODO(puskin94): edit [kTtyWrite] to make call this function on a new key combination
  //                 to make it add a new line in the middle of a "complete" multiline.
  //                 I tried with shift + enter but it is not detected. Find a new one.
  //                 Make sure to call this[kSavePreviousState](); && this.clearLine();
  //                 before calling this[kAddNewLineOnTTY] to simulate what [kLine] is doing.

  // When this function is called, the actual cursor is at the very end of the whole string,
  // No matter where the new line was entered.
  // This function should only be used when the output is a TTY
  [kAddNewLineOnTTY]() {
    // Restore terminal state and store current line
    this[kRestorePreviousState]();
    const originalLine = this.line;

    // Split the line at the current cursor position
    const beforeCursor = StringPrototypeSlice(this.line, 0, this.cursor);
    let afterCursor = StringPrototypeSlice(this.line, this.cursor, this.line.length);

    // Add the new line where the cursor is at
    this[kSetLine](`${beforeCursor}\n${afterCursor}`);

    // To account for the new line
    this.cursor += 1;

    const hasContentAfterCursor = afterCursor.length > 0;
    const cursorIsNotOnFirstLine = this.prevRows > 0;
    let needsRewriteFirstLine = false;

    // Handle cursor positioning based on different scenarios
    if (hasContentAfterCursor) {
      const splitBeg = StringPrototypeSplit(beforeCursor, '\n');
      // Determine if we need to rewrite the first line
      needsRewriteFirstLine = splitBeg.length < 2;

      // If the cursor is not on the first line
      if (cursorIsNotOnFirstLine) {
        const splitEnd = StringPrototypeSplit(afterCursor, '\n');

        // If the cursor when I pressed enter was at least on the second line
        // I need to completely erase the line where the cursor was pressed because it is possible
        // That it was pressed in the middle of the line, hence I need to write the whole line.
        // To achieve that, I need to reach the line above the current line coming from the end
        const dy = splitEnd.length + 1;

        // Calculate how many Xs we need to move on the right to get to the end of the line
        const dxEndOfLineAbove = (splitBeg[splitBeg.length - 2] || '').length + kMultilinePrompt.description.length;
        moveCursor(this.output, dxEndOfLineAbove, -dy);

        // This is the line that was split in the middle
        // Just add it to the rest of the line that will be printed later
        afterCursor = `${splitBeg[splitBeg.length - 1]}\n${afterCursor}`;
      } else {
        // Otherwise, go to the very beginning of the first line and erase everything
        const dy = StringPrototypeSplit(originalLine, '\n').length;
        moveCursor(this.output, 0, -dy);
      }

      // Erase from the cursor to the end of the line
      clearScreenDown(this.output);

      if (cursorIsNotOnFirstLine) {
        this[kWriteToOutput]('\n');
      }
    }

    if (needsRewriteFirstLine) {
      this[kWriteToOutput](`${this[kPrompt]}${beforeCursor}\n${kMultilinePrompt.description}`);
    } else {
      this[kWriteToOutput](kMultilinePrompt.description);
    }

    // Write the rest and restore the cursor to where the user left it
    if (hasContentAfterCursor) {
      // Save the cursor pos, we need to come back here
      const oldCursor = this.getCursorPos();

      // Write everything after the cursor which has been deleted by clearScreenDown
      const formattedEndContent = StringPrototypeReplaceAll(
        afterCursor,
        '\n',
        `\n${kMultilinePrompt.description}`,
      );

      this[kWriteToOutput](formattedEndContent);

      const newCursor = this[kGetDisplayPos](this.line);

      // Go back to where the cursor was, with relative movement
      moveCursor(this.output, oldCursor.cols - newCursor.cols, oldCursor.rows - newCursor.rows);

      // Setting how many rows we have on top of the cursor
      // Necessary for kRefreshLine
      this.prevRows = oldCursor.rows;
    } else {
      // Setting how many rows we have on top of the cursor
      // Necessary for kRefreshLine
      this.prevRows = StringPrototypeSplit(this.line, '\n').length - 1;
    }
  },

};
