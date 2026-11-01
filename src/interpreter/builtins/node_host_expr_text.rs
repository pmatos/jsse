//! V8's `CallPrinter` for the expressions in destructuring and iteration
//! messages (#1512): `x is not iterable`, `Cannot destructure property 'a' of
//! 'o.p' as it is undefined.` The text is printed from the AST only when an
//! error is about to be thrown under `--node`.

use crate::ast::{BinaryOp, Expression, Literal, LogicalOp, MemberProperty, UnaryOp, UpdateOp};
use crate::types::JsValue;

const UNKNOWN: &str = "(intermediate value)";

fn binary_symbol(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Mod => "%",
        BinaryOp::Exp => "**",
        BinaryOp::Eq => "==",
        BinaryOp::NotEq => "!=",
        BinaryOp::StrictEq => "===",
        BinaryOp::StrictNotEq => "!==",
        BinaryOp::Lt => "<",
        BinaryOp::Gt => ">",
        BinaryOp::LtEq => "<=",
        BinaryOp::GtEq => ">=",
        BinaryOp::LShift => "<<",
        BinaryOp::RShift => ">>",
        BinaryOp::URShift => ">>>",
        BinaryOp::BitAnd => "&",
        BinaryOp::BitOr => "|",
        BinaryOp::BitXor => "^",
        BinaryOp::In => "in",
        BinaryOp::Instanceof => "instanceof",
    }
}

fn literal_text(literal: &Literal) -> Option<String> {
    Some(match literal {
        Literal::Null => "null".to_string(),
        Literal::Boolean(b) => b.to_string(),
        Literal::Number(n) => JsValue::number(*n).to_string(),
        Literal::String(units) => format!("\"{}\"", String::from_utf16_lossy(units)),
        Literal::RegExp(pattern, flags) => {
            format!("/{}/{flags}", String::from_utf16_lossy(pattern))
        }
        Literal::BigInt(_) => return None,
    })
}

/// V8 parses a left-nested run of one of these operators as a single n-ary
/// operation, which it prints flat: `(a + b + c)`.
fn chains(op: BinaryOp) -> bool {
    use BinaryOp::*;
    matches!(
        op,
        Add | Sub | Mul | Div | Mod | BitOr | BitAnd | BitXor | LShift | RShift | URShift
    )
}

/// A literal V8's parser folds an operation on literals into.
enum Folded {
    Number(f64),
    Bool(bool),
}

/// V8 folds arithmetic and bitwise operators on number literals, unary
/// `-`, `+` and `~` of one, and `!` of any literal while it parses.
fn fold(expr: &Expression) -> Option<Folded> {
    use crate::types::number_ops as ops;
    let number = fold_number;
    Some(Folded::Number(match expr {
        Expression::Literal(Literal::Number(n)) => *n,
        Expression::Literal(Literal::Boolean(b)) => return Some(Folded::Bool(*b)),
        Expression::Literal(Literal::Null) => return Some(Folded::Bool(false)),
        Expression::Literal(Literal::String(units)) => {
            return Some(Folded::Bool(!units.is_empty()));
        }
        Expression::Unary(UnaryOp::Not, operand) => {
            return Some(Folded::Bool(match fold(operand)? {
                Folded::Number(n) => n == 0.0 || n.is_nan(),
                Folded::Bool(b) => !b,
            }));
        }
        Expression::Unary(UnaryOp::Minus, operand) => -number(operand)?,
        Expression::Unary(UnaryOp::Plus, operand) => number(operand)?,
        Expression::Unary(UnaryOp::BitNot, operand) => ops::bitwise_not(number(operand)?),
        Expression::Binary(op, left, right, _) => apply(*op, number(left)?, number(right)?)?,
        _ => return None,
    }))
}

fn fold_number(expr: &Expression) -> Option<f64> {
    match fold(expr)? {
        Folded::Number(n) => Some(n),
        Folded::Bool(_) => None,
    }
}

/// `x op y` for the operators V8 folds.
fn apply(op: BinaryOp, x: f64, y: f64) -> Option<f64> {
    use crate::types::number_ops as ops;
    let (i, j) = (ops::to_int32(x), ops::to_int32(y));
    let shift = ops::to_uint32(y) & 31;
    Some(match op {
        BinaryOp::Add => x + y,
        BinaryOp::Sub => x - y,
        BinaryOp::Mul => x * y,
        BinaryOp::Div => x / y,
        BinaryOp::Mod => x % y,
        BinaryOp::Exp => ops::exponentiate(x, y),
        BinaryOp::BitOr => f64::from(i | j),
        BinaryOp::BitAnd => f64::from(i & j),
        BinaryOp::BitXor => f64::from(i ^ j),
        BinaryOp::LShift => f64::from(i << shift),
        BinaryOp::RShift => f64::from(i >> shift),
        BinaryOp::URShift => f64::from(ops::to_uint32(x) >> shift),
        _ => return None,
    })
}

/// The text V8 prints for `expr`, `(intermediate value)` for what it does
/// not print.
pub(crate) fn expr_text(expr: &Expression) -> String {
    if matches!(expr, Expression::Unary(..) | Expression::Binary(..))
        && let Some(folded) = fold(expr)
    {
        return match folded {
            Folded::Number(n) => JsValue::number(n).to_string(),
            Folded::Bool(b) => b.to_string(),
        };
    }
    match expr {
        Expression::Literal(literal) => literal_text(literal).unwrap_or_else(|| UNKNOWN.into()),
        Expression::Identifier(name) => name.clone(),
        Expression::This => "this".into(),
        Expression::Member(object, property, _) => {
            let object = expr_text(object);
            match property {
                MemberProperty::Dot(name) => format!("{object}.{name}"),
                MemberProperty::Private(name) => format!("{object}.#{name}"),
                MemberProperty::Computed(key) => match &**key {
                    Expression::Literal(Literal::String(units)) => {
                        format!("{object}.{}", String::from_utf16_lossy(units))
                    }
                    key => format!("{object}[{}]", expr_text(key)),
                },
            }
        }
        Expression::Call(callee, _, _) => format!("{}(...)", expr_text(callee)),
        Expression::Unary(op, operand) => {
            let symbol = match op {
                UnaryOp::Minus => "-",
                UnaryOp::Plus => "+",
                UnaryOp::Not => "!",
                UnaryOp::BitNot => "~",
            };
            match (&**operand, op) {
                (Expression::Literal(Literal::Number(_)), UnaryOp::Minus) => {
                    format!("-{}", expr_text(operand))
                }
                _ => format!("({symbol}{})", expr_text(operand)),
            }
        }
        Expression::Typeof(operand) => format!("(typeof {})", expr_text(operand)),
        Expression::Void(operand) => format!("(void {})", expr_text(operand)),
        Expression::Delete(operand) => format!("(delete {})", expr_text(operand)),
        Expression::Binary(op, left, right, _) => {
            let mut rest = vec![&**right];
            let mut head: &Expression = left;
            while chains(*op)
                && let Expression::Binary(inner, l, r, _) = head
                && inner == op
            {
                rest.push(r);
                head = l;
            }
            rest.reverse();
            let mut operands = Vec::new();
            let mut taken = 0;
            if let Some(mut acc) = fold_number(head) {
                while let Some(next) = rest.get(taken).and_then(|r| fold_number(r))
                    && let Some(folded) = apply(*op, acc, next)
                {
                    acc = folded;
                    taken += 1;
                }
                operands.push(JsValue::number(acc).to_string());
            } else {
                operands.push(expr_text(head));
            }
            operands.extend(rest[taken..].iter().map(|operand| expr_text(operand)));
            format!("({})", operands.join(&format!(" {} ", binary_symbol(*op))))
        }
        Expression::Logical(op, left, right) => {
            let symbol = match op {
                LogicalOp::And => "&&",
                LogicalOp::Or => "||",
                LogicalOp::NullishCoalescing => "??",
            };
            format!("({} {symbol} {})", expr_text(left), expr_text(right))
        }
        Expression::Sequence(items) if items.len() == 1 => expr_text(&items[0]),
        Expression::Sequence(items) | Expression::Comma(items) => {
            let items: Vec<String> = items.iter().map(expr_text).collect();
            format!("({})", items.join(" , "))
        }
        Expression::Array(items, _) => {
            let items: Vec<String> = items
                .iter()
                .map(|item| item.as_ref().map(expr_text).unwrap_or_default())
                .collect();
            format!("[{}]", items.join(","))
        }
        Expression::Object(props, _) => format!("{{{}}}", UNKNOWN.repeat(props.len())),
        Expression::Conditional(..) => UNKNOWN.repeat(3),
        Expression::Assign(_, target, _, _) => expr_text(target),
        Expression::Spread(operand) => format!("(...{})", expr_text(operand)),
        Expression::Update(op, prefix, operand, _) => {
            let symbol = if *op == UpdateOp::Increment {
                "++"
            } else {
                "--"
            };
            let operand = expr_text(operand);
            if *prefix {
                format!("({symbol}{operand})")
            } else {
                format!("({operand}{symbol})")
            }
        }
        Expression::Template(template) if !template.expressions.is_empty() => {
            template.expressions.iter().map(expr_text).collect()
        }
        Expression::Template(template) if template.expressions.is_empty() => {
            let cooked = template
                .quasis
                .first()
                .cloned()
                .flatten()
                .unwrap_or_default();
            format!("\"{}\"", String::from_utf16_lossy(&cooked))
        }
        _ => UNKNOWN.into(),
    }
}
