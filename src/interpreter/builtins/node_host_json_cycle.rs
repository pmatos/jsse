//! V8's circular `JSON.stringify` message (`JsonStringifier::
//! ConstructCircularStructureErrorMessage`), under `--node` only (#1532): it
//! shows the objects of the cycle and the property each was reached through.
//! The serializer keeps the open objects, not the keys, so the keys are
//! found again from the objects (the first own data property that holds the
//! next one) on this error path.

use super::super::*;
use crate::interpreter::types::OwnKeyFilter;

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

    fn json_key_holding(&self, parent: u64, child: u64) -> Option<String> {
        let obj = self.get_object(parent)?;
        let keys = obj
            .borrow()
            .ordinary_own_property_keys(OwnKeyFilter::EnumerableStrings)?;
        keys.into_iter().find_map(|key| {
            let held = obj.borrow().get_own_property(&key)?.value?;
            (held.as_object_id() == Some(child)).then(|| key.to_string())
        })
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
                let key = i.json_key_holding(cycle[from - 1], cycle[from]);
                let key =
                    key.map_or_else(|| "property ''".to_string(), |k| Self::json_key_text(&k));
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
