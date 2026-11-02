//! Class names of ordinary and exotic objects, stored as one byte (#1401).
//!
//! `JsObjectData` used to hold a 16-byte `&'static str`. Every name the engine
//! assigns is in [`NAMES`]; a name outside the table is interned
//! in a per-thread overflow list so that behaviour never depends on the table
//! being complete. Object data never leaves its interpreter's thread.

use super::JsObjectData;
use std::cell::RefCell;

/// Index into [`NAMES`] (or the overflow list past its end).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct ClassId(u8);

/// The one list of class names; its order is the byte each name is stored as.
macro_rules! class_names {
    ($($name:literal),+ $(,)?) => {
        static NAMES: &[&str] = &[$($name),+];

        /// The table index of `name`, folded to a constant for a literal.
        #[inline(always)]
        fn known(name: &str) -> Option<u8> {
            match name {
                $($name => Some(const { position($name) }),)+
                _ => None,
            }
        }
    };
}

const fn position(name: &str) -> u8 {
    let mut i = 0;
    while i < NAMES.len() {
        let (a, b) = (NAMES[i].as_bytes(), name.as_bytes());
        if a.len() == b.len() {
            let mut k = 0;
            while k < a.len() && a[k] == b[k] {
                k += 1;
            }
            if k == a.len() {
                return i as u8;
            }
        }
        i += 1;
    }
    panic!("class name is not in the table")
}

class_names! {
    "Object",
    "Array",
    "ArrayBuffer",
    "SharedArrayBuffer",
    "Arguments",
    "AbstractModuleSource",
    "AggregateError",
    "AsyncDisposableStack",
    "AsyncFunction",
    "AsyncGenerator",
    "AsyncGeneratorFunction",
    "AsyncIterator",
    "BigInt",
    "Boolean",
    "DataView",
    "Date",
    "DisposableStack",
    "Error",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
    "SuppressedError",
    "Test262Error",
    "FinalizationRegistry",
    "Function",
    "Generator",
    "GeneratorFunction",
    "Iterator",
    "Map",
    "Math",
    "Module",
    "Number",
    "Promise",
    "Proxy",
    "RegExp",
    "Set",
    "ShadowRealm",
    "String",
    "Symbol",
    "TypedArray",
    "WeakMap",
    "WeakRef",
    "WeakSet",
    "Int8Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "Int16Array",
    "Uint16Array",
    "Int32Array",
    "Uint32Array",
    "Float16Array",
    "Float32Array",
    "Float64Array",
    "BigInt64Array",
    "BigUint64Array",
    "Intl.Collator",
    "Intl.DateTimeFormat",
    "Intl.DisplayNames",
    "Intl.DurationFormat",
    "Intl.ListFormat",
    "Intl.Locale",
    "Intl.NumberFormat",
    "Intl.PluralRules",
    "Intl.RelativeTimeFormat",
    "Intl.Segmenter",
    "Temporal.Duration",
    "Temporal.Instant",
    "Temporal.PlainDate",
    "Temporal.PlainDateTime",
    "Temporal.PlainMonthDay",
    "Temporal.PlainTime",
    "Temporal.PlainYearMonth",
    "Temporal.ZonedDateTime",
    "Array Iterator",
    "Iterator Helper",
    "Map Iterator",
    "RegExp String Iterator",
    "Segmenter Segments",
    "Segmenter String Iterator",
    "Set Iterator",
    "String Iterator",
    "For-In Iterator",
}

thread_local! {
    static OVERFLOW: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

impl ClassId {
    pub(crate) const OBJECT: ClassId = ClassId(0);

    fn overflow_index(name: &str) -> Option<u8> {
        OVERFLOW.with(|list| {
            let at = list.borrow().iter().position(|n| *n == name)?;
            Some((NAMES.len() + at) as u8)
        })
    }

    #[inline(always)]
    pub(crate) fn find(name: &str) -> Option<ClassId> {
        known(name)
            .or_else(|| Self::overflow_index(name))
            .map(ClassId)
    }

    #[inline(always)]
    pub(crate) fn intern(name: &'static str) -> ClassId {
        match known(name) {
            Some(i) => ClassId(i),
            None => Self::intern_overflow(name),
        }
    }

    #[cold]
    fn intern_overflow(name: &'static str) -> ClassId {
        if let Some(i) = Self::overflow_index(name) {
            return ClassId(i);
        }
        OVERFLOW.with(|list| {
            let mut list = list.borrow_mut();
            let at = NAMES.len() + list.len();
            assert!(at < usize::from(u8::MAX), "too many object class names");
            list.push(name);
            ClassId(at as u8)
        })
    }

    pub(crate) fn as_str(self) -> &'static str {
        match NAMES.get(usize::from(self.0)) {
            Some(name) => name,
            None => OVERFLOW.with(|list| list.borrow()[usize::from(self.0) - NAMES.len()]),
        }
    }
}

impl JsObjectData {
    /// The class name, `"Object"` for an ordinary object.
    #[inline]
    pub(crate) fn class_str(&self) -> &'static str {
        self.class.as_str()
    }

    #[inline]
    pub(crate) fn set_class(&mut self, name: &'static str) {
        self.class = ClassId::intern(name);
    }

    /// Whether the class name is `name`.
    #[inline(always)]
    pub(crate) fn is_class(&self, name: &str) -> bool {
        ClassId::find(name) == Some(self.class)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_name_round_trips() {
        for (i, name) in NAMES.iter().enumerate() {
            let id = ClassId::intern(name);
            assert_eq!(usize::from(id.0), i, "{name} is listed once");
            assert_eq!(id.as_str(), *name);
            assert_eq!(ClassId::find(name), Some(id));
        }
        assert_eq!(ClassId::OBJECT.as_str(), "Object");
    }

    #[test]
    fn unlisted_names_are_interned_per_thread() {
        assert_eq!(ClassId::find("Not A Listed Class"), None);
        let id = ClassId::intern("Not A Listed Class");
        assert_eq!(id.as_str(), "Not A Listed Class");
        assert_eq!(ClassId::intern("Not A Listed Class"), id);
        assert_eq!(ClassId::find("Not A Listed Class"), Some(id));
        let mut data = JsObjectData::new();
        assert!(data.is_class("Object"));
        data.set_class("Not A Listed Class");
        assert!(data.is_class("Not A Listed Class"));
        assert!(!data.is_class("Object"));
        assert_eq!(data.class_str(), "Not A Listed Class");
    }
}
