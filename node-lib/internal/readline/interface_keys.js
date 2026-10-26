'use strict';

const {
  DateNow,
  RegExpPrototypeExec,
  StringPrototypeCodePointAt,
  StringPrototypeSlice,
  SymbolAsyncIterator,
} = primordials;

const { AbortError } = require('internal/errors');
const { kEmptyObject } = require('internal/util');
const EventEmitter = require('events');
const { charLengthAt, charLengthLeft, kSubstringSearch } = require('internal/readline/utils');
const { clearScreenDown, cursorTo } = require('internal/readline/callbacks');
const { lineEnding, kLineObjectStream, kDeleteLeft, kDeleteLineLeft, kDeleteLineRight, kDeleteRight, kDeleteWordLeft, kDeleteWordRight, kHistoryNext, kMoveDownOrHistoryNext, kHistoryPrev, kMoveUpOrHistoryPrev, kInsertString, kLine, kMoveCursor, kPreviousKey, kQuestionReject, kRedo, kRefreshLine, kSawReturnAt, kSetRawMode, kTabComplete, kTtyWrite, kUndo, kIsMultiline, kWordLeft, kWordRight, kYank, kYanking, kYankPop, kPreviousCursorCols } = require('internal/readline/interface_state');
let kFirstEventParam;

module.exports = {
  // Handle a write from the tty
  [kTtyWrite](s, key) {
    const previousKey = this[kPreviousKey];
    key ||= kEmptyObject;
    this[kPreviousKey] = key;
    let shouldResetPreviousCursorCols = true;

    if (!key.meta || key.name !== 'y') {
      // Reset yanking state unless we are doing yank pop.
      this[kYanking] = false;
    }

    // Activate or deactivate substring search.
    if (
      (key.name === 'up' || key.name === 'down') &&
      !key.ctrl &&
      !key.meta &&
      !key.shift
    ) {
      if (this[kSubstringSearch] === null && !this[kIsMultiline]) {
        this[kSubstringSearch] = StringPrototypeSlice(
          this.line,
          0,
          this.cursor,
        );
      }
    } else if (this[kSubstringSearch] !== null) {
      this[kSubstringSearch] = null;
      // Reset the index in case there's no match.
      if (this.history.length === this.historyIndex) {
        this.historyIndex = -1;
      }
    }

    // Undo & Redo
    if (typeof key.sequence === 'string') {
      switch (StringPrototypeCodePointAt(key.sequence, 0)) {
        case 0x1f:
          this[kUndo]();
          return;
        case 0x1e:
          this[kRedo]();
          return;
        default:
          break;
      }
    }

    // Ignore escape key, fixes
    // https://github.com/nodejs/node-v0.x-archive/issues/2876.
    if (key.name === 'escape') return;

    if (key.ctrl && key.shift) {
      /* Control and shift pressed */
      switch (key.name) {
        // TODO(BridgeAR): The transmitted escape sequence is `\b` and that is
        // identical to <ctrl>-h. It should have a unique escape sequence.
        case 'backspace':
          this[kDeleteLineLeft]();
          break;

        case 'delete':
          this[kDeleteLineRight]();
          break;
      }
    } else if (key.ctrl) {
      /* Control key pressed */

      switch (key.name) {
        case 'c':
          if (this.listenerCount('SIGINT') > 0) {
            this.emit('SIGINT');
          } else {
            // This readline instance is finished
            this.close();
            this[kQuestionReject]?.(new AbortError('Aborted with Ctrl+C'));
          }
          break;

        case 'h': // delete left
          this[kDeleteLeft]();
          break;

        case 'd': // delete right or EOF
          if (this.cursor === 0 && this.line.length === 0) {
            // This readline instance is finished
            this.close();
            this[kQuestionReject]?.(new AbortError('Aborted with Ctrl+D'));
          } else if (this.cursor < this.line.length) {
            this[kDeleteRight]();
          }
          break;

        case 'u': // Delete from current to start of line
          this[kDeleteLineLeft]();
          break;

        case 'k': // Delete from current to end of line
          this[kDeleteLineRight]();
          break;

        case 'a': // Go to the start of the line
          this[kMoveCursor](-Infinity);
          break;

        case 'e': // Go to the end of the line
          this[kMoveCursor](+Infinity);
          break;

        case 'b': // back one character
          this[kMoveCursor](-charLengthLeft(this.line, this.cursor));
          break;

        case 'f': // Forward one character
          this[kMoveCursor](+charLengthAt(this.line, this.cursor));
          break;

        case 'l': // Clear the whole screen
          cursorTo(this.output, 0, 0);
          clearScreenDown(this.output);
          this[kRefreshLine]();
          break;

        case 'n': // next history item
          this[kHistoryNext]();
          break;

        case 'p': // Previous history item
          this[kHistoryPrev]();
          break;

        case 'y': // Yank killed string
          this[kYank]();
          break;

        case 'z':
          if (process.platform === 'win32') break;
          if (this.listenerCount('SIGTSTP') > 0) {
            this.emit('SIGTSTP');
          } else {
            process.once('SIGCONT', () => {
              // Don't raise events if stream has already been abandoned.
              if (!this.paused) {
                // Stream must be paused and resumed after SIGCONT to catch
                // SIGINT, SIGTSTP, and EOF.
                this.pause();
                this.emit('SIGCONT');
              }
              // Explicitly re-enable "raw mode" and move the cursor to
              // the correct position.
              // See https://github.com/joyent/node/issues/3295.
              this[kSetRawMode](true);
              this[kRefreshLine]();
            });
            this[kSetRawMode](false);
            process.kill(process.pid, 'SIGTSTP');
          }
          break;

        case 'w': // Delete backwards to a word boundary
        // TODO(BridgeAR): The transmitted escape sequence is `\b` and that is
        // identical to <ctrl>-h. It should have a unique escape sequence.
        // Falls through
        case 'backspace':
          this[kDeleteWordLeft]();
          break;

        case 'delete': // Delete forward to a word boundary
          this[kDeleteWordRight]();
          break;

        case 'left':
          this[kWordLeft]();
          break;

        case 'right':
          this[kWordRight]();
          break;
      }
    } else if (key.meta) {
      /* Meta key pressed */

      switch (key.name) {
        case 'b': // backward word
          this[kWordLeft]();
          break;

        case 'f': // forward word
          this[kWordRight]();
          break;

        case 'd': // delete forward word
        case 'delete':
          this[kDeleteWordRight]();
          break;

        case 'backspace': // Delete backwards to a word boundary
          this[kDeleteWordLeft]();
          break;

        case 'y': // Doing yank pop
          this[kYankPop]();
          break;
      }
    } else {
      /* No modifier keys used */

      // \r bookkeeping is only relevant if a \n comes right after.
      if (this[kSawReturnAt] && key.name !== 'enter') this[kSawReturnAt] = 0;

      switch (key.name) {
        case 'return': // Carriage return, i.e. \r
          this[kSawReturnAt] = DateNow();
          this[kLine]();
          break;

        case 'enter':
          // When key interval > crlfDelay
          if (
            this[kSawReturnAt] === 0 ||
            DateNow() - this[kSawReturnAt] > this.crlfDelay
          ) {
            this[kLine]();
          }
          this[kSawReturnAt] = 0;
          break;

        case 'backspace':
          this[kDeleteLeft]();
          break;

        case 'delete':
          this[kDeleteRight]();
          break;

        case 'left':
          // Obtain the code point to the left
          this[kMoveCursor](-charLengthLeft(this.line, this.cursor));
          break;

        case 'right':
          this[kMoveCursor](+charLengthAt(this.line, this.cursor));
          break;

        case 'home':
          this[kMoveCursor](-Infinity);
          break;

        case 'end':
          this[kMoveCursor](+Infinity);
          break;

        case 'up':
          shouldResetPreviousCursorCols = false;
          this[kMoveUpOrHistoryPrev]();
          break;

        case 'down':
          shouldResetPreviousCursorCols = false;
          this[kMoveDownOrHistoryNext]();
          break;

        case 'tab':
          // If tab completion enabled, do that...
          if (
            typeof this.completer === 'function' &&
            this.isCompletionEnabled
          ) {
            const lastKeypressWasTab =
              previousKey && previousKey.name === 'tab';
            this[kTabComplete](lastKeypressWasTab);
            break;
          }
        // falls through
        default:
          if (typeof s === 'string' && s) {
            // Erase state of previous searches.
            lineEnding.lastIndex = 0;
            let nextMatch;
            // Keep track of the end of the last match.
            let lastIndex = 0;
            while ((nextMatch = RegExpPrototypeExec(lineEnding, s)) !== null) {
              this[kInsertString](StringPrototypeSlice(s, lastIndex, nextMatch.index));
              ({ lastIndex } = lineEnding);
              this[kLine]();
              // Restore lastIndex as the call to kLine could have mutated it.
              lineEnding.lastIndex = lastIndex;
            }
            // This ensures that the last line is written if it doesn't end in a newline.
            // Note that the last line may be the first line, in which case this still works.
            this[kInsertString](StringPrototypeSlice(s, lastIndex));
          }
      }
    }
    if (shouldResetPreviousCursorCols) {
      this[kPreviousCursorCols] = -1;
    }
  },

  /**
   * Creates an `AsyncIterator` object that iterates through
   * each line in the input stream as a string.
   * @returns {AsyncIterableIterator<string>}
   */
  [SymbolAsyncIterator]() {
    if (this[kLineObjectStream] === undefined) {
      kFirstEventParam ??= require('internal/events/symbols').kFirstEventParam;
      this[kLineObjectStream] = EventEmitter.on(
        this, 'line', {
          close: ['close'],
          highWaterMark: 1024,
          [kFirstEventParam]: true,
        });
    }
    return this[kLineObjectStream];
  },
};
