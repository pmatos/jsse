//! Expression half of the printer (see `js_print`).

use crate::js_ast::*;
use crate::js_pools::*;
use crate::js_print::{FnKind, Printer, is_ident_name, pick};

fn un_op(op: UnOp) -> &'static str {
    match op {
        UnOp::Neg => "-",
        UnOp::Pos => "+",
        UnOp::Not => "!",
        UnOp::BitNot => "~",
        UnOp::Typeof => "typeof",
        UnOp::Void => "void",
    }
}

fn bin_op(op: BinOp) -> &'static str {
    use BinOp::*;
    match op {
        Add => "+",
        Sub => "-",
        Mul => "*",
        Div => "/",
        Mod => "%",
        Exp => "**",
        Eq => "==",
        Ne => "!=",
        StrictEq => "===",
        StrictNe => "!==",
        Lt => "<",
        Le => "<=",
        Gt => ">",
        Ge => ">=",
        BitAnd => "&",
        BitOr => "|",
        BitXor => "^",
        Shl => "<<",
        Shr => ">>",
        UShr => ">>>",
        In => "in",
        InstanceOf => "instanceof",
        And => "&&",
        Or => "||",
        Nullish => "??",
    }
}

fn assign_op(op: AssignOp) -> &'static str {
    use AssignOp::*;
    match op {
        Assign => "=",
        Add => "+=",
        Sub => "-=",
        Mul => "*=",
        Div => "/=",
        Mod => "%=",
        Exp => "**=",
        Shl => "<<=",
        Shr => ">>=",
        UShr => ">>>=",
        BitAnd => "&=",
        BitOr => "|=",
        BitXor => "^=",
        And => "&&=",
        Or => "||=",
        Nullish => "??=",
    }
}

impl Printer {
    pub fn paren(&mut self, e: &Expr) {
        self.w("(");
        self.expr(e);
        self.w(")");
    }

    fn prop_access(&mut self, name: &str, opt: bool) {
        if is_ident_name(name) {
            self.w(if opt { "?." } else { "." });
            self.w(name);
        } else {
            self.w(if opt { "?.[" } else { "[" });
            self.quoted(name);
            self.w("]");
        }
    }

    pub fn target(&mut self, t: &Target) {
        match t {
            Target::Id(i) => {
                let n = self.name(*i);
                self.w(&n);
            }
            Target::Member(o, p) => {
                self.paren(o);
                self.prop_access(pick(PROPS, *p), false);
            }
            Target::Index(o, k) => {
                self.paren(o);
                self.w("[");
                self.paren(k);
                self.w("]");
            }
        }
    }

    fn elems(&mut self, es: &[Elem], holes: bool) {
        for (i, e) in es.iter().enumerate() {
            self.w(if i > 0 { ", " } else { "" });
            match e {
                Elem::Hole if holes => {}
                Elem::Hole => self.w("undefined"),
                Elem::Val(e) => self.paren(e),
                Elem::Spread(e) => {
                    self.w("...");
                    self.paren(e);
                }
            }
        }
        if holes && matches!(es.last(), Some(Elem::Hole)) {
            self.w(",");
        }
    }

    fn args(&mut self, es: &[Elem]) {
        self.w("(");
        self.elems(es, false);
        self.w(")");
    }

    fn template(&mut self, parts: &[(Ix, Expr)], last: Ix) {
        self.w("`");
        for (chunk, e) in parts {
            self.w(pick(TPL, *chunk));
            self.w("${");
            self.expr(e);
            self.w("}");
        }
        self.w(pick(TPL, last));
        self.w("`");
    }

    fn object(&mut self, props: &[ObjProp]) {
        let mut proto_used = false;
        self.w("{");
        for (i, p) in props.iter().enumerate() {
            self.w(if i > 0 { ", " } else { "" });
            match p {
                ObjProp::Shorthand(i) => {
                    let n = self.name(*i);
                    self.w(&n);
                }
                ObjProp::Kv(k, v) => {
                    let name = pick(PROPS, *k);
                    if name == "__proto__" && !std::mem::replace(&mut proto_used, true) {
                        self.w("\"__proto__\"");
                    } else if name == "__proto__" {
                        self.w("[\"__proto__\"]");
                    } else {
                        self.quoted(name);
                    }
                    self.w(": ");
                    self.paren(v);
                }
                ObjProp::Computed(k, v) => {
                    self.w("[");
                    self.paren(k);
                    self.w("]: ");
                    self.paren(v);
                }
                ObjProp::Method(k, f) => {
                    self.w(if f.is_async { "async " } else { "" });
                    self.w(if f.is_gen { "*" } else { "" });
                    self.quoted(pick(PROPS, *k));
                    self.func_tail(FnKind::Method, f);
                }
                ObjProp::Get(k, b) | ObjProp::Set(k, b) => {
                    let setter = matches!(p, ObjProp::Set(..));
                    self.w(if setter { "set " } else { "get " });
                    self.quoted(pick(PROPS, *k));
                    self.accessor_tail(setter, b);
                }
                ObjProp::Spread(e) => {
                    self.w("...");
                    self.paren(e);
                }
            }
        }
        self.w("}");
    }

    pub fn expr(&mut self, e: &Expr) {
        if self.over() {
            self.w("0");
            return;
        }
        self.depth += 1;
        self.expr_inner(e);
        self.depth -= 1;
    }

    fn expr_inner(&mut self, e: &Expr) {
        match e {
            Expr::Int(n) if *n < 0 => self.w(&format!("({n})")),
            Expr::Int(n) => self.w(&n.to_string()),
            Expr::Num(i) => self.w(pick(NUMS, *i)),
            Expr::Str(i) => self.w(pick(STRS, *i)),
            Expr::Bool(b) => self.w(&b.to_string()),
            Expr::Null => self.w("null"),
            Expr::Undef => self.w("undefined"),
            Expr::BigInt(n) if *n < 0 => self.w(&format!("({n}n)")),
            Expr::BigInt(n) => self.w(&format!("{n}n")),
            Expr::This => self.w("this"),
            Expr::Ident(i) => {
                let n = self.name(*i);
                self.w(&n);
            }
            Expr::Global(i) => self.w(pick(GLOBALS, *i)),
            Expr::Arguments => self.w(if self.flow.args { "arguments" } else { "0" }),
            Expr::NewTarget => self.w(if self.flow.new_target {
                "new.target"
            } else {
                "undefined"
            }),
            Expr::WellKnown(i) => self.w(&format!("Symbol.{}", pick(SYMBOLS, *i))),
            Expr::Regex(p, f) => {
                self.w(&format!("/{}/{}", pick(REGEX, *p), pick(FLAGS, *f)))
            }
            Expr::Eval(i) => self.w(&format!("eval(\"{}\")", pick(EVALS, *i))),
            Expr::SuperMember(p) if self.flow.super_prop => {
                self.w("super");
                self.prop_access(pick(PROPS, *p), false);
            }
            Expr::SuperMember(_) => self.w("undefined"),
            Expr::Template(parts, last) => self.template(parts, *last),
            Expr::Tagged(tag, parts, last) => {
                self.paren(tag);
                self.template(parts, *last);
            }
            Expr::Unary(op, x) => {
                self.w(un_op(*op));
                self.w(" ");
                self.paren(x);
            }
            Expr::Binary(op, l, r) => {
                self.paren(l);
                self.w(&format!(" {} ", bin_op(*op)));
                self.paren(r);
            }
            Expr::Update(prefix, inc, t) => {
                let op = if *inc { "++" } else { "--" };
                if *prefix {
                    self.w(op);
                    self.target(t);
                } else {
                    self.target(t);
                    self.w(&format!(" {op}"));
                }
            }
            Expr::Assign(t, op, v) => {
                self.target(t);
                self.w(&format!(" {} ", assign_op(*op)));
                self.paren(v);
            }
            Expr::DestructAssign(p, v) => {
                self.pat(p, false);
                self.w(" = ");
                self.paren(v);
            }
            Expr::Cond(c, a, b) => {
                self.paren(c);
                self.w(" ? ");
                self.paren(a);
                self.w(" : ");
                self.paren(b);
            }
            Expr::Seq(a, b) => {
                self.paren(a);
                self.w(", ");
                self.paren(b);
            }
            Expr::Call(f, args, opt) => {
                self.paren(f);
                self.w(if *opt { "?." } else { "" });
                self.args(args);
            }
            Expr::MethodCall(o, m, args, opt) => {
                self.paren(o);
                self.prop_access(pick(METHODS, *m), *opt);
                self.args(args);
            }
            Expr::StaticCall(g, m, args) => {
                let (global, methods) = STATICS[*g as usize % STATICS.len()];
                self.w(&format!("{global}.{}", pick(methods, *m)));
                self.args(args);
            }
            Expr::New(f, args) => {
                self.w("new ");
                self.paren(f);
                self.args(args);
            }
            Expr::SuperCall(args) if self.flow.super_call => {
                self.w("super");
                self.args(args);
            }
            Expr::SuperCall(_) => self.w("0"),
            Expr::Member(o, p, opt) => {
                self.paren(o);
                self.prop_access(pick(PROPS, *p), *opt);
            }
            Expr::Index(o, k, opt) => {
                self.paren(o);
                self.w(if *opt { "?.[" } else { "[" });
                self.paren(k);
                self.w("]");
            }
            Expr::PrivateGet(o, p) => {
                let names = self.private_names();
                self.paren(o);
                if !names.is_empty() {
                    self.w(&format!(".{}", names[*p as usize % names.len()]));
                }
            }
            Expr::PrivateIn(p, o) => {
                let names = self.private_names();
                if names.is_empty() {
                    return self.paren(o);
                }
                self.w(&format!("{} in ", names[*p as usize % names.len()]));
                self.paren(o);
            }
            Expr::Delete(t) if !matches!(**t, Target::Id(_)) => {
                self.w("delete ");
                self.target(t);
            }
            Expr::Delete(_) => self.w("true"),
            Expr::Array(es) => {
                self.w("[");
                self.elems(es, true);
                self.w("]");
            }
            Expr::Object(props) => self.object(props),
            Expr::Arrow(a) => self.arrow(a),
            Expr::Func(f) => {
                self.func_header(f, "");
                self.func_tail(FnKind::Plain, f);
            }
            Expr::Class(c) => self.class(c, ""),
            Expr::Await(x) if self.flow.in_async => {
                self.w("await ");
                self.paren(x);
            }
            Expr::Await(x) => self.paren(x),
            Expr::Yield(delegate, arg) if self.flow.in_gen => {
                self.w(if *delegate { "yield* " } else { "yield" });
                match arg {
                    Some(x) => {
                        self.w(if *delegate { "" } else { " " });
                        self.paren(x);
                    }
                    None if *delegate => self.w("[]"),
                    None => {}
                }
            }
            Expr::Yield(_, arg) => match arg {
                Some(x) => self.paren(x),
                None => self.w("undefined"),
            },
        }
    }
}
