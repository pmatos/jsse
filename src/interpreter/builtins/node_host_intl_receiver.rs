//! V8's receiver wording for the `Intl` classes, under `--node` only (#1568):
//! `Method Intl.ListFormat.prototype.format called on incompatible receiver
//! #<Object>`. The engine's texts differ per class (`Intl.X method called on
//! ...`, `... called on non-X object`, `... requires an Intl.Locale object`);
//! the class comes from the text and the method from the running builtin.
//! `format` and `resolvedOptions` of `NumberFormat` and `DateTimeFormat` unwrap
//! their receiver first, and name `UnwrapNumberFormat` once it is an object.

use super::super::*;

const TAILS: [&str; 3] = [
    " called on incompatible receiver",
    " requires an Intl.Locale object",
    " called on non-",
];

impl Interpreter {
    pub(crate) fn intl_wording(&self, msg: &str) -> Option<String> {
        let rest = msg.strip_prefix("Intl.")?;
        let at = TAILS.iter().find_map(|tail| rest.find(tail))?;
        let mut parts = rest[..at].split(' ').next()?.split('.');
        let class = parts.next()?;
        let named = parts.nth(1);
        let (func, receiver) = self.top_native()?;
        let own = self.own_data_string(func, "name")?;
        let own = own.as_str();
        let getter = own.starts_with("get ");
        let method = named.unwrap_or_else(|| own.trim_start_matches("get "));
        if method.is_empty() || method.contains(' ') {
            return None;
        }
        if matches!(class, "NumberFormat" | "DateTimeFormat")
            && matches!(method, "format" | "resolvedOptions")
            && receiver.as_object_id().is_some()
        {
            let shown = if class == "NumberFormat" {
                "undefined".to_string()
            } else {
                self.v8_plain_text(&receiver)
            };
            return Some(format!(
                "Method Unwrap{class} called on incompatible receiver {shown}"
            ));
        }
        let accessor = getter && matches!(method, "format" | "compare");
        let label = format!(
            "{}Intl.{class}.prototype.{method}",
            if accessor { "get " } else { "" }
        );
        Some(self.incompatible(&label, &receiver))
    }
}
