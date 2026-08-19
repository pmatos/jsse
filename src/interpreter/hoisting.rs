//! Static-semantics analysis of a statement list for declaration hoisting.
//!
//! Pure functions over the AST used by `EvalDeclarationInstantiation`
//! (§19.2.1.4) and by block/global declaration processing: collecting the
//! `var`-declared names and the hoistable top-level function declarations of a
//! body. Kept free of `Interpreter` state so the subtle scope-traversal rules
//! (descend into blocks/`if`/loops/`try` but *not* into nested functions) are
//! unit-testable in isolation. Distinct from the runtime per-body hoisting
//! *cache* on `Interpreter` (#72), which memoises the output of this analysis.

use crate::ast::{ForInOfLeft, ForInit, FunctionDecl, Statement, VarKind};
use std::collections::HashSet;

/// Unwrap a (possibly label-wrapped) function declaration. A `FunctionDeclaration`
/// nested under any number of `Labeled` statements is still a function
/// declaration for hoisting purposes (§14.13.4); anything else is not.
pub(crate) fn unwrap_labeled_function(stmt: &Statement) -> Option<&FunctionDecl> {
    match stmt {
        Statement::FunctionDeclaration(f) => Some(f),
        Statement::Labeled(_, inner) => unwrap_labeled_function(inner),
        _ => None,
    }
}

/// Collect the top-level `var`-declared names of a statement list, descending
/// into nested statements (blocks, `if`, loops, `switch`, `try`, labels,
/// `with`) but never into nested function bodies. Order and duplicates mirror
/// source order; de-duplication is the caller's responsibility.
pub(crate) fn collect_var_names(stmts: &[Statement]) -> Vec<String> {
    let mut names = Vec::new();
    for stmt in stmts {
        collect_var_names_from_stmt(stmt, &mut names);
    }
    names
}

/// Collect the top-level hoistable function declarations of a statement list
/// (only top level — including under labels — never inside nested blocks), with
/// the spec's "keep the last declaration of each name" de-duplication
/// (§19.2.1.4 / Annex B). The returned order is the consumption contract of
/// `EvalDeclarationInstantiation`: reverse of source order, keeping, for each
/// name, its last source occurrence.
pub(crate) fn collect_function_decls(stmts: &[Statement]) -> Vec<FunctionDecl> {
    let mut funcs = Vec::new();
    for stmt in stmts {
        if let Some(f) = unwrap_labeled_function(stmt) {
            funcs.push(f.clone());
        }
    }
    // Per spec: reverse order, keep last occurrence of each name.
    funcs.reverse();
    let mut seen = HashSet::new();
    funcs.retain(|f| seen.insert(f.name.clone()));
    funcs
}

fn collect_var_names_from_stmt(stmt: &Statement, names: &mut Vec<String>) {
    match stmt {
        Statement::Variable(decl) if decl.kind == VarKind::Var => {
            for d in &decl.declarations {
                d.pattern.bound_names(names);
            }
        }
        Statement::Block(stmts) => {
            for s in stmts {
                collect_var_names_from_stmt(s, names);
            }
        }
        Statement::If(i) => {
            collect_var_names_from_stmt(&i.consequent, names);
            if let Some(alt) = &i.alternate {
                collect_var_names_from_stmt(alt, names);
            }
        }
        Statement::While(w) => collect_var_names_from_stmt(&w.body, names),
        Statement::DoWhile(d) => collect_var_names_from_stmt(&d.body, names),
        Statement::For(f) => {
            if let Some(ForInit::Variable(decl)) = &f.init
                && decl.kind == VarKind::Var
            {
                for d in &decl.declarations {
                    d.pattern.bound_names(names);
                }
            }
            collect_var_names_from_stmt(&f.body, names);
        }
        Statement::ForIn(fi) => {
            if let ForInOfLeft::Variable(decl) = &fi.left
                && decl.kind == VarKind::Var
            {
                for d in &decl.declarations {
                    d.pattern.bound_names(names);
                }
            }
            collect_var_names_from_stmt(&fi.body, names);
        }
        Statement::ForOf(fo) => {
            if let ForInOfLeft::Variable(decl) = &fo.left
                && decl.kind == VarKind::Var
            {
                for d in &decl.declarations {
                    d.pattern.bound_names(names);
                }
            }
            collect_var_names_from_stmt(&fo.body, names);
        }
        Statement::Switch(sw) => {
            for case in &sw.cases {
                for s in &case.consequent {
                    collect_var_names_from_stmt(s, names);
                }
            }
        }
        Statement::Try(t) => {
            for s in &t.block {
                collect_var_names_from_stmt(s, names);
            }
            if let Some(handler) = &t.handler {
                for s in &handler.body {
                    collect_var_names_from_stmt(s, names);
                }
            }
            if let Some(finalizer) = &t.finalizer {
                for s in finalizer {
                    collect_var_names_from_stmt(s, names);
                }
            }
        }
        Statement::Labeled(_, inner) => {
            collect_var_names_from_stmt(inner, names);
        }
        Statement::With(_, inner) => {
            collect_var_names_from_stmt(inner, names);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    fn parse(source: &str) -> crate::ast::Program {
        Parser::new(source)
            .expect("parser init")
            .parse_program()
            .expect("parse program")
    }

    fn var_names(source: &str) -> Vec<String> {
        collect_var_names(parse(source).body.as_slice())
    }

    #[test]
    fn collects_top_level_and_block_nested_var_names() {
        // A bare `var` and a `var` nested in a block both hoist to the body.
        assert_eq!(var_names("var x; { var y; }"), vec!["x", "y"]);
    }

    #[test]
    fn does_not_descend_into_nested_function_declarations() {
        // `var z` inside a nested function does NOT hoist to the enclosing body.
        // (Node: `eval("var a; function g(){ var b; } a")` defines `a`, not `b`.)
        assert_eq!(var_names("var a; function g(){ var b; }"), vec!["a"]);
    }

    fn unwrapped_name(source: &str) -> Option<String> {
        let program = parse(source);
        let first = &program.body.as_slice()[0];
        unwrap_labeled_function(first).map(|f| f.name.clone())
    }

    #[test]
    fn unwraps_plain_and_labeled_function_declarations() {
        assert_eq!(unwrapped_name("function f(){}"), Some("f".to_string()));
        assert_eq!(unwrapped_name("l: function g(){}"), Some("g".to_string()));
        assert_eq!(
            unwrapped_name("l1: l2: function h(){}"),
            Some("h".to_string())
        );
    }

    #[test]
    fn unwrap_rejects_non_function_statements() {
        assert_eq!(unwrapped_name("var x;"), None);
        assert_eq!(unwrapped_name("l: var y;"), None);
        assert_eq!(unwrapped_name("{ function f(){} }"), None);
    }

    fn func_decls(source: &str) -> Vec<FunctionDecl> {
        collect_function_decls(parse(source).body.as_slice())
    }

    #[test]
    fn dedups_function_declarations_keeping_last() {
        // Duplicate `f` collapses to one entry; the retained one is the LAST in
        // source order — here distinguished by arity (Node: the second `f`
        // wins, `eval("function f(){return 1} function f(){return 2} f()")` == 2).
        let decls = func_decls("function f(){} function f(a,b){} function g(){}");
        assert_eq!(decls.len(), 2);
        let f = decls.iter().find(|d| d.name == "f").expect("f present");
        assert_eq!(f.params.len(), 2, "kept the last declaration of `f`");
        assert!(decls.iter().any(|d| d.name == "g"));
    }

    #[test]
    fn function_decls_include_labeled_but_not_block_nested() {
        // Labeled top-level function declarations count; ones inside a block do not.
        let decls = func_decls("lbl: function a(){} { function b(){} }");
        let names: Vec<&str> = decls.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(names, vec!["a"]);
    }

    #[test]
    fn collects_var_names_from_control_flow_and_try() {
        let names = var_names(
            "if (t) { var a; } else { var b; }\
             for (var c = 0;;) {}\
             try { var d; } catch (e) { var f; } finally { var g; }\
             switch (s) { case 1: var h; }\
             lbl: { var i; }",
        );
        assert_eq!(names, vec!["a", "b", "c", "d", "f", "g", "h", "i"]);
    }
}
