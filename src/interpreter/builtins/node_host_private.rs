//! V8's text for failed private-name accesses, under `--node` only (#1544).
//!
//! V8 words the failure by the member it was about: a private method or
//! accessor names the class (`Receiver must be an instance of class A`, and
//! `Receiver must be class A` for a static one), a private field names itself.
//! The engine's `PrivateId` carries neither, so class evaluation notes the
//! name, the class and the shape of each private name here. The table is
//! bounded: the oldest notes go first, and a name without a note is worded
//! as a field.

use super::super::*;
use std::collections::BTreeMap;

const NOTE_CAP: usize = 1 << 15;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Field,
    Member,
    StaticMember,
}

struct Note {
    name: String,
    class: String,
    shape: Shape,
}

#[derive(Default)]
pub(crate) struct PrivateNotes {
    notes: BTreeMap<u64, Note>,
}

#[derive(Clone, Copy)]
pub(crate) enum PrivateAccess {
    Read,
    Write,
}

impl Interpreter {
    /// Create the Private Name `name` of a class body (once per source name)
    /// and note how V8 words a failure on it.
    pub(crate) fn declare_private_name(
        &mut self,
        scope: &mut PrivateNameScope,
        name: &str,
        elem: &ClassElement,
        class: &str,
    ) {
        scope.insert_if_absent(name, || {
            let id = PrivateId(self.next_private_id);
            self.next_private_id += 1;
            if self.node_host_enabled {
                let shape = match elem {
                    ClassElement::Method(m) if m.is_static => Shape::StaticMember,
                    ClassElement::Method(_) => Shape::Member,
                    _ => Shape::Field,
                };
                let class = if class.is_empty() { "anonymous" } else { class };
                let notes = &mut self.node_host.private_notes.notes;
                if notes.len() >= NOTE_CAP {
                    notes.pop_first();
                }
                notes.insert(
                    id.0,
                    Note {
                        name: name.to_string(),
                        class: class.to_string(),
                        shape,
                    },
                );
            }
            id
        });
    }

    /// The TypeError of `receiver.#name` (read) or `receiver.#name = v`
    /// (write) when `receiver` lacks the private element `branded`.
    pub(crate) fn private_access_error(
        &mut self,
        branded: PrivateId,
        name: &str,
        receiver: &JsValue,
        access: PrivateAccess,
    ) -> JsValue {
        let (verb, prep) = match access {
            PrivateAccess::Read => ("read", "from"),
            PrivateAccess::Write => ("write", "to"),
        };
        let declared = format!(
            "Cannot {verb} private member #{name} {prep} an object whose class did not declare it"
        );
        let engine = if receiver.is_object() {
            declared.clone()
        } else {
            format!("Cannot {verb} private member #{name} {prep} a non-object")
        };
        let text = self.v8_message(&engine, |i| {
            let shape = i.node_host.private_notes.notes.get(&branded.0);
            let class = shape.map_or("", |n| n.class.as_str());
            let nullish = receiver.is_nullish();
            let which = if receiver.is_null() {
                "null"
            } else {
                "undefined"
            };
            match shape.map(|n| n.shape) {
                Some(Shape::Member) if nullish => {
                    format!("Cannot read properties of {which} (reading '{class}')")
                }
                Some(Shape::Member) => format!("Receiver must be an instance of class {class}"),
                Some(Shape::StaticMember) => {
                    let shown = if class == "anonymous" { "." } else { class };
                    format!("Receiver must be class {shown}")
                }
                _ if nullish => match access {
                    PrivateAccess::Read => {
                        format!("Cannot read properties of {which} (reading '#{name}')")
                    }
                    PrivateAccess::Write => {
                        format!("Cannot set properties of {which} (setting '#{name}')")
                    }
                },
                _ => declared,
            }
        });
        self.create_type_error(&text)
    }

    /// V8's text for initializing the private name `id` on an object that has
    /// it already.
    pub(super) fn private_twice_text(&self, id: PrivateId, method: bool, engine: &str) -> String {
        let Some(note) = self.node_host.private_notes.notes.get(&id.0) else {
            return engine.to_string();
        };
        if method || note.shape != Shape::Field {
            let class = &note.class;
            format!("Cannot initialize private methods of class {class} twice on the same object")
        } else {
            format!("Cannot initialize #{} twice on the same object", note.name)
        }
    }

    /// `#name in rval` with a non-object `rval`.
    pub(crate) fn private_in_error(&mut self, branded: PrivateId, rval: &JsValue) -> JsValue {
        let engine = "Cannot use 'in' operator to search for a private field without an object";
        let text = self.v8_message(engine, |i| {
            let Some(note) = i.node_host.private_notes.notes.get(&branded.0) else {
                return engine.to_string();
            };
            let key = match note.shape {
                Shape::Member => note.class.clone(),
                _ => format!("#{}", note.name),
            };
            format!(
                "Cannot use 'in' operator to search for '{key}' in {}",
                i.v8_value_text(rval)
            )
        });
        self.create_type_error(&text)
    }
}
