//! The stored form of a property descriptor.
//!
//! A map entry is a key plus a [`PropertyRecord`], and tsc's heap holds tens of
//! millions of them, so the record is 16 bytes: a data property (everything but
//! an accessor) is one value and a flag word, and the accessor halves live in a
//! box that only accessor properties pay for (#1401). A 32-byte record with
//! three inline value slots was two thirds of an entry.

use super::*;

pub(super) const HAS_VALUE: u16 = 1 << 0;
pub(super) const HAS_GET: u16 = 1 << 1;
pub(super) const HAS_SET: u16 = 1 << 2;
pub(super) const HAS_WRITABLE: u16 = 1 << 3;
pub(super) const IS_WRITABLE: u16 = 1 << 4;
pub(super) const HAS_ENUMERABLE: u16 = 1 << 5;
pub(super) const IS_ENUMERABLE: u16 = 1 << 6;
pub(super) const HAS_CONFIGURABLE: u16 = 1 << 7;
pub(super) const IS_CONFIGURABLE: u16 = 1 << 8;

/// Attribute bits of a shaped data property.
pub(super) const ATTR_WRITABLE: u8 = 1;
pub(super) const ATTR_ENUMERABLE: u8 = 2;
pub(super) const ATTR_CONFIGURABLE: u8 = 4;

/// What reading an own property yields, see [`PropertyRecord::read`].
pub(crate) enum PropertyRead<'a> {
    Value(&'a JsValue),
    Getter(&'a JsValue),
}

/// The three value slots of a record that has an accessor half. Absent slots
/// are `undefined`.
#[derive(Clone)]
pub(crate) struct WideSlots {
    value: JsValue,
    get: JsValue,
    set: JsValue,
}

/// A `PropertyDescriptor` as the map stores it: 16 bytes where the
/// `Option`-per-field descriptor takes 56. A flag word records which fields
/// are present, partial descriptors included; an absent value reads as
/// `undefined`. A descriptor with a `get` or `set` half is `Wide`, every other
/// one `Data`.
#[derive(Clone)]
pub(crate) enum PropertyRecord {
    Data { flags: u16, value: JsValue },
    Wide { flags: u16, slots: Box<WideSlots> },
}

impl PropertyRecord {
    #[inline]
    pub(super) fn flags(&self) -> u16 {
        match self {
            Self::Data { flags, .. } | Self::Wide { flags, .. } => *flags,
        }
    }

    #[inline]
    fn flags_mut(&mut self) -> &mut u16 {
        match self {
            Self::Data { flags, .. } | Self::Wide { flags, .. } => flags,
        }
    }

    #[inline]
    fn value_slot(&self) -> &JsValue {
        match self {
            Self::Data { value, .. } => value,
            Self::Wide { slots, .. } => &slots.value,
        }
    }

    #[inline]
    fn value_slot_mut(&mut self) -> &mut JsValue {
        match self {
            Self::Data { value, .. } => value,
            Self::Wide { slots, .. } => &mut slots.value,
        }
    }

    #[inline]
    fn pack(slot: Option<JsValue>, present: u16, flags: &mut u16) -> JsValue {
        match slot {
            Some(v) => {
                *flags |= present;
                v
            }
            None => JsValue::UNDEFINED,
        }
    }

    #[inline]
    fn pack_bool(slot: Option<bool>, has: u16, is: u16, flags: &mut u16) {
        if let Some(b) = slot {
            *flags |= has;
            if b {
                *flags |= is;
            }
        }
    }

    #[inline]
    fn flag(&self, bit: u16) -> bool {
        self.flags() & bit != 0
    }

    #[inline]
    fn tri(&self, has: u16, is: u16) -> Option<bool> {
        self.flag(has).then(|| self.flag(is))
    }

    #[inline]
    fn set_tri(&mut self, has: u16, is: u16, value: Option<bool>) {
        let flags = self.flags_mut();
        *flags &= !(has | is);
        Self::pack_bool(value, has, is, flags);
    }

    #[inline]
    pub(crate) fn value(&self) -> Option<&JsValue> {
        self.flag(HAS_VALUE).then(|| self.value_slot())
    }

    #[inline]
    pub(crate) fn getter(&self) -> Option<&JsValue> {
        match self {
            Self::Wide { flags, slots } if flags & HAS_GET != 0 => Some(&slots.get),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn setter(&self) -> Option<&JsValue> {
        match self {
            Self::Wide { flags, slots } if flags & HAS_SET != 0 => Some(&slots.set),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn writable(&self) -> Option<bool> {
        self.tri(HAS_WRITABLE, IS_WRITABLE)
    }

    #[inline]
    pub(crate) fn enumerable(&self) -> Option<bool> {
        self.tri(HAS_ENUMERABLE, IS_ENUMERABLE)
    }

    #[inline]
    pub(crate) fn configurable(&self) -> Option<bool> {
        self.tri(HAS_CONFIGURABLE, IS_CONFIGURABLE)
    }

    #[inline]
    pub(crate) fn is_data_descriptor(&self) -> bool {
        self.flags() & (HAS_VALUE | HAS_WRITABLE) != 0
    }

    #[inline]
    pub(crate) fn is_accessor_descriptor(&self) -> bool {
        self.flags() & (HAS_GET | HAS_SET) != 0
    }

    /// A data property with a value, no accessor half, and `writable: true`.
    #[inline]
    fn is_plain_writable_data(&self) -> bool {
        const MASK: u16 = HAS_VALUE | HAS_GET | HAS_SET | HAS_WRITABLE | IS_WRITABLE;
        self.flags() & MASK == HAS_VALUE | HAS_WRITABLE | IS_WRITABLE
    }

    /// What [[Get]] reads from this own property: its value, or the getter
    /// to call with the receiver. An accessor without a getter reads
    /// `undefined`, like a descriptor without a value.
    #[inline]
    pub(crate) fn read(&self) -> PropertyRead<'_> {
        match self {
            Self::Data { value, .. } => PropertyRead::Value(value),
            Self::Wide { .. } => {
                if self.is_accessor_descriptor() {
                    return match self.getter() {
                        Some(getter) if !getter.is_undefined() => PropertyRead::Getter(getter),
                        _ => PropertyRead::Value(JsValue::undefined_ref()),
                    };
                }
                PropertyRead::Value(self.value().unwrap_or(JsValue::undefined_ref()))
            }
        }
    }

    /// Adds `delta` to a writable data property that holds a Number, in
    /// place, and returns the old and new values. `None` leaves the record
    /// untouched.
    #[inline]
    pub(crate) fn add_to_writable_number(&mut self, delta: f64) -> Option<(f64, f64)> {
        if !self.is_plain_writable_data() {
            return None;
        }
        let slot = self.value_slot_mut();
        let old = slot.as_number()?;
        let new = old + delta;
        *slot = JsValue::number(new);
        Some((old, new))
    }

    /// Replaces the value of a writable data property in place. Returns
    /// `false`, leaving the record untouched, when it is anything else (an
    /// accessor, or a data property that is not writable).
    #[inline]
    pub(crate) fn overwrite_writable_data(&mut self, value: &JsValue) -> bool {
        if !self.is_plain_writable_data() {
            return false;
        }
        *self.value_slot_mut() = value.clone();
        true
    }

    #[inline]
    pub(crate) fn set_value(&mut self, value: Option<JsValue>) {
        let flags = self.flags_mut();
        *flags &= !HAS_VALUE;
        let packed = Self::pack(value, HAS_VALUE, flags);
        *self.value_slot_mut() = packed;
    }

    #[inline]
    pub(crate) fn set_writable(&mut self, value: Option<bool>) {
        self.set_tri(HAS_WRITABLE, IS_WRITABLE, value);
    }

    /// The descriptor this record stores, cloned.
    pub(crate) fn to_descriptor(&self) -> PropertyDescriptor {
        PropertyDescriptor {
            value: self.value().cloned(),
            writable: self.writable(),
            get: self.getter().cloned(),
            set: self.setter().cloned(),
            enumerable: self.enumerable(),
            configurable: self.configurable(),
        }
    }

    pub(crate) fn into_descriptor(self) -> PropertyDescriptor {
        let (writable, enumerable, configurable) =
            (self.writable(), self.enumerable(), self.configurable());
        match self {
            Self::Data { flags, value } => PropertyDescriptor {
                value: (flags & HAS_VALUE != 0).then_some(value),
                writable,
                get: None,
                set: None,
                enumerable,
                configurable,
            },
            Self::Wide { flags, slots } => {
                let has = |bit: u16| flags & bit != 0;
                let WideSlots { value, get, set } = *slots;
                PropertyDescriptor {
                    value: has(HAS_VALUE).then_some(value),
                    writable,
                    get: has(HAS_GET).then_some(get),
                    set: has(HAS_SET).then_some(set),
                    enumerable,
                    configurable,
                }
            }
        }
    }

    /// The attribute bits and value of a complete data property (value,
    /// writable, enumerable and configurable all present), the only kind a
    /// shape can describe; gives the record back when it is anything else.
    #[inline]
    pub(super) fn into_plain(self) -> Result<(u8, JsValue), Self> {
        const COMPLETE: u16 = HAS_VALUE | HAS_WRITABLE | HAS_ENUMERABLE | HAS_CONFIGURABLE;
        match self {
            Self::Data { flags, value } if flags & COMPLETE == COMPLETE => Ok((
                (u8::from(flags & IS_WRITABLE != 0) * ATTR_WRITABLE)
                    | (u8::from(flags & IS_ENUMERABLE != 0) * ATTR_ENUMERABLE)
                    | (u8::from(flags & IS_CONFIGURABLE != 0) * ATTR_CONFIGURABLE),
                value,
            )),
            other => Err(other),
        }
    }

    /// The record of the complete data property `into_plain` described.
    #[inline]
    pub(super) fn from_plain(attrs: u8, value: JsValue) -> Self {
        Self::data(
            value,
            attrs & ATTR_WRITABLE != 0,
            attrs & ATTR_ENUMERABLE != 0,
            attrs & ATTR_CONFIGURABLE != 0,
        )
    }

    /// The record of `PropertyDescriptor::data(value, writable, enumerable,
    /// configurable)`, built without the per-field `Option` packing.
    #[inline(always)]
    pub(crate) fn data(
        value: JsValue,
        writable: bool,
        enumerable: bool,
        configurable: bool,
    ) -> Self {
        Self::Data {
            value,
            flags: HAS_VALUE
                | HAS_WRITABLE
                | HAS_ENUMERABLE
                | HAS_CONFIGURABLE
                | if writable { IS_WRITABLE } else { 0 }
                | if enumerable { IS_ENUMERABLE } else { 0 }
                | if configurable { IS_CONFIGURABLE } else { 0 },
        }
    }

    /// The record of `PropertyDescriptor::accessor(Some(getter), None,
    /// enumerable, configurable)`.
    #[inline(always)]
    pub(crate) fn getter_only(getter: JsValue, enumerable: bool, configurable: bool) -> Self {
        Self::Wide {
            slots: Box::new(WideSlots {
                value: JsValue::UNDEFINED,
                get: getter,
                set: JsValue::UNDEFINED,
            }),
            flags: HAS_GET
                | HAS_ENUMERABLE
                | HAS_CONFIGURABLE
                | if enumerable { IS_ENUMERABLE } else { 0 }
                | if configurable { IS_CONFIGURABLE } else { 0 },
        }
    }
}

impl From<PropertyDescriptor> for PropertyRecord {
    #[inline]
    fn from(d: PropertyDescriptor) -> Self {
        let mut flags = 0;
        let value = Self::pack(d.value, HAS_VALUE, &mut flags);
        Self::pack_bool(d.writable, HAS_WRITABLE, IS_WRITABLE, &mut flags);
        Self::pack_bool(d.enumerable, HAS_ENUMERABLE, IS_ENUMERABLE, &mut flags);
        Self::pack_bool(
            d.configurable,
            HAS_CONFIGURABLE,
            IS_CONFIGURABLE,
            &mut flags,
        );
        if d.get.is_none() && d.set.is_none() {
            return Self::Data { flags, value };
        }
        let get = Self::pack(d.get, HAS_GET, &mut flags);
        let set = Self::pack(d.set, HAS_SET, &mut flags);
        Self::Wide {
            flags,
            slots: Box::new(WideSlots { value, get, set }),
        }
    }
}

impl std::fmt::Debug for PropertyRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.to_descriptor().fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::helpers::same_value;

    #[test]
    fn entry_is_two_words_of_key_and_two_of_record() {
        assert_eq!(std::mem::size_of::<PropertyRecord>(), 16);
        assert_eq!(
            std::mem::size_of::<Entry>(),
            std::mem::size_of::<JsPropertyKey>() + 16
        );
        assert_eq!(
            std::mem::size_of::<Option<Entry>>(),
            std::mem::size_of::<Entry>()
        );
    }

    fn same_record(a: &PropertyRecord, b: &PropertyRecord) -> bool {
        let same = |x: Option<&JsValue>, y: Option<&JsValue>| match (x, y) {
            (Some(x), Some(y)) => same_value(x, y),
            (None, None) => true,
            _ => false,
        };
        a.flags() == b.flags()
            && same(a.value(), b.value())
            && same(a.getter(), b.getter())
            && same(a.setter(), b.setter())
    }

    #[test]
    fn data_record_matches_the_packed_descriptor() {
        for bits in 0..8u8 {
            let (w, e, c) = (bits & 1 != 0, bits & 2 != 0, bits & 4 != 0);
            let direct = PropertyRecord::data(JsValue::number(3.0), w, e, c);
            let packed =
                PropertyRecord::from(PropertyDescriptor::data(JsValue::number(3.0), w, e, c));
            assert!(same_record(&direct, &packed), "data({w}, {e}, {c})");
        }
    }

    #[test]
    fn getter_record_matches_the_packed_descriptor() {
        for bits in 0..4u8 {
            let (e, c) = (bits & 1 != 0, bits & 2 != 0);
            let getter = JsValue::number(7.0);
            let direct = PropertyRecord::getter_only(getter.clone(), e, c);
            let packed =
                PropertyRecord::from(PropertyDescriptor::accessor(Some(getter), None, e, c));
            assert!(same_record(&direct, &packed), "getter({e}, {c})");
        }
    }

    #[test]
    fn every_partial_descriptor_round_trips() {
        let some = |b: bool, v: f64| b.then(|| JsValue::number(v));
        let flag = |b: bool, on: bool| b.then_some(on);
        for bits in 0..512u16 {
            let bit = |i: u16| bits >> i & 1 != 0;
            let d = PropertyDescriptor {
                value: some(bit(0), 1.0),
                get: some(bit(1), 2.0),
                set: some(bit(2), 3.0),
                writable: flag(bit(3), bit(4)),
                enumerable: flag(bit(5), bit(6)),
                configurable: flag(bit(7), bit(8)),
            };
            let record = PropertyRecord::from(d.clone());
            let back = record.to_descriptor();
            let same = |x: &Option<JsValue>, y: &Option<JsValue>| match (x, y) {
                (Some(x), Some(y)) => same_value(x, y),
                (None, None) => true,
                _ => false,
            };
            assert!(same(&d.value, &back.value), "value {bits:#b}");
            assert!(same(&d.get, &back.get), "get {bits:#b}");
            assert!(same(&d.set, &back.set), "set {bits:#b}");
            assert_eq!(
                (d.writable, d.enumerable, d.configurable),
                (back.writable, back.enumerable, back.configurable)
            );
            let owned = record.into_descriptor();
            assert!(same(&d.get, &owned.get) && same(&d.value, &owned.value));
        }
    }
}
