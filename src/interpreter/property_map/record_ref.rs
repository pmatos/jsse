//! Borrowed views of a property's record.
//!
//! A per-object map stores a [`PropertyRecord`]; a shaped one stores only the
//! value, with the attributes in its shape. Callers read and update a property
//! through these views, which behave the same for both.

use super::record::{ATTR_CONFIGURABLE, ATTR_ENUMERABLE, ATTR_WRITABLE};
use super::*;

/// A shared view of one own property.
#[derive(Clone, Copy)]
pub(crate) enum RecordRef<'a> {
    Stored(&'a PropertyRecord),
    /// A complete data property of a shaped map, with its attribute bits.
    Plain {
        value: &'a JsValue,
        attrs: u8,
    },
}

impl<'a> RecordRef<'a> {
    #[inline]
    pub(crate) fn value(self) -> Option<&'a JsValue> {
        match self {
            Self::Stored(record) => record.value(),
            Self::Plain { value, .. } => Some(value),
        }
    }

    #[inline]
    pub(crate) fn getter(self) -> Option<&'a JsValue> {
        match self {
            Self::Stored(record) => record.getter(),
            Self::Plain { .. } => None,
        }
    }

    #[inline]
    pub(crate) fn setter(self) -> Option<&'a JsValue> {
        match self {
            Self::Stored(record) => record.setter(),
            Self::Plain { .. } => None,
        }
    }

    #[inline]
    pub(crate) fn writable(self) -> Option<bool> {
        match self {
            Self::Stored(record) => record.writable(),
            Self::Plain { attrs, .. } => Some(attrs & ATTR_WRITABLE != 0),
        }
    }

    #[inline]
    pub(crate) fn enumerable(self) -> Option<bool> {
        match self {
            Self::Stored(record) => record.enumerable(),
            Self::Plain { attrs, .. } => Some(attrs & ATTR_ENUMERABLE != 0),
        }
    }

    #[inline]
    pub(crate) fn configurable(self) -> Option<bool> {
        match self {
            Self::Stored(record) => record.configurable(),
            Self::Plain { attrs, .. } => Some(attrs & ATTR_CONFIGURABLE != 0),
        }
    }

    #[inline]
    pub(crate) fn is_data_descriptor(self) -> bool {
        match self {
            Self::Stored(record) => record.is_data_descriptor(),
            Self::Plain { .. } => true,
        }
    }

    #[inline]
    pub(crate) fn is_accessor_descriptor(self) -> bool {
        match self {
            Self::Stored(record) => record.is_accessor_descriptor(),
            Self::Plain { .. } => false,
        }
    }

    /// See [`PropertyRecord::read`].
    #[inline]
    pub(crate) fn read(self) -> PropertyRead<'a> {
        match self {
            Self::Stored(record) => record.read(),
            Self::Plain { value, .. } => PropertyRead::Value(value),
        }
    }

    /// The stored form of this property, cloned.
    pub(crate) fn to_record(self) -> PropertyRecord {
        match self {
            Self::Stored(record) => record.clone(),
            Self::Plain { value, attrs } => PropertyRecord::from_plain(attrs, value.clone()),
        }
    }

    pub(crate) fn to_descriptor(self) -> PropertyDescriptor {
        match self {
            Self::Stored(record) => record.to_descriptor(),
            Self::Plain { value, attrs } => PropertyDescriptor::data(
                value.clone(),
                attrs & ATTR_WRITABLE != 0,
                attrs & ATTR_ENUMERABLE != 0,
                attrs & ATTR_CONFIGURABLE != 0,
            ),
        }
    }
}

/// An exclusive view of one own property that can change its value but not its
/// attributes: that would change a shaped map's shape, so a caller that needs
/// to goes through [`PropertyMap::get_stored_mut`].
pub(crate) enum RecordMut<'a> {
    Stored(&'a mut PropertyRecord),
    Plain { value: &'a mut JsValue, attrs: u8 },
}

impl RecordMut<'_> {
    #[inline]
    fn as_ref(&self) -> RecordRef<'_> {
        match self {
            Self::Stored(record) => RecordRef::Stored(record),
            Self::Plain { value, attrs } => RecordRef::Plain {
                value,
                attrs: *attrs,
            },
        }
    }

    #[inline]
    pub(crate) fn value(&self) -> Option<&JsValue> {
        self.as_ref().value()
    }

    #[inline]
    pub(crate) fn writable(&self) -> Option<bool> {
        self.as_ref().writable()
    }

    #[inline]
    pub(crate) fn is_accessor_descriptor(&self) -> bool {
        self.as_ref().is_accessor_descriptor()
    }

    /// Sets the value of a property that has one (a data property).
    #[inline]
    pub(crate) fn set_value(self, value: JsValue) {
        match self {
            Self::Stored(record) => record.set_value(Some(value)),
            Self::Plain { value: slot, .. } => *slot = value,
        }
    }

    /// See [`PropertyRecord::add_to_writable_number`].
    #[inline]
    pub(crate) fn add_to_writable_number(self, delta: f64) -> Option<(f64, f64)> {
        match self {
            Self::Stored(record) => record.add_to_writable_number(delta),
            Self::Plain { value, attrs } => {
                if attrs & ATTR_WRITABLE == 0 {
                    return None;
                }
                let old = value.as_number()?;
                let new = old + delta;
                *value = JsValue::number(new);
                Some((old, new))
            }
        }
    }

    /// See [`PropertyRecord::overwrite_writable_data`].
    #[inline]
    pub(crate) fn overwrite_writable_data(self, value: &JsValue) -> bool {
        match self {
            Self::Stored(record) => record.overwrite_writable_data(value),
            Self::Plain { value: slot, attrs } => {
                if attrs & ATTR_WRITABLE == 0 {
                    return false;
                }
                *slot = value.clone();
                true
            }
        }
    }
}
