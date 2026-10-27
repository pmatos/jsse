'use strict';

const { Symbol } = primordials;

const kMaxUndoRedoStackSize = 2048;
const kMincrlfDelay = 100;
const lineEnding = /\r?\n|\r(?!\n)|\u2028|\u2029/g;
const kLineObjectStream = Symbol('line object stream');
const kQuestionCancel = Symbol('kQuestionCancel');
const kQuestion = Symbol('kQuestion');
const ESCAPE_CODE_TIMEOUT = 500;
const kMaxLengthOfKillRing = 32;
const kMultilinePrompt = Symbol('| ');
const kAddHistory = Symbol('_addHistory');
const kBeforeEdit = Symbol('_beforeEdit');
const kDecoder = Symbol('_decoder');
const kDeleteLeft = Symbol('_deleteLeft');
const kDeleteLineLeft = Symbol('_deleteLineLeft');
const kDeleteLineRight = Symbol('_deleteLineRight');
const kDeleteRight = Symbol('_deleteRight');
const kDeleteWordLeft = Symbol('_deleteWordLeft');
const kDeleteWordRight = Symbol('_deleteWordRight');
const kGetDisplayPos = Symbol('_getDisplayPos');
const kHistoryNext = Symbol('_historyNext');
const kMoveDownOrHistoryNext = Symbol('_moveDownOrHistoryNext');
const kHistoryPrev = Symbol('_historyPrev');
const kMoveUpOrHistoryPrev = Symbol('_moveUpOrHistoryPrev');
const kInsertString = Symbol('_insertString');
const kLine = Symbol('_line');
const kLine_buffer = Symbol('_line_buffer');
const kKillRing = Symbol('_killRing');
const kKillRingCursor = Symbol('_killRingCursor');
const kMoveCursor = Symbol('_moveCursor');
const kNormalWrite = Symbol('_normalWrite');
const kOldPrompt = Symbol('_oldPrompt');
const kOnLine = Symbol('_onLine');
const kSetLine = Symbol('_setLine');
const kPreviousKey = Symbol('_previousKey');
const kPrompt = Symbol('_prompt');
const kPushToKillRing = Symbol('_pushToKillRing');
const kPushToUndoStack = Symbol('_pushToUndoStack');
const kQuestionCallback = Symbol('_questionCallback');
const kLastCommandErrored = Symbol('_lastCommandErrored');
const kQuestionReject = Symbol('_questionReject');
const kRedo = Symbol('_redo');
const kRedoStack = Symbol('_redoStack');
const kRefreshLine = Symbol('_refreshLine');
const kSawKeyPress = Symbol('_sawKeyPress');
const kSawReturnAt = Symbol('_sawReturnAt');
const kSetRawMode = Symbol('_setRawMode');
const kTabComplete = Symbol('_tabComplete');
const kTabCompleter = Symbol('_tabCompleter');
const kTtyWrite = Symbol('_ttyWrite');
const kUndo = Symbol('_undo');
const kUndoStack = Symbol('_undoStack');
const kIsMultiline = Symbol('_isMultiline');
const kWordLeft = Symbol('_wordLeft');
const kWordRight = Symbol('_wordRight');
const kWriteToOutput = Symbol('_writeToOutput');
const kYank = Symbol('_yank');
const kYanking = Symbol('_yanking');
const kYankPop = Symbol('_yankPop');
const kSavePreviousState = Symbol('_savePreviousState');
const kRestorePreviousState = Symbol('_restorePreviousState');
const kPreviousLine = Symbol('_previousLine');
const kPreviousCursor = Symbol('_previousCursor');
const kPreviousCursorCols = Symbol('_previousCursorCols');
const kMultilineMove = Symbol('_multilineMove');
const kPreviousPrevRows = Symbol('_previousPrevRows');
const kAddNewLineOnTTY = Symbol('_addNewLineOnTTY');
const kColorize = Symbol('_colorize');

module.exports = {
  kMaxUndoRedoStackSize,
  kMincrlfDelay,
  lineEnding,
  kLineObjectStream,
  kQuestionCancel,
  kQuestion,
  ESCAPE_CODE_TIMEOUT,
  kMaxLengthOfKillRing,
  kMultilinePrompt,
  kAddHistory,
  kBeforeEdit,
  kDecoder,
  kDeleteLeft,
  kDeleteLineLeft,
  kDeleteLineRight,
  kDeleteRight,
  kDeleteWordLeft,
  kDeleteWordRight,
  kGetDisplayPos,
  kHistoryNext,
  kMoveDownOrHistoryNext,
  kHistoryPrev,
  kMoveUpOrHistoryPrev,
  kInsertString,
  kLine,
  kLine_buffer,
  kKillRing,
  kKillRingCursor,
  kMoveCursor,
  kNormalWrite,
  kOldPrompt,
  kOnLine,
  kSetLine,
  kPreviousKey,
  kPrompt,
  kPushToKillRing,
  kPushToUndoStack,
  kQuestionCallback,
  kLastCommandErrored,
  kQuestionReject,
  kRedo,
  kRedoStack,
  kRefreshLine,
  kSawKeyPress,
  kSawReturnAt,
  kSetRawMode,
  kTabComplete,
  kTabCompleter,
  kTtyWrite,
  kUndo,
  kUndoStack,
  kIsMultiline,
  kWordLeft,
  kWordRight,
  kWriteToOutput,
  kYank,
  kYanking,
  kYankPop,
  kSavePreviousState,
  kRestorePreviousState,
  kPreviousLine,
  kPreviousCursor,
  kPreviousCursorCols,
  kMultilineMove,
  kPreviousPrevRows,
  kAddNewLineOnTTY,
  kColorize,
};
