use super::*;

impl<'a> Parser<'a> {
    pub(super) fn check_strict_assignment_pattern(&self, pat: &Pattern) -> Result<(), ParseError> {
        if !self.strict {
            return Ok(());
        }
        match pat {
            Pattern::Identifier(name) => {
                if name == "eval" || name == "arguments" {
                    return Err(self.error(format!("Assignment to '{name}' in strict mode")));
                }
            }
            Pattern::Array(elems) => {
                for elem in elems.iter().flatten() {
                    match elem {
                        ArrayPatternElement::Pattern(p) | ArrayPatternElement::Rest(p) => {
                            self.check_strict_element(p)?;
                        }
                    }
                }
            }
            Pattern::Object(props) => {
                for prop in props {
                    match prop {
                        ObjectPatternProperty::KeyValue(_, p) | ObjectPatternProperty::Rest(p) => {
                            self.check_strict_element(p)?;
                        }
                        ObjectPatternProperty::Shorthand(name) => {
                            if name == "eval" || name == "arguments" {
                                return Err(
                                    self.error(format!("Assignment to '{name}' in strict mode"))
                                );
                            }
                        }
                    }
                }
            }
            Pattern::Assign(p, _) => {
                self.check_strict_assignment_pattern(p)?;
            }
            Pattern::Rest(p) => {
                self.check_strict_assignment_pattern(p)?;
            }
            Pattern::MemberExpression(_) => {}
        }
        Ok(())
    }

    /// An element of a pattern. V8 words `eval` and `arguments` there as a
    /// target that cannot be assigned to, not as an assignment to them.
    fn check_strict_element(&self, pat: &Pattern) -> Result<(), ParseError> {
        if self.strict
            && inferred_names::enabled()
            && matches!(pat, Pattern::Identifier(name) if name == "eval" || name == "arguments")
        {
            return Err(self.error("Invalid destructuring assignment target"));
        }
        self.check_strict_assignment_pattern(pat)
    }
}
