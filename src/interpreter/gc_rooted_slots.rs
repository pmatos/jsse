use super::*;

/// A growable, indexable run of GC-visible values that a native closure can
/// replace in place — the engine's Rooted Slot (see `CONTEXT.md`).
///
/// Pin the container **once** on a long-lived anchor with [`Self::pin_on`] (or
/// root it with `gc_root_value(&slots.root())`), then `push`/`set` freely: the
/// tracer follows the anchor to the container and reads whatever it holds now,
/// so a superseded value is collectable and nothing accumulates the way repeated
/// `pin_native_root` calls do.
///
/// The backing store is a real arena object, not a Rust container: writes go
/// through `ObjectHandle::borrow_mut`, which runs the generational write
/// barrier, so an old anchor that takes on a young value is remembered for the
/// next minor collection. A container the arena does not own would be both
/// untraced and unbarriered.
///
/// Every write goes to the backing's `array_elements` and nowhere else.
/// `trace_object_fields` also visits `properties`, so mirroring a write into an
/// index property would retain every superseded value and silently degrade the
/// slot to accumulate-only pinning. The `length` and index properties that
/// `create_array` seeds go stale after the first `push`; that is harmless
/// because the backing object is never reachable from JavaScript.
///
/// Reads and writes borrow the backing for the duration of one call and never
/// hold it across an allocation; `snapshot` returns an owned `Vec` for that
/// reason.
#[derive(Clone)]
pub(crate) struct RootedSlots(JsValue);

impl RootedSlots {
    pub(crate) fn new(interp: &mut Interpreter, len: usize) -> Self {
        Self(interp.create_array(vec![JsValue::UNDEFINED; len]))
    }

    /// The value to hand to `gc_root_value` or `set_helper_gc_roots`; rooting it
    /// roots every slot.
    pub(crate) fn root(&self) -> JsValue {
        self.0.clone()
    }

    /// Pin the container — not any one value — on `anchor`, so the slots live as
    /// long as `anchor` does. Do this once per anchor.
    pub(crate) fn pin_on(&self, interp: &Interpreter, anchor: &JsValue) {
        interp.pin_native_root(anchor, &self.0);
    }

    pub(crate) fn len(&self, interp: &Interpreter) -> usize {
        interp.with_array_elements(&self.0, |elements| elements.len())
    }

    pub(crate) fn push(&self, interp: &Interpreter, value: JsValue) {
        interp.with_array_elements_mut(&self.0, |elements| elements.push(value));
    }

    /// The stored value, whatever it is: occupancy is the caller's business.
    pub(crate) fn get(&self, interp: &Interpreter, index: usize) -> JsValue {
        interp.with_array_elements(&self.0, |elements| elements[index].clone())
    }

    pub(crate) fn set(&self, interp: &Interpreter, index: usize, value: JsValue) {
        interp.with_array_elements_mut(&self.0, |elements| elements[index] = value);
    }

    pub(crate) fn snapshot(&self, interp: &Interpreter) -> Vec<JsValue> {
        interp.with_array_elements(&self.0, |e| e.to_vec())
    }
}
