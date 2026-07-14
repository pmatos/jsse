// Fixture for ./source-phase-import-module-source.mjs — an ordinary Source
// Text Module. Its [[ModuleSource]] is empty, so `import.source()` of it must
// reject with a SyntaxError (GetModuleSource, §16.2.1.7.2).
export const plain = 1;
