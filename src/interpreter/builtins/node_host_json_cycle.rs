//! V8's circular `JSON.stringify` message (`JsonStringifier::
//! ConstructCircularStructureErrorMessage`), under `--node` only (#1532): it
//! shows the objects of the cycle and the property each was reached through.
//! The serializer keeps the open objects; the keys they were reached by are
//! kept beside them in `NodeHostState::json_keys` (only under `--node`).

use super::super::*;

const ENGINE: &str = "Converting circular structure to JSON";
const PREFIX_COUNT: usize = 2;
const POSTFIX_COUNT: usize = 1;

impl Interpreter {
    fn json_constructor_name(&self, id: u64) -> String {
        self.data_property_value(id, "constructor")
            .and_then(|c| c.as_object_id())
            .and_then(|c| self.own_data_string(c, "name"))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| "Object".to_string())
    }

    fn json_key_text(key: &str) -> String {
        match key.parse::<u32>() {
            Ok(index) if index.to_string() == key => format!("index {index}"),
            _ => format!("property '{key}'"),
        }
    }

    pub(crate) fn json_key_push(&mut self, key: &JsPropertyKey) {
        if self.node_host.stack.installed {
            self.node_host.json_keys.push(key.clone());
        }
    }

    pub(crate) fn json_key_pop(&mut self) {
        if self.node_host.stack.installed {
            self.node_host.json_keys.pop();
        }
    }

    /// The key the open object at `index` of `stack` was reached by.
    fn json_open_key(&self, stack: &[u64], index: usize) -> String {
        let keys = &self.node_host.json_keys;
        keys.len()
            .checked_sub(stack.len())
            .and_then(|base| keys.get(base + index))
            .map_or_else(String::new, |key| key.to_string())
    }

    /// `stack` holds the objects being serialized, `target` is the one met
    /// again through `closing`.
    pub(crate) fn json_cycle_error(
        &mut self,
        stack: &[u64],
        target: u64,
        closing: &JsPropertyKey,
    ) -> JsValue {
        let text = self.v8_message(ENGINE, |i| {
            let Some(start) = stack.iter().position(|&id| id == target) else {
                return ENGINE.to_string();
            };
            let cycle = &stack[start..];
            let line = |from: usize| {
                let key = Self::json_key_text(&i.json_open_key(stack, start + from));
                format!(
                    "\n    |     {key} -> object with constructor '{}'",
                    i.json_constructor_name(cycle[from])
                )
            };
            let mut out = format!(
                "{ENGINE}\n    --> starting at object with constructor '{}'",
                i.json_constructor_name(target)
            );
            let prefix_end = cycle.len().min(1 + PREFIX_COUNT);
            (1..prefix_end).for_each(|n| out.push_str(&line(n)));
            if cycle.len() > prefix_end + POSTFIX_COUNT {
                out.push_str("\n    |     ...");
            }
            let postfix_start = prefix_end.max(cycle.len().saturating_sub(POSTFIX_COUNT));
            (postfix_start..cycle.len()).for_each(|n| out.push_str(&line(n)));
            out.push_str(&format!(
                "\n    --- {} closes the circle",
                Self::json_key_text(&closing.to_string())
            ));
            out
        });
        self.create_type_error(&text)
    }
}
