//! Renders decoded [`Op`]s as a deterministic, single-threaded JS program.
//!
//! Each op runs inside `log(n, ...)`, which catches the exception and prints
//! the op index with either a result summary or the error name. The epilogue
//! dumps every buffer and view slot, so a length-tracking or detach state
//! that differs between execution engines shows up in the compared output.
//! The program never mentions `agent`, `waitAsync` or `sleep`, and
//! `Atomics.wait` always carries the literal timeout `0`.

use super::ops::{ATOMICS, Action, Arg, Hostile, METHODS, Op, SLOTS, TransferMode};

const PREAMBLE: &str = "\
var B=[],V=[];
function big(t,v){return typeof v==='number'&&Object.prototype.toString.call(t).indexOf('Big')>=0?BigInt(Number.isFinite(v)?Math.trunc(v):0):v}
function rz(b,n){var x=B[b];return x.resize?x.resize(n):x.grow(n)}
function dt(b){$262.detachArrayBuffer(B[b])}
function tf(b,m,n){B[b]=m==0?B[b].transfer():m==1?B[b].transfer(n):B[b].transferToFixedLength()}
function h(k,b,n){try{k==0?rz(b,n):k==1?dt(b):tf(b,1,n)}catch(e){}}
function summ(r){if(r!==null&&typeof r==='object'&&ArrayBuffer.isView(r)&&!(r instanceof DataView))r=Array.from(r);return typeof r+':'+String(r)}
function log(i,f){try{print(i,summ(f()))}catch(e){print(i,'E',e&&e.name)}}
function t(f){try{return f()}catch(e){return e.name}}
";

const EPILOGUE: &str = "\
for(var i=0;i<4;i++){var b=B[i],v=V[i];
print('B'+i,b?[t(()=>b.byteLength),t(()=>b.maxByteLength),!!(b.resizable||b.growable),b.detached,t(()=>b.detached?-1:new Uint8Array(b).reduce((s,x)=>(s*31+x)|0,0))].join():'-');
print('V'+i,v?[t(()=>v.length),t(()=>v.byteLength),t(()=>v.byteOffset)].join():'-')}
";

#[derive(Clone, Copy)]
enum ArgForm {
    Verbatim,
    Big,
    Comparator,
    Mapper,
}

fn hostile_call(h: &Hostile) -> String {
    let k = match h.action {
        Action::Resize => 0,
        Action::Detach => 1,
        Action::Transfer => 2,
    };
    format!("h({k},{},{})", h.buf, h.len)
}

fn render_arg(arg: &Arg, view: u8, form: ArgForm) -> String {
    let lit = arg.value;
    let big = format!("big(V[{view}],{lit})");
    let cmp = "x<y?-1:x>y?1:0";
    match (arg.hostile.as_ref(), form) {
        (None, ArgForm::Verbatim) => lit.to_string(),
        (None, ArgForm::Big) => big,
        (None, ArgForm::Comparator) => format!("(x,y)=>{cmp}"),
        (None, ArgForm::Mapper) => "(x)=>x".to_string(),
        (Some(h), ArgForm::Verbatim) => {
            format!("{{valueOf(){{{};return {lit}}}}}", hostile_call(h))
        }
        (Some(h), ArgForm::Big) => format!("{{valueOf(){{{};return {big}}}}}", hostile_call(h)),
        (Some(h), ArgForm::Comparator) => format!("(x,y)=>{{{};return {cmp}}}", hostile_call(h)),
        (Some(h), ArgForm::Mapper) => format!("(x)=>{{{};return x}}", hostile_call(h)),
    }
}

fn expand(template: &str, view: u8, idx: Option<i32>, args: &[Arg]) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let close = rest[open..].find('}').expect("unterminated placeholder") + open;
        let token = &rest[open + 1..close];
        match token {
            "v" => out.push_str(&format!("V[{view}]")),
            "w" => out.push_str(&format!("V[{}]", (view + 1) % SLOTS)),
            "i" => out.push_str(&idx.expect("template needs an index").to_string()),
            _ => {
                let n = token[1..].parse::<usize>().expect("argument number");
                let form = match &token[..1] {
                    "a" => ArgForm::Verbatim,
                    "b" => ArgForm::Big,
                    "c" => ArgForm::Comparator,
                    "m" => ArgForm::Mapper,
                    other => panic!("unknown placeholder {other}"),
                };
                out.push_str(&render_arg(&args[n], view, form));
            }
        }
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    out
}

fn template<'a>(table: &'a [(&str, &str)], name: &str) -> &'a str {
    table
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| *t)
        .expect("name not in table")
}

fn buffer_ctor(shared: bool) -> &'static str {
    if shared {
        "SharedArrayBuffer"
    } else {
        "ArrayBuffer"
    }
}

fn render_op(op: &Op) -> String {
    match op {
        Op::NewBuffer {
            slot,
            len,
            max,
            shared,
        } => {
            let ctor = buffer_ctor(*shared);
            let opts = max.map_or(String::new(), |m| format!(",{{maxByteLength:{m}}}"));
            format!("return B[{slot}]=new {ctor}({len}{opts})")
        }
        Op::NewView {
            slot,
            kind,
            buf,
            offset,
            len,
        } => {
            let len = len.map_or(String::new(), |n| format!(",{n}"));
            format!(
                "return V[{slot}]=new {}(B[{buf}],{offset}{len})",
                kind.name()
            )
        }
        Op::Resize { buf, len } => format!("return rz({buf},{len})"),
        Op::Transfer { buf, mode } => {
            let call = match mode {
                TransferMode::Same => "transfer()".to_string(),
                TransferMode::Fixed(n) => format!("transfer({n})"),
                TransferMode::ToFixed => "transferToFixedLength()".to_string(),
            };
            format!("return B[{buf}]=B[{buf}].{call}")
        }
        Op::Detach { buf } => format!("return dt({buf})"),
        Op::Access { view, idx, reps } => {
            format!("var r;for(var k=0;k<{reps};k++)r=V[{view}][{idx}];return r")
        }
        Op::Store {
            view,
            idx,
            val,
            reps,
        } => {
            let v = render_arg(val, *view, ArgForm::Big);
            format!("for(var k=0;k<{reps};k++)V[{view}][{idx}]={v};return V[{view}][{idx}]")
        }
        Op::Method { view, name, args } => {
            format!(
                "return {}",
                expand(template(METHODS, name), *view, None, args)
            )
        }
        Op::Atomic {
            view,
            op,
            idx,
            args,
        } => format!(
            "return {}",
            expand(template(ATOMICS, op), *view, Some(*idx), args)
        ),
    }
}

pub fn render(ops: &[Op]) -> String {
    let mut src = String::from(PREAMBLE);
    for (n, op) in ops.iter().enumerate() {
        src.push_str(&format!("log({n},()=>{{{}}});\n", render_op(op)));
    }
    src.push_str(EPILOGUE);
    src
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typed_array::ops::{Arg, Kind, METHODS};
    use jsse::SigOutcome;

    fn run(src: &str, bytecode: bool) -> jsse::RunSignature {
        jsse::run_on_engine_stack(|| jsse::run_script_observed(src, 200_000, bytecode))
    }

    fn contains_ident(src: &str, ident: &str) -> bool {
        let is_ident_char = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
        src.match_indices(ident).any(|(at, _)| {
            let before = src[..at].chars().next_back();
            let after = src[at + ident.len()..].chars().next();
            !before.is_some_and(is_ident_char) && !after.is_some_and(is_ident_char)
        })
    }

    fn setup() -> Vec<Op> {
        vec![
            Op::NewBuffer {
                slot: 0,
                len: 16,
                max: Some(24),
                shared: false,
            },
            Op::NewView {
                slot: 0,
                kind: Kind::Int8,
                buf: 0,
                offset: 0,
                len: None,
            },
        ]
    }

    #[test]
    fn empty_program_runs() {
        let src = render(&[]);
        for bytecode in [true, false] {
            let sig = run(&src, bytecode);
            assert!(
                matches!(sig.outcome, SigOutcome::Completed(_)),
                "{:?}",
                sig.outcome
            );
            assert!(sig.output.contains("B0 -"), "{}", sig.output);
        }
    }

    #[test]
    fn every_method_and_atomic_renders_and_runs() {
        let mut ops = setup();
        ops.push(Op::NewView {
            slot: 1,
            kind: Kind::DataView,
            buf: 0,
            offset: 0,
            len: None,
        });
        for (name, _) in METHODS {
            for view in [0, 1] {
                ops.push(Op::Method {
                    view,
                    name,
                    args: [Arg::plain("1"); 3],
                });
            }
        }
        for (op, _) in ATOMICS {
            ops.push(Op::Atomic {
                view: 0,
                op,
                idx: 1,
                args: [Arg::plain("1"); 2],
            });
        }
        let src = render(&ops);
        for bytecode in [true, false] {
            let sig = run(&src, bytecode);
            assert!(
                matches!(sig.outcome, SigOutcome::Completed(_)),
                "{:?}\n{src}",
                sig.outcome
            );
        }
    }

    #[test]
    fn hostile_argument_resizes_the_buffer_mid_call() {
        let mut ops = setup();
        ops.push(Op::Method {
            view: 0,
            name: "fill",
            args: [
                Arg::plain("1"),
                Arg::plain("0"),
                Arg::hostile("16", Action::Resize, 0, 1),
            ],
        });
        let sig = run(&render(&ops), true);
        assert!(sig.output.contains("2 object:"), "{}", sig.output);
        assert!(sig.output.contains("B0 1,24,true,false,"), "{}", sig.output);
        assert!(sig.output.contains("V0 1,1,0"), "{}", sig.output);
    }

    #[test]
    fn hostile_callback_detaches_the_buffer_mid_iteration() {
        let mut ops = setup();
        ops.push(Op::Method {
            view: 0,
            name: "forEach",
            args: [
                Arg::hostile("0", Action::Detach, 0, 0),
                Arg::plain("0"),
                Arg::plain("0"),
            ],
        });
        let sig = run(&render(&ops), false);
        assert!(sig.output.contains("B0 0,0,true,true,-1"), "{}", sig.output);
    }

    #[test]
    fn atomics_wait_only_with_zero_timeout_and_no_agents() {
        let mut ops = setup();
        ops.push(Op::Atomic {
            view: 0,
            op: "wait",
            idx: 0,
            args: [Arg::plain("0"), Arg::plain("0")],
        });
        let src = render(&ops);
        assert!(src.contains("Atomics.wait(V[0],0,"));
        assert!(src.contains(",0)"));
        for ident in ["agent", "waitAsync", "sleep"] {
            assert!(!contains_ident(&src, ident), "{ident}");
        }
        for (_, template) in ATOMICS.iter().chain(METHODS) {
            assert!(!template.contains("waitAsync"));
        }
    }

    #[test]
    fn transfer_replaces_the_slot_and_detaches_the_source() {
        let mut ops = setup();
        ops.push(Op::Transfer {
            buf: 0,
            mode: TransferMode::Fixed(3),
        });
        let sig = run(&render(&ops), true);
        assert!(sig.output.contains("B0 3,24,true,false,"), "{}", sig.output);
        assert!(sig.output.contains("V0 0,0,0"), "{}", sig.output);

        ops.push(Op::Transfer {
            buf: 0,
            mode: TransferMode::ToFixed,
        });
        let sig = run(&render(&ops), false);
        assert!(sig.output.contains("B0 3,3,false,false,"), "{}", sig.output);
    }
}
