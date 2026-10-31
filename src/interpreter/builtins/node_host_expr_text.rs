//! V8's `CallPrinter` for the expressions in destructuring and iteration
//! messages (#1512): `x is not iterable`, `Cannot destructure property 'a' of
//! 'o.p' as it is undefined.` The text is printed from the AST only when an
//! error is about to be thrown under `--node`.

use crate::ast::{BinaryOp, Expression, Literal, LogicalOp, MemberProperty, UnaryOp};
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

/// The text V8 prints for `expr`, `(intermediate value)` for what it does
/// not print.
pub(crate) fn expr_text(expr: &Expression) -> String {
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
        Expression::Binary(op, left, right) => format!(
            "({} {} {})",
            expr_text(left),
            binary_symbol(*op),
            expr_text(right)
        ),
        Expression::Logical(op, left, right) => {
            let symbol = match op {
                LogicalOp::And => "&&",
                LogicalOp::Or => "||",
                LogicalOp::NullishCoalescing => "??",
            };
            format!("({} {symbol} {})", expr_text(left), expr_text(right))
        }
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
        Expression::Object(props, _) if props.is_empty() => "{}".into(),
        Expression::Conditional(..) => UNKNOWN.repeat(3),
        _ => UNKNOWN.into(),
    }
}
