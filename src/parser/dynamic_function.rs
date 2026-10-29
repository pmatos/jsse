use super::*;

/// CreateDynamicFunction (§20.2.1.1.1) steps 17-18: `P` must parse as
/// FormalParameters and `bodyString` as a FunctionBody, each on its own, so
/// neither can close or reopen the synthetic wrapper that the combined source
/// is parsed under.
pub(crate) fn check_dynamic_function_parts(
    params: &str,
    body: &str,
    stand_ins: bool,
    is_generator: bool,
    is_async: bool,
) -> Result<(), ParseError> {
    let params_src = format!("({params}\n)");
    let mut p = Parser::for_source(&params_src, stand_ins)?;
    p.in_generator = is_generator;
    p.in_async = is_async;
    p.in_non_arrow_function += 1;
    p.parse_formal_parameters()?;
    p.expect_end_of_input("formal parameters")?;

    let body_src = format!("{{\n{body}\n}}");
    let mut p = Parser::for_source(&body_src, stand_ins)?;
    p.parse_function_body_with_context(is_generator, is_async)?;
    p.expect_end_of_input("function body")
}

impl Parser<'_> {
    fn expect_end_of_input(&self, what: &str) -> Result<(), ParseError> {
        if self.current == Token::Eof {
            Ok(())
        } else {
            Err(self.error(format!("Unexpected token {:?} after {what}", self.current)))
        }
    }
}
