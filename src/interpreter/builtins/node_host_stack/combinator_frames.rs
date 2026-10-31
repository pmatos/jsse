//! `at async Promise.all (index N)` frames of `--node` stacks (issue #1508).
//! V8's `at async` walk (see `async_frames.rs`) recognises the element
//! functions of `Promise.all`, `allSettled` and `any`, prints a frame with the
//! element's index and continues from the combinator's result promise; a
//! promise's own resolving function (what `Promise.race` hands to `then`) is
//! followed without a frame. The element functions carry hidden slots, set
//! only while the hooks are installed, so default mode is untouched and the
//! collector traces the result promise through them.

use super::*;

const ELEMENT_PROMISE: PrivateId = PrivateId(u64::MAX - 6);
const ELEMENT_KIND: PrivateId = PrivateId(u64::MAX - 7);
const NAMES: [&str; 3] = ["all", "allSettled", "any"];

pub(super) struct CombinatorHop {
    pub(super) name: &'static str,
    pub(super) index: usize,
    pub(super) next: Option<u64>,
}

impl Interpreter {
    /// Marks `function`, element `index` of the combinator `name`, whose
    /// result promise is `promise`.
    pub(crate) fn node_mark_combinator(
        &mut self,
        function: &JsValue,
        name: &str,
        index: usize,
        promise: &JsValue,
    ) {
        let Some(kind) = NAMES.iter().position(|known| *known == name) else {
            return;
        };
        if !self.node_host.stack.installed {
            return;
        }
        let Some(handle) = function
            .as_object_id()
            .and_then(|id| self.get_object_cell(id).cloned())
        else {
            return;
        };
        {
            let mut object = handle.borrow_mut();
            object.set_hidden_slot(ELEMENT_PROMISE, promise.clone());
            object.set_hidden_slot(ELEMENT_KIND, JsValue::number((index * 4 + kind) as f64));
        }
        self.gc_write_barrier_value(&handle, promise);
    }

    fn combinator_element(&self, handler: Option<&JsValue>) -> Option<CombinatorHop> {
        let object = self.get_object(handler?.as_object_id()?)?;
        let object = object.borrow();
        let code = object.hidden_slot(ELEMENT_KIND)?.as_number()? as usize;
        Some(CombinatorHop {
            name: NAMES.get(code % 4).copied()?,
            index: code / 4,
            next: object.hidden_slot(ELEMENT_PROMISE)?.as_object_id(),
        })
    }

    /// `handler` is the resolving function a pending promise handed out, which
    /// settles the promise it was made for.
    fn resolves_pending_promise(&self, handler: Option<&JsValue>) -> Option<u64> {
        let handler = handler?.as_object_id()?;
        let object = self.get_object(handler)?;
        let promise = object.borrow().gc_native_roots().first()?.as_object_id()?;
        let target = self.get_object(promise)?;
        let target = target.borrow();
        let own = target
            .promise_data()?
            .resolving_functions
            .iter()
            .any(|(resolve, _)| resolve.as_object_id() == Some(handler));
        own.then_some(promise)
    }

    /// What the single reaction of a pending promise leads to when it belongs
    /// to a combinator (V8 tries the fulfill handler, then the reject
    /// handler of `any`, then a promise's own resolving function).
    pub(super) fn combinator_hop(&self, data: &PromiseData) -> Option<CombinatorHop> {
        let fulfill = data.fulfill_reactions.first()?.handler.as_ref();
        if let Some(hop) = self.combinator_element(fulfill) {
            return Some(hop);
        }
        if let [reaction] = data.reject_reactions.as_slice()
            && let Some(hop) = self.combinator_element(reaction.handler.as_ref())
        {
            return Some(hop);
        }
        let next = self.resolves_pending_promise(fulfill)?;
        Some(CombinatorHop {
            name: "",
            index: 0,
            next: Some(next),
        })
    }
}
