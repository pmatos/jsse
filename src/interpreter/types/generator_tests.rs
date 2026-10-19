use super::*;

fn empty_state_machine() -> Rc<GeneratorStateMachine> {
    Rc::new(GeneratorStateMachine {
        states: Rc::from(Vec::new()),
        local_vars: Vec::new(),
        params: Vec::new(),
        num_yields: 0,
        temp_vars: Vec::new(),
        #[cfg(feature = "perf-counters")]
        perf_key: None,
    })
}

#[test]
fn completed_sync_generator_clears_every_pending_field() {
    let state = IteratorState::completed_state_machine_generator(
        empty_state_machine(),
        Environment::new(None),
        true,
    );
    match state {
        IteratorState::StateMachineGenerator {
            is_strict,
            execution_state,
            _sent_value,
            try_stack,
            pending_binding,
            delegated_iterator,
            pending_exception,
            pending_return,
            ..
        } => {
            assert!(is_strict);
            assert!(matches!(
                execution_state,
                StateMachineExecutionState::Completed
            ));
            assert!(_sent_value.is_undefined());
            assert!(try_stack.is_empty());
            assert!(pending_binding.is_none());
            assert!(delegated_iterator.is_none());
            assert!(pending_exception.is_none());
            assert!(pending_return.is_none());
        }
        _ => panic!("expected a StateMachineGenerator"),
    }
}

#[test]
fn completed_async_generator_clears_every_pending_field() {
    let state = IteratorState::completed_state_machine_async_generator(
        empty_state_machine(),
        Environment::new(None),
        false,
    );
    match state {
        IteratorState::StateMachineAsyncGenerator {
            is_strict,
            execution_state,
            _sent_value,
            try_stack,
            pending_binding,
            delegated_iterator,
            pending_exception,
            pending_return,
            ..
        } => {
            assert!(!is_strict);
            assert!(matches!(
                execution_state,
                StateMachineExecutionState::Completed
            ));
            assert!(_sent_value.is_undefined());
            assert!(try_stack.is_empty());
            assert!(pending_binding.is_none());
            assert!(delegated_iterator.is_none());
            assert!(pending_exception.is_none());
            assert!(pending_return.is_none());
        }
        _ => panic!("expected a StateMachineAsyncGenerator"),
    }
}
