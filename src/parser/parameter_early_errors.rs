use super::*;

/// Grammar facts select validation policy; callers retain parsing and its phases.
#[derive(Clone, Copy, Debug)]
pub(super) enum ParameterGrammar<'a> {
    Function {
        name: Option<&'a str>,
        is_async: bool,
        is_generator: bool,
    },
    // Class methods reach this check with strict ambient context.
    Method,
    SuspendingMethod,
    Accessor,
    Arrow,
}

impl Parser<'_> {
    /// Cover-list checks run before parsing an arrow's body, so parameter errors
    /// still take precedence over body syntax errors.
    pub(super) fn check_arrow_head_parameters(&self, params: &[Pattern]) -> Result<(), ParseError> {
        if self.strict {
            self.check_strict_params(params)?;
        }
        self.check_duplicate_params_strict(params)
    }

    /// Called after the body parser restores ambient strictness. The directive
    /// flag records an explicit Use Strict Directive, not effective strictness.
    pub(super) fn check_body_parameters(
        &self,
        params: &[Pattern],
        grammar: ParameterGrammar<'_>,
        body_has_use_strict: bool,
    ) -> Result<(), ParseError> {
        let simple = Self::is_simple_parameter_list(params);
        if body_has_use_strict && !simple {
            return Err(self.error(
                "Illegal 'use strict' directive in function with non-simple parameter list",
            ));
        }
        // Arrows already checked names at the head. Async/generator object
        // methods retain their existing omission of post-body strict-name checks.
        if body_has_use_strict
            && !matches!(
                grammar,
                ParameterGrammar::Arrow | ParameterGrammar::SuspendingMethod
            )
        {
            if let ParameterGrammar::Function {
                name: Some(name), ..
            } = grammar
            {
                self.check_strict_function_name(name)?;
            }
            self.check_strict_params(params)?;
        }
        let unique = match grammar {
            ParameterGrammar::Function {
                is_async,
                is_generator,
                ..
            } => body_has_use_strict || self.strict || is_async || is_generator || !simple,
            ParameterGrammar::Method | ParameterGrammar::SuspendingMethod => true,
            ParameterGrammar::Accessor => body_has_use_strict || self.strict,
            ParameterGrammar::Arrow => false,
        };
        if unique {
            self.check_duplicate_params_strict(params)?;
        }
        Ok(())
    }

    fn is_simple_parameter_list(params: &[Pattern]) -> bool {
        params.iter().all(|p| matches!(p, Pattern::Identifier(_)))
    }

    fn check_duplicate_params_strict(&self, params: &[Pattern]) -> Result<(), ParseError> {
        let mut seen = HashSet::new();
        let mut names = Vec::new();
        for p in params {
            p.bound_names(&mut names);
        }
        for name in &names {
            if !seen.insert(name.as_str()) {
                return Err(self.error("Duplicate parameter name not allowed in this context"));
            }
        }
        Ok(())
    }

    fn check_strict_function_name(&self, name: &str) -> Result<(), ParseError> {
        if name == "eval" || name == "arguments" {
            return Err(self.error(format!(
                "'{name}' is not allowed as a function name in strict mode"
            )));
        }
        if Self::is_strict_reserved_word(name) {
            return Err(self.error(format!("Unexpected strict mode reserved word '{name}'")));
        }
        Ok(())
    }

    fn check_strict_params(&self, params: &[Pattern]) -> Result<(), ParseError> {
        let mut names = Vec::new();
        for p in params {
            p.bound_names(&mut names);
        }
        for name in &names {
            if name == "eval" || name == "arguments" {
                return Err(self.error(format!(
                    "'{}' can't be used as a parameter name in strict mode",
                    name
                )));
            }
            if Self::is_strict_reserved_word(name) {
                return Err(self.error(format!("Unexpected strict mode reserved word '{name}'")));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::ParameterGrammar;

    const DUPLICATE: &str = "Duplicate parameter name not allowed in this context";
    const DIRECTIVE: &str =
        "Illegal 'use strict' directive in function with non-simple parameter list";

    fn parameters(source: &str) -> Vec<Pattern> {
        Parser::new(source).unwrap().parse_param_list().unwrap()
    }

    #[test]
    fn body_parameter_policy_owns_grammar_and_error_precedence() {
        use ParameterGrammar::*;
        let ordinary = Function {
            name: None,
            is_async: false,
            is_generator: false,
        };
        let asynchronous = Function {
            name: None,
            is_async: true,
            is_generator: false,
        };
        let generator = Function {
            name: None,
            is_async: false,
            is_generator: true,
        };
        let async_generator = Function {
            name: None,
            is_async: true,
            is_generator: true,
        };
        let mut parser = Parser::new("").unwrap();
        let duplicate = parameters("(a, a)");
        for (grammar, rejected) in [
            (ordinary, false),
            (asynchronous, true),
            (generator, true),
            (async_generator, true),
            (Method, true),
            (SuspendingMethod, true),
            (Accessor, false),
            (Arrow, false),
        ] {
            let result = parser.check_body_parameters(&duplicate, grammar, false);
            assert_eq!(result.is_err(), rejected, "{grammar:?}");
            if rejected {
                assert_eq!(result.unwrap_err().message, DUPLICATE);
            }
        }
        parser.set_strict(true);
        assert_eq!(
            parser
                .check_body_parameters(&duplicate, ordinary, false)
                .unwrap_err()
                .message,
            DUPLICATE
        );
        assert_eq!(
            parser
                .check_body_parameters(&duplicate, Accessor, false)
                .unwrap_err()
                .message,
            DUPLICATE
        );
        parser.set_strict(false);

        let named = Function {
            name: Some("eval"),
            is_async: false,
            is_generator: false,
        };
        let non_simple = parameters("(arguments = 0, arguments)");
        assert_eq!(
            parser
                .check_body_parameters(&non_simple, named, true)
                .unwrap_err()
                .message,
            DIRECTIVE
        );
        let names = parameters("(arguments, arguments)");
        assert_eq!(
            parser
                .check_body_parameters(&names, named, true)
                .unwrap_err()
                .message,
            "'eval' is not allowed as a function name in strict mode"
        );
        assert_eq!(
            parser
                .check_body_parameters(&names, ordinary, true)
                .unwrap_err()
                .message,
            "'arguments' can't be used as a parameter name in strict mode"
        );
        for grammar in [
            ordinary,
            asynchronous,
            generator,
            async_generator,
            Method,
            SuspendingMethod,
            Accessor,
            Arrow,
        ] {
            for source in ["(a = 0)", "(...a)", "({a})", "([a])"] {
                assert_eq!(
                    parser
                        .check_body_parameters(&parameters(source), grammar, true)
                        .unwrap_err()
                        .message,
                    DIRECTIVE,
                    "{grammar:?}: {source}"
                );
            }
        }
    }

    #[test]
    fn arrow_parameter_checks_keep_their_two_phases() {
        let mut parser = Parser::new("").unwrap();
        let names = parameters("(eval, eval)");
        assert_eq!(
            parser
                .check_arrow_head_parameters(&names)
                .unwrap_err()
                .message,
            DUPLICATE
        );
        parser.set_strict(true);
        assert_eq!(
            parser
                .check_arrow_head_parameters(&names)
                .unwrap_err()
                .message,
            "'eval' can't be used as a parameter name in strict mode"
        );
        // The body phase must not repeat head checks after parsing the body.
        assert!(
            parser
                .check_body_parameters(&names, ParameterGrammar::Arrow, true)
                .is_ok()
        );
        let nested = parameters("({left: [a]}, ...a)");
        parser.set_strict(false);
        assert_eq!(
            parser
                .check_arrow_head_parameters(&nested)
                .unwrap_err()
                .message,
            DUPLICATE
        );
    }

    #[test]
    fn parser_finalizers_preserve_diagnostics_and_phase_order() {
        for (source, message) in [
            ("function f(a, a = 0) { 'use strict'; }", DIRECTIVE),
            (
                "function eval(a, a) { 'use strict'; }",
                "'eval' is not allowed as a function name in strict mode",
            ),
            (
                "(function f(eval, eval) { 'use strict'; });",
                "'eval' can't be used as a parameter name in strict mode",
            ),
            ("({m(a, a) {}});", DUPLICATE),
            ("class C {m(a, a) {}}", DUPLICATE),
            ("(a, a) => { return +; };", DUPLICATE),
            ("async (a, a) => { return +; };", DUPLICATE),
            ("(a = 0) => { 'use strict'; };", DIRECTIVE),
        ] {
            let error = Parser::new(source).unwrap().parse_program().unwrap_err();
            assert_eq!(error.message, message, "{source}");
        }
        // These finalizers intentionally retain their existing narrower checks.
        for source in [
            "({async m(eval) { 'use strict'; }});",
            "({*m(eval) { 'use strict'; }});",
            "({set m({a, b: a}) {}});",
        ] {
            assert!(
                Parser::new(source).unwrap().parse_program().is_ok(),
                "{source}"
            );
        }
    }
}
