use super::*;

impl Parser<'_> {
    pub(in super::super) fn set_function_param_names(&mut self, params: &[Pattern]) {
        let mut names = HashSet::new();
        for p in params {
            let mut bound = Vec::new();
            p.bound_names(&mut bound);
            names.extend(bound);
        }
        self.function_param_names = Some(names);
    }
}
