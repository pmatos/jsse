'use strict';

const {
  FunctionPrototypeCall,
  MathMax,
  NumberIsFinite,
  ObjectDefineProperties,
  ObjectSetPrototypeOf,
  StringPrototypeCodePointAt,
  StringPrototypeIncludes,
  SymbolDispose,
  ObjectDefineProperty,
  ReflectOwnKeys,
} = primordials;

const { codes: { ERR_INVALID_ARG_VALUE, ERR_USE_AFTER_CLOSE } } = require('internal/errors');
const { validateAbortSignal, validateString, validateUint32 } = require('internal/validators');
const { assignFunctionName } = require('internal/util');
const EventEmitter = require('events');
const { addAbortListener } = require('internal/events/abort_listener');
const { kSubstringSearch } = require('internal/readline/utils');
const { StringDecoder } = require('internal/readline/decoder');
const { ReplHistory } = require('internal/repl/history');
const { kMincrlfDelay, kLineObjectStream, kQuestionCancel, kQuestion, ESCAPE_CODE_TIMEOUT, kMaxLengthOfKillRing, kMultilinePrompt, kAddHistory, kBeforeEdit, kDecoder, kDeleteLeft, kDeleteLineLeft, kDeleteLineRight, kDeleteRight, kDeleteWordLeft, kDeleteWordRight, kGetDisplayPos, kHistoryNext, kHistoryPrev, kInsertString, kLine, kLine_buffer, kKillRing, kKillRingCursor, kMoveCursor, kNormalWrite, kOldPrompt, kOnLine, kSetLine, kPreviousKey, kPrompt, kPushToUndoStack, kQuestionCallback, kLastCommandErrored, kQuestionReject, kRedoStack, kRefreshLine, kSawKeyPress, kSawReturnAt, kSetRawMode, kTabComplete, kTabCompleter, kTtyWrite, kUndoStack, kIsMultiline, kWordLeft, kWordRight, kWriteToOutput, kRestorePreviousState, kPreviousCursorCols, kAddNewLineOnTTY, kColorize } = require('internal/readline/interface_state');
let emitKeypressEvents;

function InterfaceConstructor(input, output, completer, terminal) {
  this[kSawReturnAt] = 0;
  // TODO(BridgeAR): Document this property. The name is not ideal, so we
  // might want to expose an alias and document that instead.
  this.isCompletionEnabled = true;
  this[kSawKeyPress] = false;
  this[kColorize] = undefined;
  this[kPreviousKey] = null;
  this.escapeCodeTimeout = ESCAPE_CODE_TIMEOUT;
  this.tabSize = 8;

  FunctionPrototypeCall(EventEmitter, this);

  let crlfDelay;
  let prompt = '> ';
  let signal;
  let historyOptions;

  if (input?.input) {
    // An options object was given
    output = input.output;
    completer = input.completer;
    terminal = input.terminal;
    signal = input.signal;

    // It is possible to configure the history through the input object
    const historySize = input.historySize;
    const history = input.history;
    const removeHistoryDuplicates = input.removeHistoryDuplicates;
    this[kColorize] = input.colorize;

    if (input.tabSize !== undefined) {
      validateUint32(input.tabSize, 'tabSize', true);
      this.tabSize = input.tabSize;
    }
    if (input.prompt !== undefined) {
      prompt = input.prompt;
    }
    if (input.escapeCodeTimeout !== undefined) {
      if (NumberIsFinite(input.escapeCodeTimeout)) {
        this.escapeCodeTimeout = input.escapeCodeTimeout;
      } else {
        throw new ERR_INVALID_ARG_VALUE(
          'input.escapeCodeTimeout',
          this.escapeCodeTimeout,
        );
      }
    }

    if (signal) {
      validateAbortSignal(signal, 'options.signal');
    }

    crlfDelay = input.crlfDelay;
    input = input.input;

    historyOptions = {
      __proto__: null,
      size: historySize,
      history,
      removeHistoryDuplicates,
    };
  }

  this.setupHistoryManager(historyOptions ?? input);

  if (completer !== undefined && typeof completer !== 'function') {
    throw new ERR_INVALID_ARG_VALUE('completer', completer);
  }

  // Backwards compat; check the isTTY prop of the output stream
  //  when `terminal` was not specified
  if (terminal === undefined && !(output === null || output === undefined)) {
    terminal = !!output.isTTY;
  }

  const self = this;

  this.line = '';
  this[kIsMultiline] = false;
  this[kSubstringSearch] = null;
  this.output = output;
  this.input = input;
  this[kUndoStack] = [];
  this[kRedoStack] = [];
  this[kPreviousCursorCols] = -1;

  // The kill ring is a global list of blocks of text that were previously
  // killed (deleted). If its size exceeds kMaxLengthOfKillRing, the oldest
  // element will be removed to make room for the latest deletion. With kill
  // ring, users are able to recall (yank) or cycle (yank pop) among previously
  // killed texts, quite similar to the behavior of Emacs.
  this[kKillRing] = [];
  this[kKillRingCursor] = 0;

  this.crlfDelay = crlfDelay ?
    MathMax(kMincrlfDelay, crlfDelay) :
    kMincrlfDelay;
  this.completer = completer;

  this.setPrompt(prompt);

  this.terminal = !!terminal;

  function onerror(err) {
    self.emit('error', err);
  }

  function ondata(data) {
    self[kNormalWrite](data);
  }

  function onend() {
    if (
      typeof self[kLine_buffer] === 'string' &&
        self[kLine_buffer].length > 0
    ) {
      self.emit('line', self[kLine_buffer]);
    }
    self.close();
  }

  function ontermend() {
    if (typeof self.line === 'string' && self.line.length > 0) {
      self.emit('line', self.line);
    }
    self.close();
  }

  function onkeypress(s, key) {
    self[kTtyWrite](s, key);
    if (key?.sequence) {
      // If the key.sequence is half of a surrogate pair
      // (>= 0xd800 and <= 0xdfff), refresh the line so
      // the character is displayed appropriately.
      const ch = StringPrototypeCodePointAt(key.sequence, 0);
      if (ch >= 0xd800 && ch <= 0xdfff) self[kRefreshLine]();
    }
  }

  function onresize() {
    self[kRefreshLine]();
  }

  this[kLineObjectStream] = undefined;

  input.on('error', onerror);

  if (!this.terminal) {
    function onSelfCloseWithoutTerminal() {
      input.removeListener('data', ondata);
      input.removeListener('error', onerror);
      input.removeListener('end', onend);
    }

    input.on('data', ondata);
    input.on('end', onend);
    self.once('close', onSelfCloseWithoutTerminal);
    this[kDecoder] = new StringDecoder('utf8');
  } else {
    function onSelfCloseWithTerminal() {
      input.removeListener('keypress', onkeypress);
      input.removeListener('error', onerror);
      input.removeListener('end', ontermend);
      if (output !== null && output !== undefined) {
        output.removeListener('resize', onresize);
      }
    }

    emitKeypressEvents ??= require('internal/readline/emitKeypressEvents');
    emitKeypressEvents(input, this);

    // `input` usually refers to stdin
    input.on('keypress', onkeypress);
    input.on('end', ontermend);

    this[kSetRawMode](true);
    this.terminal = true;

    // Cursor position on the line.
    this.cursor = 0;

    if (output !== null && output !== undefined)
      output.on('resize', onresize);

    self.once('close', onSelfCloseWithTerminal);
  }

  if (signal) {
    const onAborted = () => self.close();
    if (signal.aborted) {
      process.nextTick(onAborted);
    } else {
      const disposable = addAbortListener(signal, onAborted);
      self.once('close', disposable[SymbolDispose]);
    }
  }

  // Current line
  this[kSetLine]('');

  input.resume();
}

ObjectSetPrototypeOf(InterfaceConstructor.prototype, EventEmitter.prototype);
ObjectSetPrototypeOf(InterfaceConstructor, EventEmitter);

// Shared descriptors for the history accessors defined on each instance.
// Hoisted to avoid allocating fresh closures on every construction.
const kHistoryAccessorDescriptors = {
  __proto__: null,
  history: {
    __proto__: null, configurable: true, enumerable: true,
    get() { return this.historyManager.history; },
    set(newHistory) { return this.historyManager.history = newHistory; },
  },
  historyIndex: {
    __proto__: null, configurable: true, enumerable: true,
    get() { return this.historyManager.index; },
    set(historyIndex) { return this.historyManager.index = historyIndex; },
  },
  historySize: {
    __proto__: null, configurable: true, enumerable: true,
    get() { return this.historyManager.size; },
  },
  isFlushing: {
    __proto__: null, configurable: true, enumerable: true,
    get() { return this.historyManager.isFlushing; },
  },
};

class Interface extends InterfaceConstructor {
  get columns() {
    if (this.output?.columns) return this.output.columns;
    return Infinity;
  }

  /**
   * Sets the prompt written to the output.
   * @param {string} prompt
   * @returns {void}
   */
  setPrompt(prompt) {
    this[kPrompt] = prompt;
  }

  /**
   * Returns the current prompt used by `rl.prompt()`.
   * @returns {string}
   */
  getPrompt() {
    return this[kPrompt];
  }

  setupHistoryManager(options) {
    this.historyManager = new ReplHistory(this, options);

    if (options.onHistoryFileLoaded) {
      this.historyManager.initialize(options.onHistoryFileLoaded);
    }

    ObjectDefineProperties(this, kHistoryAccessorDescriptors);
  }

  [kSetRawMode](mode) {
    const wasInRawMode = this.input.isRaw;

    if (typeof this.input.setRawMode === 'function') {
      this.input.setRawMode(mode);
    }

    return wasInRawMode;
  }

  /**
   * Writes the configured `prompt` to a new line in `output`.
   * @param {boolean} [preserveCursor]
   * @returns {void}
   */
  prompt(preserveCursor) {
    if (this.paused) this.resume();
    if (this.terminal && process.env.TERM !== 'dumb') {
      if (!preserveCursor) this.cursor = 0;
      this[kRefreshLine]();
    } else {
      this[kWriteToOutput](this[kPrompt]);
    }
  }

  [kQuestion](query, cb) {
    if (this.closed) {
      throw new ERR_USE_AFTER_CLOSE('readline');
    }
    if (this[kQuestionCallback]) {
      this.prompt();
    } else {
      this[kOldPrompt] = this[kPrompt];
      this.setPrompt(query);
      this[kQuestionCallback] = cb;
      this.prompt();
    }
  }

  [kSetLine](line = '') {
    this.line = line;
    this[kIsMultiline] = StringPrototypeIncludes(line, '\n');
  }

  [kOnLine](line) {
    if (this[kQuestionCallback]) {
      const cb = this[kQuestionCallback];
      this[kQuestionCallback] = null;
      this.setPrompt(this[kOldPrompt]);
      cb(line);
    } else {
      this.emit('line', line);
    }
  }

  [kBeforeEdit](oldText, oldCursor) {
    this[kPushToUndoStack](oldText, oldCursor);
  }

  [kQuestionCancel]() {
    if (this[kQuestionCallback]) {
      this[kQuestionCallback] = null;
      this.setPrompt(this[kOldPrompt]);
      this.clearLine();
    }
  }

  [kWriteToOutput](stringToWrite) {
    validateString(stringToWrite, 'stringToWrite');

    if (this.output !== null && this.output !== undefined) {
      this.output.write(stringToWrite);
    }
  }

  [kAddHistory]() {
    return this.historyManager.addHistory(this[kIsMultiline], this[kLastCommandErrored]);
  }

}

function defineMethods(proto, methods) {
  for (const key of ReflectOwnKeys(methods)) {
    ObjectDefineProperty(proto, key, {
      __proto__: null,
      value: methods[key],
      writable: true,
      enumerable: false,
      configurable: true,
    });
  }
}

defineMethods(Interface.prototype, require('internal/readline/interface_io'));
defineMethods(Interface.prototype, require('internal/readline/interface_edit'));
defineMethods(Interface.prototype, require('internal/readline/interface_nav'));
defineMethods(Interface.prototype, require('internal/readline/interface_keys'));
Interface.prototype[SymbolDispose] = assignFunctionName(SymbolDispose, function() {
  this.close();
});

module.exports = {
  Interface,
  InterfaceConstructor,
  kAddHistory,
  kDecoder,
  kDeleteLeft,
  kDeleteLineLeft,
  kDeleteLineRight,
  kDeleteRight,
  kDeleteWordLeft,
  kDeleteWordRight,
  kGetDisplayPos,
  kHistoryNext,
  kHistoryPrev,
  kInsertString,
  kIsMultiline,
  kLine,
  kLine_buffer,
  kMoveCursor,
  kNormalWrite,
  kOldPrompt,
  kOnLine,
  kSetLine,
  kPreviousKey,
  kPrompt,
  kQuestion,
  kQuestionCallback,
  kQuestionCancel,
  kQuestionReject,
  kRefreshLine,
  kSawKeyPress,
  kSawReturnAt,
  kSetRawMode,
  kTabComplete,
  kTabCompleter,
  kTtyWrite,
  kWordLeft,
  kWordRight,
  kWriteToOutput,
  kMultilinePrompt,
  kRestorePreviousState,
  kAddNewLineOnTTY,
  kLastCommandErrored,
};
