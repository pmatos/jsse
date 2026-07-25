# Object arena allocator design

## Goal

Eliminate the system allocation and deallocation performed for every
`Rc<RefCell<JsObjectData>>` while preserving the interpreter's existing object
identity, `RefCell` borrow checks, GC slot reuse, and re-entrant builtin
behaviour.

ECMAScript specifies object behaviour and internal slots, but explicitly leaves
their representation to the implementation. This change is therefore a storage
optimization: `MakeBasicObject`, ordinary objects, and exotic objects retain
the same observable semantics.

## Considered approaches

1. Store `Option<RefCell<JsObjectData>>` directly in the existing chunked
   `ObjectArena`. This has the smallest per-object header, but the remaining
   owned `Rc` handles deliberately span re-entrant `&mut Interpreter` calls.
   Replacing them with raw pointers would be unsafe if GC frees and reuses a
   slot while such a handle is live; replacing all of them with borrows requires
   a broad evaluator rewrite.
2. Use Rust's allocator-aware `Rc`. This preserves the API and lets an arena
   supply the backing memory, but `Rc::new_in` and the allocator API remain
   unstable on the project's stable toolchain.
3. Use a narrow, object-specific reference-counted handle backed by a
   per-interpreter fixed-block arena. This preserves the ownership behaviour of
   the current `Rc`, confines unsafe code to one module, and replaces one system
   allocation per object with one allocation per chunk. This is the selected
   approach.

## Design

`ObjectHandle` is an eight-byte non-null pointer to an `ObjectAllocation`.
`ObjectAllocation` contains a single-threaded strong count, a reference to its
owning storage, and the existing `RefCell<JsObjectData>`. `Clone`, `Deref`, and
`AsRef` provide the small API used by current call sites.

`ObjectStorage` owns fixed-size chunks of uninitialized, correctly aligned
`ObjectAllocation` slots and a free list. Allocation takes a free block or the
next block in the current chunk, initializes it in place, and returns an
`ObjectHandle`. Releasing the last handle drops `JsObjectData` and returns the
block to the free list; backing chunks remain allocated until the interpreter
and any temporarily escaped handles are gone.

Logical GC slots and physical allocation blocks remain separate. GC may remove
an object from its logical slot and reuse the numeric object id while a
temporary handle to the old allocation exists. The old allocation is not
reused until that handle is dropped, matching the current `Rc` behaviour and
preventing ABA access to a newly allocated object.

The storage is per interpreter. This avoids cross-interpreter lifetime coupling,
global synchronization, and memory retention beyond the last related object
handle.

## Safety invariants

- Chunk allocations never move, so every `ObjectHandle` pointer remains stable.
- An initialized block is returned to the storage free list exactly once, after
  its strong count reaches zero and its value has been dropped.
- The embedded storage reference keeps all chunks alive until the last handle
  backed by them is released.
- Free-list blocks are uninitialized and are never dereferenced before the next
  allocation writes a complete `ObjectAllocation`.
- `ObjectHandle` remains single-threaded, like `Rc`, and `RefCell` continues to
  enforce dynamic borrow rules.

## Validation

- Unit-test chunk growth, block reuse, and the case where GC frees a logical
  slot while a cloned handle keeps its physical allocation alive.
- Run formatting, clippy, release build, and release unit/integration tests.
- Run object/proxy/collection test262 subsets because they exercise ordinary
  allocation, re-entrancy, and owned handles, followed by the full suite.
- Compare JetStream splay before and after when the local JetStream checkout is
  available. At minimum, record arena allocation statistics in unit tests so
  the one-chunk-per-many-objects property cannot regress silently.

