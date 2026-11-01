use super::*;

pub(super) fn check_dynamic_parts(
    interp: &mut Interpreter,
    params: &str,
    body: &str,
    pua: bool,
    is_generator: bool,
    is_async: bool,
) -> Result<(), JsValue> {
    parser::dynamic_function::check_dynamic_function_parts(
        params,
        body,
        pua,
        is_generator,
        is_async,
    )
    .map_err(|e| interp.create_error("SyntaxError", &e.js()))
}
