//! %RegExpStringIteratorPrototype%.next (sec-%regexpstringiteratorprototype%.next):
//! GeneratorResume over the matchAll closure, rejecting re-entrant calls.

use super::super::*;
use super::regexp::{
    advance_string_index, regex_input_for_subject, regexp_exec_abstract, spec_set,
};

/// The flag is true when the closure suspended at a yield and can resume.
fn rsi_next_step(
    interp: &mut Interpreter,
    string: JsString,
    global: bool,
    full_unicode: bool,
    mid: u64,
) -> (Completion, bool) {
    let regex_input = regex_input_for_subject(interp, string);

    let result_val = match regexp_exec_abstract(interp, mid, &regex_input) {
        Completion::Normal(v) => v,
        other => return (other, false),
    };

    if result_val.is_null() {
        let done = interp.create_iter_result_object(JsValue::UNDEFINED, true);
        return (Completion::Normal(done), false);
    }

    let yielded = |interp: &mut Interpreter, more: bool| {
        let item = interp.create_iter_result_object(result_val.clone(), false);
        (Completion::Normal(item), more)
    };

    if !global {
        return yielded(interp, false);
    }

    let Some(result_id) = result_val.as_object_id() else {
        return yielded(interp, false);
    };
    let match_str_val = match interp.get_object_property(result_id, "0", &result_val) {
        Completion::Normal(v) => v,
        other => return (other, false),
    };
    let match_str = match interp.to_string_lossy(&match_str_val) {
        Ok(s) => s,
        Err(e) => return (Completion::Throw(e), false),
    };
    if match_str.is_empty() {
        let matcher_val = JsValue::object(mid);
        let li_val = match interp.get_object_property(mid, "lastIndex", &matcher_val) {
            Completion::Normal(v) => v,
            other => return (other, false),
        };
        let li_num = match interp.to_number_value(&li_val) {
            Ok(n) => n,
            Err(e) => return (Completion::Throw(e), false),
        };
        let this_index = if li_num.is_nan() || li_num <= 0.0 {
            0
        } else {
            li_num.min(9007199254740991.0).floor() as usize
        };
        let next_index = advance_string_index(&regex_input, this_index, full_unicode);
        let next_val = JsValue::number(next_index as f64);
        if let Err(e) = spec_set(interp, mid, "lastIndex", next_val, true) {
            return (Completion::Throw(e), false);
        }
    }

    yielded(interp, true)
}

fn set_rsi_status(interp: &mut Interpreter, o_id: u64, new_status: BytecodeGeneratorStatus) {
    if let Some(obj) = interp.get_object_cell(o_id)
        && let Some(IteratorState::RegExpStringIterator { state, .. }) =
            obj.borrow_mut().iterator_state_mut()
    {
        *state = new_status;
    }
}

pub(super) fn rsi_next(interp: &mut Interpreter, this_val: &JsValue) -> Completion {
    let Some(o_id) = this_val.as_object_id() else {
        return Completion::Throw(interp.create_type_error(
            "%RegExpStringIteratorPrototype%.next requires that 'this' be an Object",
        ));
    };
    let Some(obj) = interp.get_object_cell(o_id) else {
        return Completion::Throw(
            interp
                .create_type_error("%RegExpStringIteratorPrototype%.next called on invalid object"),
        );
    };
    let (string, global, full_unicode, status, mid) =
        if let Some(IteratorState::RegExpStringIterator {
            string,
            global,
            full_unicode,
            state,
            matcher_id,
        }) = obj.borrow().iterator_state()
        {
            (string.clone(), *global, *full_unicode, *state, *matcher_id)
        } else {
            return Completion::Throw(interp.create_type_error(
            "%RegExpStringIteratorPrototype%.next requires that 'this' be a RegExp String Iterator",
        ));
        };

    match status {
        BytecodeGeneratorStatus::Executing => {
            return Completion::Throw(interp.create_type_error(
                "%RegExpStringIteratorPrototype%.next called on an iterator that is already executing",
            ));
        }
        BytecodeGeneratorStatus::Completed => {
            return Completion::Normal(interp.create_iter_result_object(JsValue::UNDEFINED, true));
        }
        BytecodeGeneratorStatus::SuspendedStart | BytecodeGeneratorStatus::SuspendedYield => {}
    }

    set_rsi_status(interp, o_id, BytecodeGeneratorStatus::Executing);
    let (completion, suspended) = rsi_next_step(interp, string, global, full_unicode, mid);
    let settled = if suspended {
        BytecodeGeneratorStatus::SuspendedYield
    } else {
        BytecodeGeneratorStatus::Completed
    };
    set_rsi_status(interp, o_id, settled);
    completion
}
