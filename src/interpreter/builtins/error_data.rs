/// Whether an object with this `class_name` carries an [[ErrorData]] internal
/// slot (ECMA-262 20.1.3.6 step 9 builtinTag "Error").
pub(super) fn has_error_data_slot(class_name: &str) -> bool {
    matches!(
        class_name,
        "Error"
            | "TypeError"
            | "RangeError"
            | "ReferenceError"
            | "SyntaxError"
            | "URIError"
            | "EvalError"
            | "AggregateError"
            | "SuppressedError"
    )
}
