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

static NAMES: [&str; 85] = [
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
];

thread_local! {
    static OVERFLOW: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

#[inline(always)]
fn known(name: &str) -> Option<u8> {
    match name {
        "Object" => Some(0),
        "Array" => Some(1),
        "ArrayBuffer" => Some(2),
        "SharedArrayBuffer" => Some(3),
        "Arguments" => Some(4),
        "AbstractModuleSource" => Some(5),
        "AggregateError" => Some(6),
        "AsyncDisposableStack" => Some(7),
        "AsyncFunction" => Some(8),
        "AsyncGenerator" => Some(9),
        "AsyncGeneratorFunction" => Some(10),
        "AsyncIterator" => Some(11),
        "BigInt" => Some(12),
        "Boolean" => Some(13),
        "DataView" => Some(14),
        "Date" => Some(15),
        "DisposableStack" => Some(16),
        "Error" => Some(17),
        "EvalError" => Some(18),
        "RangeError" => Some(19),
        "ReferenceError" => Some(20),
        "SyntaxError" => Some(21),
        "TypeError" => Some(22),
        "URIError" => Some(23),
        "SuppressedError" => Some(24),
        "Test262Error" => Some(25),
        "FinalizationRegistry" => Some(26),
        "Function" => Some(27),
        "Generator" => Some(28),
        "GeneratorFunction" => Some(29),
        "Iterator" => Some(30),
        "Map" => Some(31),
        "Math" => Some(32),
        "Module" => Some(33),
        "Number" => Some(34),
        "Promise" => Some(35),
        "Proxy" => Some(36),
        "RegExp" => Some(37),
        "Set" => Some(38),
        "ShadowRealm" => Some(39),
        "String" => Some(40),
        "Symbol" => Some(41),
        "TypedArray" => Some(42),
        "WeakMap" => Some(43),
        "WeakRef" => Some(44),
        "WeakSet" => Some(45),
        "Int8Array" => Some(46),
        "Uint8Array" => Some(47),
        "Uint8ClampedArray" => Some(48),
        "Int16Array" => Some(49),
        "Uint16Array" => Some(50),
        "Int32Array" => Some(51),
        "Uint32Array" => Some(52),
        "Float16Array" => Some(53),
        "Float32Array" => Some(54),
        "Float64Array" => Some(55),
        "BigInt64Array" => Some(56),
        "BigUint64Array" => Some(57),
        "Intl.Collator" => Some(58),
        "Intl.DateTimeFormat" => Some(59),
        "Intl.DisplayNames" => Some(60),
        "Intl.DurationFormat" => Some(61),
        "Intl.ListFormat" => Some(62),
        "Intl.Locale" => Some(63),
        "Intl.NumberFormat" => Some(64),
        "Intl.PluralRules" => Some(65),
        "Intl.RelativeTimeFormat" => Some(66),
        "Intl.Segmenter" => Some(67),
        "Temporal.Duration" => Some(68),
        "Temporal.Instant" => Some(69),
        "Temporal.PlainDate" => Some(70),
        "Temporal.PlainDateTime" => Some(71),
        "Temporal.PlainMonthDay" => Some(72),
        "Temporal.PlainTime" => Some(73),
        "Temporal.PlainYearMonth" => Some(74),
        "Temporal.ZonedDateTime" => Some(75),
        "Array Iterator" => Some(76),
        "Iterator Helper" => Some(77),
        "Map Iterator" => Some(78),
        "RegExp String Iterator" => Some(79),
        "Segmenter Segments" => Some(80),
        "Segmenter String Iterator" => Some(81),
        "Set Iterator" => Some(82),
        "String Iterator" => Some(83),
        "For-In Iterator" => Some(84),
        _ => None,
    }
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
