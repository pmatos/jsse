# JSSE Engine Context

Domain language for the JSSE JavaScript engine: the modules, concepts, and seams that shape the interpreter and its interfaces.

## Language

**Body**:
A unit of executable ECMAScript syntax — a script, module, or function body — that owns its own IC site map and is the granularity at which inline-cache state is stored.
_Avoid_: function body, script body, code unit.

**IC Site**:
A specific call or property-access location in a Body that can be inline-cached at runtime.
_Avoid_: cache entry, IC slot (when referring to the location rather than the stored value).

**CallSiteId**:
A dense identifier assigned to a call IC site within a single Body.
_Avoid_: call IC index, call cache id.

**PropSiteId**:
A dense identifier assigned to a property-access IC site within a single Body.
_Avoid_: prop IC index, property cache id.

**BodyIcInfo**:
Metadata describing the number and kinds of IC sites in a Body, used to size the runtime IC store without coupling the AST to the runtime slot types.
_Avoid_: cache header, IC metadata.

**BodyIcStore**:
The runtime cache of IC slot values for a Body, keyed by the Body's identity and shared by all closures of that Body.
_Avoid_: cache table, IC map.

**Seam**:
A place where one module's interface ends and another's begins. In JSSE, the seams between the AST, the inline-cache system, and the interpreter are intentionally narrow: the AST carries site identifiers, the runtime carries slot values, and the interpreter maps one to the other.
_Avoid_: boundary, layer.
