//! Conversions between the expression and pattern forms of a cover grammar
//! (ECMAScript 13.2.1.1, 13.15.5): a parenthesised list or a literal is
//! parsed as an expression and reinterpreted as parameters or an assignment
//! target once the following token shows which one it is.

use super::*;

/// AssignmentRestProperty : `...` DestructuringAssignmentTarget (13.15.5.1):
/// the target must not be an ArrayLiteral or an ObjectLiteral.
pub(crate) fn check_object_rest_target(target: &Expression) -> Result<(), ParseError> {
    if matches!(target, Expression::Array(..) | Expression::Object(..)) {
        return Err(ParseError {
            message: "`...` must be followed by an assignable reference in assignment contexts"
                .to_string(),
        });
    }
    Ok(())
}

pub(crate) fn expr_to_pattern(expr: Expression) -> Result<Pattern, ParseError> {
    match expr {
        Expression::Identifier(name) => Ok(Pattern::Identifier(name)),
        Expression::Assign(AssignOp::Assign, left, right, _) => {
            let pat = expr_to_pattern(left.into_expression())?;
            Ok(Pattern::Assign(Box::new(pat), right))
        }
        Expression::Array(elements, trailing_comma_after_spread) => {
            let mut pats = Vec::new();
            let mut saw_rest = false;
            for e in elements {
                if saw_rest {
                    return Err(ParseError {
                        message: "Rest element must be last element".to_string(),
                    });
                }
                let pat = e
                    .map(|e| {
                        if let Expression::Spread(inner) = e {
                            if let Expression::Assign(AssignOp::Assign, _, _, _) = *inner {
                                return Err(ParseError {
                                    message: "Rest element may not have a default initializer"
                                        .to_string(),
                                });
                            }
                            saw_rest = true;
                            expr_to_pattern(inner.into_expression()).map(ArrayPatternElement::Rest)
                        } else {
                            expr_to_pattern(e).map(ArrayPatternElement::Pattern)
                        }
                    })
                    .transpose()?;
                if saw_rest && pat.is_none() {
                    return Err(ParseError {
                        message: "Rest element must be last element".to_string(),
                    });
                }
                pats.push(pat);
            }
            if saw_rest && trailing_comma_after_spread {
                return Err(ParseError {
                    message: "Rest element must be last element".to_string(),
                });
            }
            Ok(Pattern::Array(pats))
        }
        Expression::Object(props, trailing_comma_after_spread) => {
            let mut pat_props = Vec::new();
            let mut saw_rest = false;
            for prop in props {
                if saw_rest {
                    return Err(ParseError {
                        message: "Rest element must be last element".to_string(),
                    });
                }
                if let PropertyKind::Init = prop.kind {
                    if let Expression::Spread(inner) = prop.value {
                        saw_rest = true;
                        check_object_rest_target(inner.as_ref())?;
                        let pat = expr_to_pattern(inner.into_expression())?;
                        pat_props.push(ObjectPatternProperty::Rest(pat));
                    } else if prop.shorthand {
                        if let PropertyKey::Identifier(ref name) = prop.key {
                            if let Expression::Assign(AssignOp::Assign, left, right, _) = prop.value
                            {
                                let pat = expr_to_pattern(left.into_expression())?;
                                pat_props.push(ObjectPatternProperty::KeyValue(
                                    prop.key,
                                    Pattern::Assign(Box::new(pat), right),
                                ));
                            } else {
                                pat_props.push(ObjectPatternProperty::Shorthand(name.clone()));
                            }
                        } else {
                            return Err(ParseError {
                                message: "Invalid destructuring target".to_string(),
                            });
                        }
                    } else {
                        let val_pat = expr_to_pattern(prop.value)?;
                        pat_props.push(ObjectPatternProperty::KeyValue(prop.key, val_pat));
                    }
                } else {
                    return Err(ParseError {
                        message: "Invalid destructuring target".to_string(),
                    });
                }
            }
            if saw_rest && trailing_comma_after_spread {
                return Err(ParseError {
                    message: "Rest element must be last element".to_string(),
                });
            }
            Ok(Pattern::Object(pat_props))
        }
        Expression::Spread(inner) => {
            let pat = expr_to_pattern(inner.into_expression())?;
            Ok(Pattern::Rest(Box::new(pat)))
        }
        Expression::Member(_, _, _) => Ok(Pattern::MemberExpression(ExprBox::new(expr))),
        _ => Err(ParseError {
            message: "Invalid destructuring target".to_string(),
        }),
    }
}

pub(crate) fn pattern_to_expr(pat: Pattern) -> Expression {
    match pat {
        Pattern::Identifier(name) => Expression::Identifier(name),
        Pattern::Rest(inner) => Expression::Spread(ExprBox::new(pattern_to_expr(*inner))),
        Pattern::Array(elements) => {
            let exprs: Vec<Option<Expression>> = elements
                .into_iter()
                .map(|elem| {
                    elem.map(|e| match e {
                        ArrayPatternElement::Pattern(p) => pattern_to_expr(p),
                        ArrayPatternElement::Rest(p) => {
                            Expression::Spread(ExprBox::new(pattern_to_expr(p)))
                        }
                    })
                })
                .collect();
            Expression::Array(exprs, false)
        }
        Pattern::Object(props) => {
            let expr_props: Vec<Property> = props
                .into_iter()
                .map(|prop| match prop {
                    ObjectPatternProperty::KeyValue(key, val) => Property {
                        key,
                        value: pattern_to_expr(val),
                        kind: PropertyKind::Init,
                        computed: false,
                        shorthand: false,
                        method: false,
                    },
                    ObjectPatternProperty::Shorthand(name) => Property {
                        key: PropertyKey::Identifier(name.clone()),
                        value: Expression::Identifier(name),
                        kind: PropertyKind::Init,
                        computed: false,
                        shorthand: true,
                        method: false,
                    },
                    ObjectPatternProperty::Rest(p) => Property {
                        key: PropertyKey::Identifier("__rest__".into()),
                        value: Expression::Spread(ExprBox::new(pattern_to_expr(p))),
                        kind: PropertyKind::Init,
                        computed: false,
                        shorthand: false,
                        method: false,
                    },
                })
                .collect();
            Expression::Object(expr_props, false)
        }
        Pattern::Assign(pat, default) => Expression::Assign(
            AssignOp::Assign,
            ExprBox::new(pattern_to_expr(*pat)),
            default,
            0,
        ),
        Pattern::MemberExpression(expr) => expr.into_expression(),
    }
}
