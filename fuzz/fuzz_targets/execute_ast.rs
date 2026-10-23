#![no_main]

//! Structure-aware sibling of `execute`: libFuzzer bytes are decoded into a
//! JS AST, printed to source, and run through the same bytecode-vs-tree-walker
//! check (`jsse::fuzz_execute_source`), so programs get past the parser.

use jsse_fuzz::js_ast::Program;
use jsse_fuzz::js_print::print;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|program: Program| {
    jsse::fuzz_execute_source(&print(&program));
});
