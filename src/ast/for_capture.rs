//! Whether a C-style `for` loop can observe its per-iteration environments.

use super::*;

impl ForStatement {
    /// The verdict behind [`ForStatement::may_capture_iteration_scope`].
    ///
    /// A `let`/`const` head is observed only through its own names, so a loop
    /// whose functions and classes mention none of them (shadowing ignored)
    /// and that has no direct `eval` or `with` cannot tell the copies apart.
    pub(super) fn capture_verdict(&self) -> bool {
        let mut head_names = Vec::new();
        if let Some(ForInit::Variable(decl)) = &self.init
            && decl.kind != VarKind::Var
        {
            for declarator in &decl.declarations {
                declarator.pattern.bound_names(&mut head_names);
            }
        }
        if head_names.is_empty() {
            let mut walk = CaptureWalk::default();
            walk.push_for_init(self.init.as_ref());
            walk.push_opt_expr(self.test.as_ref());
            walk.push_opt_expr(self.update.as_deref());
            walk.push_stmt(&self.body);
            return walk.run();
        }
        let nested = CaptureWalk::nested_mentions_of(|walk| {
            walk.push_for_init(self.init.as_ref());
            walk.push_opt_expr(self.test.as_ref());
            walk.push_opt_expr(self.update.as_deref());
            walk.push_stmt(&self.body);
        });
        nested.opaque
            || nested
                .names
                .iter()
                .any(|n| head_names.iter().any(|h| h == n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::Parser;

    fn loop_may_capture(source: &str) -> bool {
        let program = Parser::new(source)
            .expect("parser init")
            .parse_program()
            .expect("parse program");
        match &program.body.as_slice()[0] {
            Statement::For(f) => f.may_capture_iteration_scope(),
            other => panic!("not a for loop: {other:?}"),
        }
    }

    #[test]
    fn closures_that_ignore_the_head_bindings_cannot_capture_them() {
        for source in [
            "for (let i = 0; i < 3; i++) { const x = i; g = () => x; }",
            "for (let i = 0; i < 3; i++) { const x = i; g = function () { return x + 1; }; }",
            "for (let i = 0, j = 1; i < j; i++) { g = () => k; }",
            "for (const i = 0; ;) { g = class { m() { return 1; } }; break; }",
        ] {
            assert!(!loop_may_capture(source), "{source}");
        }
    }

    #[test]
    fn closures_reaching_a_head_binding_by_any_route_may_capture() {
        for source in [
            "for (let i = 0; i < 3; i++) { g = () => () => i; }",
            "for (let i = 0; i < 3; i++) { g = () => eval('i'); }",
            "for (let i = 0; i < 3; i++) { (0, eval)('1'); }",
            "for (let i = 0; i < 3; i++) { g = class { f = i; }; }",
            "for (let i = 0; i < 3; i++) { g = class { static { i; } }; }",
            "for (let i = 0; i < 3; i++) { g = (i) => i; }",
            "for (let i = 0, j = 1; i < j; i++) { g = () => j; }",
            "for (let [a, b] = [0, 1]; a < b; a++) { g = () => b; }",
            "for (let i = 0; i < 3; i++) { g = () => { with (o) { x; } }; }",
        ] {
            assert!(loop_may_capture(source), "{source}");
        }
    }
}
