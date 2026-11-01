use super::*;

impl<'a> Parser<'a> {
    pub(super) fn is_simple_assignment_target(expr: &Expression) -> bool {
        matches!(
            expr,
            Expression::Identifier(_) | Expression::Member(_, _, _)
        )
    }

    pub(in crate::parser) fn validate_assignment_target(
        &self,
        expr: &Expression,
        simple_only: bool,
        allow_call: bool,
    ) -> Result<(), ParseError> {
        if !simple_only && matches!(expr, Expression::Array(..) | Expression::Object(..)) {
            if self.last_expr_parenthesized {
                return Err(self.error("Invalid left-hand side in assignment"));
            }
            self.validate_destructuring_pattern(expr)?;
            return Ok(());
        }
        if Self::is_simple_assignment_target(expr) {
            if self.strict
                && let Expression::Identifier(name) = expr
                && (name == "eval" || name == "arguments")
            {
                return Err(self.error("Assignment to 'eval' or 'arguments' in strict mode"));
            }
            return Ok(());
        }
        if allow_call && !self.strict && matches!(expr, Expression::Call(_, _, _)) {
            return Ok(());
        }
        Err(self.error("Invalid left-hand side in assignment"))
    }

    pub(super) fn validate_destructuring_pattern(
        &self,
        expr: &Expression,
    ) -> Result<(), ParseError> {
        match expr {
            Expression::Array(elements, trailing_comma_after_spread) => {
                let mut saw_rest = false;
                for elem in elements {
                    if let Some(e) = elem {
                        if saw_rest {
                            return Err(self.error(
                                "Rest element must be last element in array destructuring pattern",
                            ));
                        }
                        if let Expression::Spread(inner) = e {
                            saw_rest = true;
                            // Rest element cannot have initializer: [...x = 1]
                            if matches!(
                                inner.as_ref(),
                                Expression::Assign(AssignOp::Assign, _, _, _)
                            ) {
                                return Err(
                                    self.error("Rest element may not have a default initializer")
                                );
                            }
                            self.validate_destructuring_target(inner)?;
                        } else if let Expression::Assign(AssignOp::Assign, target, _, _) = e {
                            self.validate_destructuring_target(target)?;
                        } else {
                            self.validate_destructuring_target(e)?;
                        }
                    } else {
                        // Elision after rest is also invalid
                        if saw_rest {
                            return Err(self.error(
                                "Rest element must be last element in array destructuring pattern",
                            ));
                        }
                    }
                }
                if saw_rest && *trailing_comma_after_spread {
                    return Err(self.error(
                        "Rest element must be last element in array destructuring pattern",
                    ));
                }
                Ok(())
            }
            Expression::Object(props, trailing_comma_after_spread) => {
                let mut saw_rest = false;
                for prop in props {
                    if saw_rest {
                        return Err(self.error(
                            "Rest element must be last element in object destructuring pattern",
                        ));
                    }
                    if let Expression::Spread(target) = &prop.value {
                        saw_rest = true;
                        check_object_rest_target(target)?;
                    }
                    match prop.kind {
                        PropertyKind::Get | PropertyKind::Set => {
                            return Err(self.error("Invalid destructuring assignment target"));
                        }
                        PropertyKind::Init => {
                            if let Expression::Assign(AssignOp::Assign, target, _, _) = &prop.value
                            {
                                self.validate_destructuring_target(target)?;
                            } else if prop.shorthand {
                                // Shorthand { eval }/{ arguments } invalid in strict
                                if let Expression::Identifier(name) = &prop.value {
                                    if self.strict && (name == "eval" || name == "arguments") {
                                        return Err(self.error(
                                            "Assignment to 'eval' or 'arguments' in strict mode",
                                        ));
                                    }
                                    if self.in_generator && name == "yield" {
                                        return Err(self.error(
                                            "'yield' is not allowed as an identifier in this context",
                                        ));
                                    }
                                }
                            } else {
                                self.validate_destructuring_target(&prop.value)?;
                            }
                        }
                    }
                }
                if saw_rest && *trailing_comma_after_spread {
                    return Err(self.error(
                        "Rest element must be last element in object destructuring pattern",
                    ));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    pub(super) fn validate_destructuring_target(
        &self,
        expr: &Expression,
    ) -> Result<(), ParseError> {
        match expr {
            Expression::Array(..) | Expression::Object(..) => {
                self.validate_destructuring_pattern(expr)
            }
            Expression::Spread(inner) => self.validate_destructuring_target(inner),
            // Single-element Sequence is a parenthesization marker
            Expression::Sequence(exprs) if exprs.len() == 1 => {
                match &exprs[0] {
                    // Parenthesized Object/Array patterns are not destructuring targets
                    Expression::Object(..) | Expression::Array(..) => {
                        Err(self.error("Invalid destructuring assignment target"))
                    }
                    other => self.validate_destructuring_target(other),
                }
            }
            Expression::Sequence(_) | Expression::Comma(_) => {
                Err(self.error("Invalid destructuring assignment target"))
            }
            _ => {
                // §13.15.5.1: DestructuringAssignmentTarget must have
                // AssignmentTargetType == simple (identifier or member expression)
                if Self::is_simple_assignment_target(expr) {
                    if self.strict
                        && let Expression::Identifier(name) = expr
                        && (name == "eval" || name == "arguments")
                    {
                        return Err(self.nested_eval_error());
                    }
                    Ok(())
                } else {
                    Err(self.error("Invalid destructuring assignment target"))
                }
            }
        }
    }
}
