'use strict';

const {
  ArrayPrototypePop,
  ArrayPrototypePush,
  ArrayPrototypeShift,
  MathCeil,
  SafeStringIterator,
  StringPrototypeSlice,
  StringPrototypeSplit,
} = primordials;

const { getStringWidth, stripVTControlCharacters } = require('internal/util/inspect');
const { kSubstringSearch } = require('internal/readline/utils');
const { moveCursor } = require('internal/readline/callbacks');
const { kMaxUndoRedoStackSize, kMultilinePrompt, kBeforeEdit, kGetDisplayPos, kHistoryNext, kMoveDownOrHistoryNext, kHistoryPrev, kMoveUpOrHistoryPrev, kMoveCursor, kSetLine, kPrompt, kPushToUndoStack, kRedo, kRedoStack, kRefreshLine, kUndo, kUndoStack, kIsMultiline, kPreviousCursorCols, kMultilineMove } = require('internal/readline/interface_state');

module.exports = {
  [kPushToUndoStack](text, cursor) {
    if (ArrayPrototypePush(this[kUndoStack], { text, cursor }) >
        kMaxUndoRedoStackSize) {
      ArrayPrototypeShift(this[kUndoStack]);
    }
  },

  [kUndo]() {
    if (this[kUndoStack].length <= 0) return;

    ArrayPrototypePush(
      this[kRedoStack],
      { text: this.line, cursor: this.cursor },
    );

    const entry = ArrayPrototypePop(this[kUndoStack]);
    this[kSetLine](entry.text);
    this.cursor = entry.cursor;

    this[kRefreshLine]();
  },

  [kRedo]() {
    if (this[kRedoStack].length <= 0) return;

    ArrayPrototypePush(
      this[kUndoStack],
      { text: this.line, cursor: this.cursor },
    );

    const entry = ArrayPrototypePop(this[kRedoStack]);
    this[kSetLine](entry.text);
    this.cursor = entry.cursor;

    this[kRefreshLine]();
  },

  [kMultilineMove](direction, splitLines, { rows, cols }) {
    const curr = splitLines[rows];
    const down = direction === 1;
    const adj = splitLines[rows + direction];
    const promptLen = kMultilinePrompt.description.length;
    let amountToMove;
    // Clamp distance to end of current + prompt + next/prev line + newline
    const clamp = down ?
      curr.length - cols + promptLen + adj.length + 1 :
      -cols + 1;
    const shouldClamp = cols > adj.length + 1;

    if (shouldClamp) {
      if (this[kPreviousCursorCols] === -1) {
        this[kPreviousCursorCols] = cols;
      }
      amountToMove = clamp;
    } else {
      if (down) {
        amountToMove = curr.length + 1;
      } else {
        amountToMove = -adj.length - 1;
      }
      if (this[kPreviousCursorCols] !== -1) {
        if (this[kPreviousCursorCols] <= adj.length) {
          amountToMove += this[kPreviousCursorCols] - cols;
          this[kPreviousCursorCols] = -1;
        } else {
          amountToMove = clamp;
        }
      }
    }

    this[kMoveCursor](amountToMove);
  },

  [kMoveDownOrHistoryNext]() {
    const cursorPos = this.getCursorPos();
    const splitLines = StringPrototypeSplit(this.line, '\n');
    if (this[kIsMultiline] && cursorPos.rows < splitLines.length - 1) {
      this[kMultilineMove](1, splitLines, cursorPos);
      return;
    }
    this[kPreviousCursorCols] = -1;
    this[kHistoryNext]();
  },

  // TODO(BridgeAR): Add underscores to the search part and a red background in
  // case no match is found. This should only be the visual part and not the
  // actual line content!
  // TODO(BridgeAR): In case the substring based search is active and the end is
  // reached, show a comment how to search the history as before. E.g., using
  // <ctrl> + N. Only show this after two/three UPs or DOWNs, not on the first
  // one.
  [kHistoryNext]() {
    if (!this.historyManager.canNavigateToNext()) { return; }

    this[kBeforeEdit](this.line, this.cursor);
    this[kSetLine](this.historyManager.navigateToNext(this[kSubstringSearch]));
    this.cursor = this.line.length; // Set cursor to end of line.
    this[kRefreshLine]();
  },

  [kMoveUpOrHistoryPrev]() {
    const cursorPos = this.getCursorPos();
    if (this[kIsMultiline] && cursorPos.rows > 0) {
      const splitLines = StringPrototypeSplit(this.line, '\n');
      this[kMultilineMove](-1, splitLines, cursorPos);
      return;
    }
    this[kPreviousCursorCols] = -1;
    this[kHistoryPrev]();
  },

  [kHistoryPrev]() {
    if (!this.historyManager.canNavigateToPrevious()) { return; }

    this[kBeforeEdit](this.line, this.cursor);
    this[kSetLine](this.historyManager.navigateToPrevious(this[kSubstringSearch]));
    this.cursor = this.line.length; // Set cursor to end of line.
    this[kRefreshLine]();
  },

  // Returns the last character's display position of the given string
  [kGetDisplayPos](str) {
    let offset = 0;
    const col = this.columns;
    let rows = 0;
    str = stripVTControlCharacters(str);

    for (const char of new SafeStringIterator(str)) {
      if (char === '\n') {
        // Rows must be incremented by 1 even if offset = 0 or col = +Infinity.
        rows += MathCeil(offset / col) || 1;
        // Only add prefix offset for continuation lines in user input (not prompts)
        offset = this[kIsMultiline] ? kMultilinePrompt.description.length : 0;
        continue;
      }
      // Tabs must be aligned by an offset of the tab size.
      if (char === '\t') {
        offset += this.tabSize - (offset % this.tabSize);
        continue;
      }
      const width = getStringWidth(char, false /* stripVTControlCharacters */);
      if (width === 0 || width === 1) {
        offset += width;
      } else {
        // width === 2
        if ((offset + 1) % col === 0) {
          offset++;
        }
        offset += 2;
      }
    }

    const cols = offset % col;
    rows += (offset - cols) / col;

    return { cols, rows };
  },

  /**
   * Returns the real position of the cursor in relation
   * to the input prompt + string.
   * @returns {{
   *   rows: number;
   *   cols: number;
   *   }}
   */
  getCursorPos() {
    const strBeforeCursor = this[kPrompt] + StringPrototypeSlice(this.line, 0, this.cursor);

    return this[kGetDisplayPos](strBeforeCursor);
  },

  // This function moves cursor dx places to the right
  // (-dx for left) and refreshes the line if it is needed.
  [kMoveCursor](dx) {
    if (dx === 0) {
      return;
    }
    const oldPos = this.getCursorPos();
    this.cursor += dx;

    // Bounds check
    if (this.cursor < 0) {
      this.cursor = 0;
    } else if (this.cursor > this.line.length) {
      this.cursor = this.line.length;
    }

    const newPos = this.getCursorPos();

    // Check if cursor stayed on the line.
    if (oldPos.rows === newPos.rows) {
      const diffWidth = newPos.cols - oldPos.cols;
      moveCursor(this.output, diffWidth, 0);
    } else {
      this[kRefreshLine]();
    }
  },

};
