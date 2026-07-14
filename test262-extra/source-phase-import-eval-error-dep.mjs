// Fixture for ./source-phase-import-eval-error.mjs — an ordinary Source Text
// Module that throws during evaluation. Imported normally first (to cache the
// evaluation error), then via `import.source()` which must NOT replay it.
throw new Error("boom during evaluation");
