use crate::interpreter::Interpreter;
use crate::interpreter::types::Completion;
use crate::types::JsValue;

impl Interpreter {
    /// Get(superclass, "prototype") of ClassDefinitionEvaluation step 8, which
    /// must be an Object or null.
    pub(crate) fn superclass_prototype(
        &mut self,
        super_id: u64,
        superclass: &JsValue,
    ) -> Completion {
        let proto = match self.get_object_property(super_id, "prototype", superclass) {
            Completion::Normal(v) => v,
            other => return other,
        };
        if !proto.is_object() && !proto.is_null() {
            return Completion::Throw(
                self.create_type_error(
                    "Class extends value does not have valid prototype property",
                ),
            );
        }
        Completion::Normal(proto)
    }

    /// `protoParent` for an evaluated ClassHeritage (§15.7.14 step 8), with
    /// the checks that precede any ClassElement.
    pub(crate) fn class_proto_parent(&mut self, superclass: &JsValue, at: u32) -> Completion {
        if superclass.is_null() {
            return Completion::Normal(JsValue::NULL);
        }
        let Some(super_id) = superclass
            .as_object_id()
            .filter(|_| self.is_constructor(superclass))
        else {
            if at != 0 {
                self.node_host.call_pos = at;
            }
            return Completion::Throw(self.class_extends_error(superclass, &None));
        };
        self.superclass_prototype(super_id, superclass)
    }
}
