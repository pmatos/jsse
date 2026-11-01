use super::*;

impl<'a> Parser<'a> {
    pub(super) fn expr_contains_yield_expression(expr: &Expression) -> bool {
        use crate::ast::{ArrowBody, Expression, MemberProperty, Property};
        match expr {
            Expression::Yield(_, _) => true,
            Expression::Array(elems, _) => elems
                .iter()
                .any(|e| e.as_ref().is_some_and(Self::expr_contains_yield_expression)),
            Expression::Object(props, _) => props.iter().any(|p: &Property| {
                Self::expr_contains_yield_expression(&p.value)
                    || matches!(&p.key, crate::ast::PropertyKey::Computed(e) if Self::expr_contains_yield_expression(e))
            }),
            Expression::Member(object, property, _) => {
                Self::expr_contains_yield_expression(object)
                    || matches!(property, MemberProperty::Computed(e) if Self::expr_contains_yield_expression(e))
            }
            Expression::Call(callee, args, _) | Expression::New(callee, args, _) => {
                Self::expr_contains_yield_expression(callee)
                    || args.iter().any(Self::expr_contains_yield_expression)
            }
            Expression::Binary(_, left, right, _)
            | Expression::Logical(_, left, right)
            | Expression::Assign(_, left, right, _) => {
                Self::expr_contains_yield_expression(left) || Self::expr_contains_yield_expression(right)
            }
            Expression::Unary(_, operand) | Expression::Update(.., operand, _) => {
                Self::expr_contains_yield_expression(operand)
            }
            Expression::Conditional(test, consequent, alternate) => {
                Self::expr_contains_yield_expression(test)
                    || Self::expr_contains_yield_expression(consequent)
                    || Self::expr_contains_yield_expression(alternate)
            }
            Expression::Sequence(exprs) | Expression::Comma(exprs) => {
                exprs.iter().any(Self::expr_contains_yield_expression)
            }
            Expression::Spread(inner, _) => Self::expr_contains_yield_expression(inner),
            Expression::Await(inner, _) => Self::expr_contains_yield_expression(inner),
            Expression::ArrowFunction(af) => {
                af.params.iter().any(Self::pattern_contains_yield_expression)
                    || match &af.body {
                                            ArrowBody::Expression(body) => {
                                                if let Statement::Return(Some(e), _) = &body.as_slice()[0] {
                                                    Self::expr_contains_yield_expression(e)
                                                } else {
                                                    false
                                                }
                                            }
                                            ArrowBody::Block(_) => false,
                                        }
            }
            Expression::Template(tl) => {
                tl.expressions.iter().any(Self::expr_contains_yield_expression)
            }
            Expression::TaggedTemplate(tag, tl) => {
                Self::expr_contains_yield_expression(tag)
                    || tl.expressions.iter().any(Self::expr_contains_yield_expression)
            }
            Expression::Typeof(e) | Expression::Void(e) | Expression::Delete(e) => {
                Self::expr_contains_yield_expression(e)
            }
            Expression::OptionalChain(object, chain) => {
                Self::expr_contains_yield_expression(object) || Self::expr_contains_yield_expression(chain)
            }
            _ => false,
        }
    }

    fn pattern_contains_yield_expression(pat: &Pattern) -> bool {
        match pat {
            Pattern::Identifier(_) => false,
            Pattern::Array(elems) => elems.iter().any(|e| {
                e.as_ref().is_some_and(|elem| match elem {
                    ArrayPatternElement::Pattern(p) | ArrayPatternElement::Rest(p) => {
                        Self::pattern_contains_yield_expression(p)
                    }
                })
            }),
            Pattern::Object(props) => props.iter().any(|prop| match prop {
                ObjectPatternProperty::KeyValue(_, p) | ObjectPatternProperty::Rest(p) => {
                    Self::pattern_contains_yield_expression(p)
                }
                ObjectPatternProperty::Shorthand(_) => false,
            }),
            Pattern::Assign(p, default) => {
                Self::pattern_contains_yield_expression(p)
                    || Self::expr_contains_yield_expression(default)
            }
            Pattern::Rest(p) => Self::pattern_contains_yield_expression(p),
            Pattern::MemberExpression(e) => Self::expr_contains_yield_expression(e),
        }
    }
}
