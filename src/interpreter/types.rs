use crate::ast::*;
mod buffer_data;
mod function_props;
use crate::interpreter::Atom;
use crate::interpreter::binding_map::{BindingMap, BindingName};
use crate::interpreter::generator_transform::{
    GeneratorStateMachine, LoopControlTarget, SentValueBinding,
};
use crate::interpreter::helpers::{same_value, to_number};
use crate::interpreter::key_intern::intern_js_key;
use crate::interpreter::keyed_entries::KeyedEntries;
use crate::interpreter::property_map::PropertyRecord;
pub(crate) use crate::interpreter::weak_slots::FinalizationRegistryData;
use crate::interpreter::{ElementVec, PropertyMap};
use crate::types::{JsPropertyKey, JsString, JsValue, PropertyKeyLike, number_ops};
pub(crate) use buffer_data::{BufferData, ForeignBytes};
use rustc_hash::FxHashMap;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock, RwLock};

/// Process-global counter for `JsObjectData.shape_id`; starts at 1 because `0`
/// is the invalid sentinel (#71, inline caching).
static NEXT_SHAPE_ID: AtomicU64 = AtomicU64::new(1);

/// Draws a fresh, globally-unique shape id (one relaxed fetch-add).
#[inline]
pub(crate) fn fresh_shape_id() -> u64 {
    NEXT_SHAPE_ID.fetch_add(1, Ordering::Relaxed)
}

static NEXT_SAB_ID: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_sab_id() -> u64 {
    NEXT_SAB_ID.fetch_add(1, Ordering::Relaxed)
}

const SPLITMIX64_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;
const MATH_RANDOM_FALLBACK_SEED: u64 = 0xA076_1D64_78BD_642F;
static MATH_RANDOM_PROCESS_SEED: OnceLock<u64> = OnceLock::new();
static NEXT_MATH_RANDOM_REALM: AtomicU64 = AtomicU64::new(0);

#[inline]
fn splitmix64_mix(mut value: u64) -> u64 {
    value = (value ^ (value >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn new_math_random_state() -> u64 {
    let process_seed = *MATH_RANDOM_PROCESS_SEED
        .get_or_init(|| getrandom::u64().unwrap_or(MATH_RANDOM_FALLBACK_SEED));
    let realm_index = NEXT_MATH_RANDOM_REALM.fetch_add(1, Ordering::Relaxed);
    process_seed ^ splitmix64_mix(realm_index)
}

pub(crate) struct SharedBufferInner {
    words: RwLock<Vec<u64>>,
    len: AtomicUsize,
    pub id: u64,
}

impl SharedBufferInner {
    pub(crate) fn new(data: Vec<u8>, id: u64) -> Self {
        let len = data.len();
        let mut words = vec![0u64; len.div_ceil(8)];
        for (idx, byte) in data.into_iter().enumerate() {
            let word_idx = idx / 8;
            let shift = (idx % 8) * 8;
            words[word_idx] |= (byte as u64) << shift;
        }
        Self {
            words: RwLock::new(words),
            len: AtomicUsize::new(len),
            id,
        }
    }

    #[allow(clippy::len_without_is_empty)]
    pub(crate) fn len(&self) -> usize {
        self.len.load(Ordering::SeqCst)
    }

    fn byte_slice(words: &[u64], len: usize) -> &[u8] {
        let bytes =
            unsafe { std::slice::from_raw_parts(words.as_ptr() as *const u8, words.len() * 8) };
        &bytes[..len]
    }

    fn byte_slice_mut(words: &mut [u64], len: usize) -> &mut [u8] {
        let bytes = unsafe {
            std::slice::from_raw_parts_mut(words.as_mut_ptr() as *mut u8, words.len() * 8)
        };
        &mut bytes[..len]
    }

    pub(crate) fn with_read<R>(&self, f: impl FnOnce(&[u8]) -> R) -> R {
        let len = self.len();
        let guard = self.words.read().unwrap();
        f(Self::byte_slice(&guard, len))
    }

    pub(crate) fn with_write<R>(&self, f: impl FnOnce(&mut [u8]) -> R) -> R {
        let len = self.len();
        let mut guard = self.words.write().unwrap();
        f(Self::byte_slice_mut(&mut guard, len))
    }

    pub(crate) fn resize(&self, new_len: usize, value: u8) {
        let old_len = self.len();
        let mut guard = self.words.write().unwrap();
        let new_words_len = new_len.div_ceil(8);
        if guard.len() < new_words_len {
            guard.resize(new_words_len, 0);
        }
        {
            let old_words_len = old_len.div_ceil(8);
            let new_words = &mut guard[..new_words_len.max(old_words_len)];
            let bytes = unsafe {
                std::slice::from_raw_parts_mut(
                    new_words.as_mut_ptr() as *mut u8,
                    new_words.len() * 8,
                )
            };
            if new_len > old_len {
                bytes[old_len..new_len].fill(value);
            } else {
                bytes[new_len..old_len].fill(0);
            }
        }
        if guard.len() > new_words_len {
            guard.truncate(new_words_len);
        }
        self.len.store(new_len, Ordering::SeqCst);
    }

    pub(crate) fn to_vec(&self) -> Vec<u8> {
        self.with_read(|bytes| bytes.to_vec())
    }

    pub(crate) fn with_atomic_ptr<T, R>(
        &self,
        offset: usize,
        size: usize,
        f: impl FnOnce(*mut T) -> R,
    ) -> Option<R> {
        if offset + size > self.len() {
            return None;
        }
        let mut guard = self.words.write().unwrap();
        let base = guard.as_mut_ptr() as *mut u8;
        Some(f(unsafe { base.add(offset) as *mut T }))
    }
}

impl std::fmt::Debug for SharedBufferInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedBufferInner")
            .field("id", &self.id)
            .field("len", &self.len())
            .finish()
    }
}

#[derive(Debug)]
pub(crate) enum Completion {
    Normal(JsValue),
    Return(JsValue),
    Throw(JsValue),
    Break(Option<String>, Option<JsValue>),
    Continue(Option<String>, Option<JsValue>),
    Yield(JsValue),
    TailCall {
        func: JsValue,
        this: JsValue,
        args: Vec<JsValue>,
    },
    /// A host-requested process exit (`__host_exit`, issue #242). Propagates
    /// like `Throw` — it is abrupt and unwinds the stack — but is structurally
    /// uncatchable: it does not match the `Throw` arm any `catch`, `finally`,
    /// disposer, iterator `return()`, or Promise reaction inspects, so no user
    /// code can ever consume it. Only the node host floor produces it (behind
    /// `--node`), so it never arises with the floor off.
    Exit(i32),
    Empty,
}

impl Completion {
    pub(crate) fn is_abrupt(&self) -> bool {
        !matches!(self, Completion::Normal(_) | Completion::Empty)
    }
    pub(crate) fn update_empty(self, val: JsValue) -> Completion {
        match self {
            Completion::Empty => Completion::Normal(val),
            Completion::Break(label, None) => Completion::Break(label, Some(val)),
            Completion::Continue(label, None) => Completion::Continue(label, Some(val)),
            other => other,
        }
    }
    pub(crate) fn value_or(&self, default: JsValue) -> JsValue {
        match self {
            Completion::Normal(v) => v.clone(),
            Completion::Empty => default,
            _ => default,
        }
    }
    /// Every `JsValue` this completion carries as a payload, for GC-rooting
    /// across a call (`finally`, `IteratorClose`'s `return()`) that can run
    /// user code and trigger a collection while the completion is still a
    /// bare Rust local. Mirrors `DisposeCursor::for_each_value`'s match.
    pub(crate) fn root_payload(&self, mut f: impl FnMut(&JsValue)) {
        match self {
            Completion::Normal(v)
            | Completion::Return(v)
            | Completion::Throw(v)
            | Completion::Yield(v) => f(v),
            Completion::Break(_, Some(v)) | Completion::Continue(_, Some(v)) => f(v),
            Completion::TailCall { func, this, args } => {
                f(func);
                f(this);
                for a in args {
                    f(a);
                }
            }
            _ => {}
        }
    }
}

/// Convert a fallible or control-flow-carrying value into the interpreter's
/// canonical `Result<success, abrupt Completion>`: `Ok` is the value to bind,
/// `Err` is a [`Completion`] that must early-return from the caller. Implemented
/// for the three error-propagation source shapes that `-> Completion` functions
/// consume, so [`propagate!`] can unwrap any of them with one spelling.
pub(crate) trait IntoAbrupt {
    /// The success payload — fixed per source shape, hence an associated type
    /// (functionally determined by `Self`) rather than a type parameter.
    type Ok;
    fn into_abrupt(self) -> Result<Self::Ok, Completion>;
}

impl IntoAbrupt for Completion {
    type Ok = JsValue;
    fn into_abrupt(self) -> Result<JsValue, Completion> {
        match self {
            Completion::Normal(v) => Ok(v),
            // Every other variant early-returns — including `Empty`, which is
            // not abrupt but was propagated verbatim by the `other => return
            // other` sites this seam replaces.
            other => Err(other),
        }
    }
}

impl<T> IntoAbrupt for Result<T, JsValue> {
    type Ok = T;
    fn into_abrupt(self) -> Result<T, Completion> {
        // A Rust `Err(JsValue)` is a JS throw.
        self.map_err(Completion::Throw)
    }
}

impl<T> IntoAbrupt for Result<T, Completion> {
    type Ok = T;
    fn into_abrupt(self) -> Result<T, Completion> {
        // The `Err` already carries the completion — pass it through.
        self
    }
}

/// Unwrap the success value of a [`Completion`], `Result<T, JsValue>`, or
/// `Result<T, Completion>` (via [`IntoAbrupt`]), early-returning the abrupt
/// completion out of the enclosing `-> Completion` function otherwise. Replaces
/// the hand-rolled two-line `match` propagation heads across the interpreter.
macro_rules! propagate {
    ($expr:expr) => {
        match $crate::interpreter::IntoAbrupt::into_abrupt($expr) {
            Ok(v) => v,
            Err(c) => return c,
        }
    };
}
pub(crate) use propagate;

pub(crate) enum GeneratorResumeKind {
    Next,
    Return(JsValue),
    Throw(JsValue),
}

pub(crate) struct GeneratorContext {
    pub(crate) target_yield: usize,
    pub(crate) current_yield: usize,
    /// Values sent to previous yields (index k = value passed to next() after yield k)
    pub(crate) prev_sent_values: Vec<JsValue>,
    pub(crate) resume_kind: GeneratorResumeKind,
}

#[derive(Debug, Clone)]
pub(crate) enum GeneratorExecutionState {
    #[allow(dead_code)]
    SuspendedStart,
    SuspendedYield {
        target_yield: usize,
        /// Values sent to yields 0..target_yield-1
        prev_sent: Vec<JsValue>,
    },
    Executing,
    Completed,
}

#[derive(Debug, Clone)]
pub(crate) enum StateMachineExecutionState {
    SuspendedStart,
    SuspendedAtState { state_id: usize },
    Executing,
    Completed,
}

/// A generator's lifecycle (sec-generatorstart, sec-generatorresume).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BytecodeGeneratorStatus {
    SuspendedStart,
    SuspendedYield,
    Executing,
    Completed,
}

/// What every resumption of one bytecode generator shares: the compiled body,
/// the body's environment and the `Body` whose inline-cache store it runs in.
#[derive(Debug)]
pub(crate) struct BytecodeGeneratorCode {
    pub(crate) chunk: Rc<crate::interpreter::bytecode::chunk::Chunk>,
    pub(crate) body: Body,
    /// The literal's call-frame facts, whose handle caches the Body's
    /// inline-cache store.
    pub(crate) frame: Rc<crate::interpreter::literal_frame::LiteralFrame>,
    pub(crate) func_env: EnvRef,
    #[cfg(feature = "perf-counters")]
    pub(crate) perf_key: crate::interpreter::perf_counters::BodyKey,
}

/// A sync generator whose body runs as a bytecode chunk and suspends at
/// `Op::Yield`, keeping its frame here between resumptions. `frame` is `None`
/// before the first start and while the body runs (the VM owns it then).
#[derive(Debug, Clone)]
pub(crate) struct BytecodeGenerator {
    pub(crate) code: Rc<BytecodeGeneratorCode>,
    pub(crate) status: BytecodeGeneratorStatus,
    pub(crate) frame: Option<Box<crate::interpreter::bytecode::vm::GeneratorFrame>>,
    /// An async generator: its requests queue in the scheduler.
    pub(crate) is_async: bool,
}

/// A Completion Record intercepted by a running `finally`, owned by the
/// `TryContextInfo` whose finalizer is running it (issue #719). ECMAScript
/// carries exactly one Completion Record at a time, so this is a tagged union
/// rather than three independently optional fields: a context can be
/// restoring a throw, a return, or a loop-control jump, never more than one.
#[derive(Debug, Clone)]
pub(crate) enum PendingCompletion {
    Return(JsValue),
    Throw(JsValue),
    LoopControl(LoopControlTarget),
}

#[derive(Debug, Clone)]
pub(crate) struct TryContextInfo {
    pub catch_state: Option<usize>,
    pub finally_state: Option<usize>,
    pub _after_state: usize,
    pub entered_catch: bool,
    pub entered_finally: bool,
    /// The completion this context's finalizer is running on behalf of.
    /// Living on the context means a jump or throw that leaves the finalizer
    /// discards it together with the context, and a nested finalizer cannot
    /// overwrite it.
    pub pending_completion: Option<PendingCompletion>,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingForOfUnwind {
    /// State reached when the intervening handler completes normally. Until
    /// then, a replacement abrupt completion must keep unwinding retained
    /// enclosing loops.
    pub clear_at_state: Option<usize>,
}

pub(crate) struct AsyncFunctionState {
    pub state_machine: Rc<GeneratorStateMachine>,
    pub func_env: EnvRef,
    pub is_strict: bool,
    pub current_state: usize,
    pub try_stack: Vec<TryContextInfo>,
    pub pending_binding: Option<SentValueBinding>,
    pub pending_for_of_unwind: Option<PendingForOfUnwind>,
    pub resolvers: PromiseResolvers,
    pub for_of_stack: Vec<ForOfLoopState>,
    pub module_path: Option<super::ModuleKey>,
    /// Set while the function-level disposal is parked at an `Await`; the
    /// resumption feeds its outcome to this cursor instead of the body.
    pub pending_dispose: Option<super::PendingDispose>,
    /// Lexical scope stack for lowered blocks, loop bodies, clause bodies,
    /// catch bindings, and `for`-head per-iteration frames. Frames created by
    /// `EnterScope` additionally own `await using` resources and are disposed
    /// when control crosses them.
    pub scope_stack: Vec<ScopeFrame>,
    /// Realm of the function, restored around every resumption: a reaction job
    /// runs in whichever realm drained the queue.
    pub realm_id: usize,
    /// `--node` `at async` frames: the function (0: a module) and its `await`.
    pub func_obj_id: u64,
    pub await_pos: u32,
}

/// One lowered lexical environment, innermost last. `EnterScope`/`ExitScope`
/// use the same frame shape for `await using` blocks so abrupt completion can
/// order scope disposal against `finally` handlers and nested for-of loops.
#[derive(Clone)]
pub(crate) struct ScopeFrame {
    pub(crate) env: EnvRef,
    /// Depth of the driver's try stack when the scope was entered, so an
    /// abrupt completion can tell a `finally` lexically inside the scope from
    /// one outside it, exactly as `ForOfLoopState::try_depth` does.
    pub(crate) try_depth: usize,
    /// Depth of the driver's for-of stack when the scope was entered, used to
    /// order this frame's disposal against a for-of loop nested more deeply
    /// (which must close first) or more shallowly (which closes after).
    pub(crate) for_of_depth: usize,
}

#[derive(Clone)]
pub(crate) struct ForOfLoopState {
    pub(crate) iter_var: Rc<str>,
    /// The labels attached to this iteration statement. `LoopContinues` uses
    /// this set to decide whether a labelled continue begins its next iteration.
    pub(crate) label_set: Rc<[String]>,
    pub(crate) head_state: usize,
    pub(crate) after_state: usize,
    /// Depth of the driver's try stack when the loop began, so an abrupt
    /// completion can tell a `finally` lexically inside the loop (which runs
    /// before IteratorClose) from one outside it (which runs after).
    /// The async-function and generator drivers use the same boundary while
    /// unwinding their saved loop state.
    pub(crate) try_depth: usize,
    /// The LexicalEnvironment active when ForIn/OfBodyEvaluation began.
    pub(crate) outer_env: EnvRef,
    /// The current lexical head's per-iteration environment, when any.
    pub(crate) iteration_env: Option<EnvRef>,
    /// `for await`: closing the iterator is AsyncIteratorClose, which Awaits
    /// the result of `return()`.
    pub(crate) is_await: bool,
    /// AsyncIteratorClose already called `return()` and is parked at its
    /// Await; re-entering the unwind must not call it again.
    pub(crate) iterator_closed: bool,
}

impl ForOfLoopState {
    pub(crate) fn effective_env(&self) -> &EnvRef {
        self.iteration_env.as_ref().unwrap_or(&self.outer_env)
    }

    pub(crate) fn matches_continue_target(&self, target: Option<&str>) -> bool {
        target.is_none_or(|target| self.label_set.iter().any(|label| label == target))
    }

    pub(crate) fn same_as(&self, other: &ForOfLoopState) -> bool {
        let ForOfLoopState {
            iter_var,
            label_set,
            head_state,
            after_state,
            try_depth,
            outer_env,
            iteration_env,
            is_await,
            iterator_closed,
        } = self;
        *head_state == other.head_state
            && *after_state == other.after_state
            && *try_depth == other.try_depth
            && *is_await == other.is_await
            && *iterator_closed == other.iterator_closed
            && Rc::ptr_eq(outer_env, &other.outer_env)
            && match (iteration_env, &other.iteration_env) {
                (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && (Rc::ptr_eq(iter_var, &other.iter_var) || *iter_var == other.iter_var)
            && (Rc::ptr_eq(label_set, &other.label_set) || *label_set == other.label_set)
    }
}

impl ScopeFrame {
    pub(crate) fn same_as(&self, other: &ScopeFrame) -> bool {
        let ScopeFrame {
            env,
            try_depth,
            for_of_depth,
        } = self;
        *try_depth == other.try_depth
            && *for_of_depth == other.for_of_depth
            && Rc::ptr_eq(env, &other.env)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DelegatedIteratorInfo {
    pub iterator: JsValue,
    pub next_method: JsValue,
    pub resume_state: usize,
    pub sent_value_binding: Option<SentValueBinding>,
}

pub(crate) type EnvRef = Rc<RefCell<Environment>>;

/// Function ids of a realm's intrinsic iterator `next` methods, compared by
/// identity to skip the iterator-result object. They are GC roots so a
/// collected-and-recycled id can never match an unrelated function.
#[derive(Clone, Copy, Default)]
pub(crate) struct IteratorNextIntrinsics {
    pub(crate) array: Option<u64>,
    pub(crate) string: Option<u64>,
    pub(crate) map: Option<u64>,
    pub(crate) set: Option<u64>,
    pub(crate) generator: Option<u64>,
}

impl IteratorNextIntrinsics {
    fn roots(&self) -> impl Iterator<Item = u64> {
        [self.array, self.string, self.map, self.set, self.generator]
            .into_iter()
            .flatten()
    }
}

pub(crate) struct Realm {
    pub(crate) global_env: EnvRef,
    pub(crate) math_random_state: u64,
    pub(crate) global_object: Option<u64>,
    pub(crate) throw_type_error: Option<JsValue>,
    /// %Array.prototype.values%, captured when the realm's Array prototype is
    /// built; the `@@iterator` value of every arguments object.
    pub(crate) array_prototype_values: Option<JsValue>,
    /// Intrinsics whose identity lets promise operations skip function-object
    /// round trips nobody can observe.
    pub(crate) promise_constructor: Option<JsValue>,
    pub(crate) promise_then: Option<JsValue>,
    pub(crate) promise_species_getter: Option<JsValue>,
    pub(crate) iterator_next_intrinsics: IteratorNextIntrinsics,
    pub(crate) template_cache: FxHashMap<u64, u64>,
    pub(crate) builtin_eval_id: Option<u64>,
    pub(crate) object_prototype: Option<u64>,
    pub(crate) array_prototype: Option<u64>,
    pub(crate) string_prototype: Option<u64>,
    pub(crate) number_prototype: Option<u64>,
    pub(crate) boolean_prototype: Option<u64>,
    pub(crate) regexp_prototype: Option<u64>,
    pub(crate) builtin_regexp_exec_id: Option<u64>,
    pub(crate) regexp_string_iterator_prototype: Option<u64>,
    pub(crate) iterator_prototype: Option<u64>,
    pub(crate) array_iterator_prototype: Option<u64>,
    pub(crate) for_in_iterator_prototype: Option<u64>,
    pub(crate) string_iterator_prototype: Option<u64>,
    pub(crate) map_prototype: Option<u64>,
    pub(crate) map_iterator_prototype: Option<u64>,
    pub(crate) set_prototype: Option<u64>,
    pub(crate) set_iterator_prototype: Option<u64>,
    pub(crate) weakmap_prototype: Option<u64>,
    pub(crate) weakset_prototype: Option<u64>,
    pub(crate) weakref_prototype: Option<u64>,
    pub(crate) finalization_registry_prototype: Option<u64>,
    pub(crate) date_prototype: Option<u64>,
    pub(crate) generator_prototype: Option<u64>,
    pub(crate) function_prototype: Option<u64>,
    pub(crate) generator_function_prototype: Option<u64>,
    pub(crate) async_iterator_prototype: Option<u64>,
    pub(crate) async_generator_prototype: Option<u64>,
    pub(crate) async_generator_function_prototype: Option<u64>,
    pub(crate) async_function_prototype: Option<u64>,
    pub(crate) bigint_prototype: Option<u64>,
    pub(crate) symbol_prototype: Option<u64>,
    pub(crate) arraybuffer_prototype: Option<u64>,
    pub(crate) shared_arraybuffer_prototype: Option<u64>,
    pub(crate) typed_array_prototype: Option<u64>,
    pub(crate) typed_array_constructor: Option<JsValue>,
    /// `%Array%` of this realm; `Array.of` compares its `this` against it.
    pub(crate) array_constructor: Option<JsValue>,
    pub(crate) int8array_prototype: Option<u64>,
    pub(crate) uint8array_prototype: Option<u64>,
    pub(crate) uint8clampedarray_prototype: Option<u64>,
    pub(crate) int16array_prototype: Option<u64>,
    pub(crate) uint16array_prototype: Option<u64>,
    pub(crate) int32array_prototype: Option<u64>,
    pub(crate) uint32array_prototype: Option<u64>,
    pub(crate) float16array_prototype: Option<u64>,
    pub(crate) float32array_prototype: Option<u64>,
    pub(crate) float64array_prototype: Option<u64>,
    pub(crate) bigint64array_prototype: Option<u64>,
    pub(crate) biguint64array_prototype: Option<u64>,
    pub(crate) dataview_prototype: Option<u64>,
    pub(crate) promise_prototype: Option<u64>,
    pub(crate) aggregate_error_prototype: Option<u64>,
    pub(crate) temporal_duration_prototype: Option<u64>,
    pub(crate) temporal_instant_prototype: Option<u64>,
    pub(crate) temporal_plain_date_prototype: Option<u64>,
    pub(crate) temporal_plain_time_prototype: Option<u64>,
    pub(crate) temporal_plain_date_time_prototype: Option<u64>,
    pub(crate) temporal_plain_year_month_prototype: Option<u64>,
    pub(crate) temporal_plain_month_day_prototype: Option<u64>,
    pub(crate) temporal_zoned_date_time_prototype: Option<u64>,
    pub(crate) intl_collator_prototype: Option<u64>,
    pub(crate) intl_number_format_prototype: Option<u64>,
    pub(crate) intl_date_time_format_prototype: Option<u64>,
    pub(crate) intl_plural_rules_prototype: Option<u64>,
    pub(crate) intl_relative_time_format_prototype: Option<u64>,
    pub(crate) intl_list_format_prototype: Option<u64>,
    pub(crate) intl_segmenter_prototype: Option<u64>,
    pub(crate) intl_segments_prototype: Option<u64>,
    pub(crate) intl_segment_iterator_prototype: Option<u64>,
    pub(crate) intl_display_names_prototype: Option<u64>,
    pub(crate) intl_locale_prototype: Option<u64>,
    pub(crate) intl_duration_format_prototype: Option<u64>,
    pub(crate) intl_number_format_ctor: Option<JsValue>,
    pub(crate) intl_date_time_format_ctor: Option<JsValue>,
    pub(crate) intl_duration_format_ctor: Option<JsValue>,
    /// ECMA-402 %Intl%.[[FallbackSymbol]]: a fresh per-realm Symbol with
    /// description "IntlLegacyConstructedSymbol", minted lazily on first use.
    pub(crate) intl_fallback_symbol: Option<JsValue>,
    /// %AbstractModuleSource% constructor (source-phase imports proposal),
    /// minted lazily on first use. Its `prototype` is the [[Prototype]] of
    /// every host-provided Module Source object.
    pub(crate) abstract_module_source_ctor: Option<JsValue>,
    pub(crate) error_prototype: Option<u64>,
    pub(crate) type_error_prototype: Option<u64>,
    pub(crate) range_error_prototype: Option<u64>,
    pub(crate) reference_error_prototype: Option<u64>,
    pub(crate) syntax_error_prototype: Option<u64>,
    pub(crate) uri_error_prototype: Option<u64>,
    pub(crate) eval_error_prototype: Option<u64>,
    pub(crate) disposable_stack_prototype: Option<u64>,
    pub(crate) async_disposable_stack_prototype: Option<u64>,
    pub(crate) suppressed_error_prototype: Option<u64>,
    pub(crate) shadow_realm_prototype: Option<u64>,
    pub(crate) object_prototype_tostring: Option<JsValue>,
    pub(crate) sloppy_caller_getter: Option<JsValue>,
    pub(crate) sloppy_arguments_getter: Option<JsValue>,
    pub(crate) iterator_helper_prototype: Option<u64>,
}

impl Realm {
    pub(crate) fn new(global_env: EnvRef) -> Self {
        Self {
            global_env,
            math_random_state: new_math_random_state(),
            global_object: None,
            throw_type_error: None,
            array_prototype_values: None,
            promise_constructor: None,
            promise_then: None,
            promise_species_getter: None,
            iterator_next_intrinsics: IteratorNextIntrinsics::default(),
            template_cache: FxHashMap::default(),
            builtin_eval_id: None,
            object_prototype: None,
            array_prototype: None,
            string_prototype: None,
            number_prototype: None,
            boolean_prototype: None,
            regexp_prototype: None,
            builtin_regexp_exec_id: None,
            regexp_string_iterator_prototype: None,
            iterator_prototype: None,
            array_iterator_prototype: None,
            for_in_iterator_prototype: None,
            string_iterator_prototype: None,
            map_prototype: None,
            map_iterator_prototype: None,
            set_prototype: None,
            set_iterator_prototype: None,
            weakmap_prototype: None,
            weakset_prototype: None,
            weakref_prototype: None,
            finalization_registry_prototype: None,
            date_prototype: None,
            generator_prototype: None,
            function_prototype: None,
            generator_function_prototype: None,
            async_iterator_prototype: None,
            async_generator_prototype: None,
            async_generator_function_prototype: None,
            async_function_prototype: None,
            bigint_prototype: None,
            symbol_prototype: None,
            arraybuffer_prototype: None,
            shared_arraybuffer_prototype: None,
            typed_array_prototype: None,
            typed_array_constructor: None,
            array_constructor: None,
            int8array_prototype: None,
            uint8array_prototype: None,
            uint8clampedarray_prototype: None,
            int16array_prototype: None,
            uint16array_prototype: None,
            int32array_prototype: None,
            uint32array_prototype: None,
            float16array_prototype: None,
            float32array_prototype: None,
            float64array_prototype: None,
            bigint64array_prototype: None,
            biguint64array_prototype: None,
            dataview_prototype: None,
            promise_prototype: None,
            aggregate_error_prototype: None,
            temporal_duration_prototype: None,
            temporal_instant_prototype: None,
            temporal_plain_date_prototype: None,
            temporal_plain_time_prototype: None,
            temporal_plain_date_time_prototype: None,
            temporal_plain_year_month_prototype: None,
            temporal_plain_month_day_prototype: None,
            temporal_zoned_date_time_prototype: None,
            intl_collator_prototype: None,
            intl_number_format_prototype: None,
            intl_date_time_format_prototype: None,
            intl_plural_rules_prototype: None,
            intl_relative_time_format_prototype: None,
            intl_list_format_prototype: None,
            intl_segmenter_prototype: None,
            intl_segments_prototype: None,
            intl_segment_iterator_prototype: None,
            intl_display_names_prototype: None,
            intl_locale_prototype: None,
            intl_duration_format_prototype: None,
            intl_number_format_ctor: None,
            intl_date_time_format_ctor: None,
            intl_duration_format_ctor: None,
            intl_fallback_symbol: None,
            abstract_module_source_ctor: None,
            error_prototype: None,
            type_error_prototype: None,
            range_error_prototype: None,
            reference_error_prototype: None,
            syntax_error_prototype: None,
            uri_error_prototype: None,
            eval_error_prototype: None,
            disposable_stack_prototype: None,
            async_disposable_stack_prototype: None,
            suppressed_error_prototype: None,
            shadow_realm_prototype: None,
            object_prototype_tostring: None,
            sloppy_caller_getter: None,
            sloppy_arguments_getter: None,
            iterator_helper_prototype: None,
        }
    }

    pub(crate) fn math_random(&mut self) -> f64 {
        self.math_random_state = self.math_random_state.wrapping_add(SPLITMIX64_GAMMA);
        let random_bits = splitmix64_mix(self.math_random_state) >> 11;
        random_bits as f64 / 9_007_199_254_740_992.0
    }

    pub(crate) fn collect_roots(&self, worklist: &mut Vec<u64>, seen_envs: &mut EnvVisits) {
        super::Interpreter::collect_env_roots(&self.global_env, worklist, seen_envs);
        if let Some(id) = self.global_object {
            worklist.push(id);
        }
        for proto in [
            self.object_prototype,
            self.array_prototype,
            self.string_prototype,
            self.number_prototype,
            self.boolean_prototype,
            self.regexp_prototype,
            self.regexp_string_iterator_prototype,
            self.iterator_prototype,
            self.array_iterator_prototype,
            self.for_in_iterator_prototype,
            self.string_iterator_prototype,
            self.map_prototype,
            self.map_iterator_prototype,
            self.set_prototype,
            self.set_iterator_prototype,
            self.date_prototype,
            self.generator_prototype,
            self.function_prototype,
            self.generator_function_prototype,
            self.async_iterator_prototype,
            self.async_generator_prototype,
            self.async_generator_function_prototype,
            self.async_function_prototype,
            self.weakmap_prototype,
            self.weakset_prototype,
            self.weakref_prototype,
            self.finalization_registry_prototype,
            self.bigint_prototype,
            self.symbol_prototype,
            self.arraybuffer_prototype,
            self.shared_arraybuffer_prototype,
            self.typed_array_prototype,
            self.int8array_prototype,
            self.uint8array_prototype,
            self.uint8clampedarray_prototype,
            self.int16array_prototype,
            self.uint16array_prototype,
            self.int32array_prototype,
            self.uint32array_prototype,
            self.float16array_prototype,
            self.float32array_prototype,
            self.float64array_prototype,
            self.bigint64array_prototype,
            self.biguint64array_prototype,
            self.dataview_prototype,
            self.promise_prototype,
            self.aggregate_error_prototype,
            self.temporal_duration_prototype,
            self.temporal_instant_prototype,
            self.temporal_plain_date_prototype,
            self.temporal_plain_time_prototype,
            self.temporal_plain_date_time_prototype,
            self.temporal_plain_year_month_prototype,
            self.temporal_plain_month_day_prototype,
            self.temporal_zoned_date_time_prototype,
            self.intl_locale_prototype,
            self.intl_collator_prototype,
            self.intl_number_format_prototype,
            self.intl_plural_rules_prototype,
            self.intl_list_format_prototype,
            self.intl_segmenter_prototype,
            self.intl_relative_time_format_prototype,
            self.intl_display_names_prototype,
            self.intl_duration_format_prototype,
            self.intl_date_time_format_prototype,
            self.intl_segments_prototype,
            self.intl_segment_iterator_prototype,
            self.error_prototype,
            self.type_error_prototype,
            self.range_error_prototype,
            self.reference_error_prototype,
            self.syntax_error_prototype,
            self.uri_error_prototype,
            self.eval_error_prototype,
            self.disposable_stack_prototype,
            self.async_disposable_stack_prototype,
            self.suppressed_error_prototype,
            self.shadow_realm_prototype,
            self.iterator_helper_prototype,
        ]
        .into_iter()
        .flatten()
        {
            worklist.push(proto);
        }
        for ctor in [
            &self.intl_number_format_ctor,
            &self.intl_date_time_format_ctor,
            &self.intl_duration_format_ctor,
            &self.typed_array_constructor,
            &self.array_constructor,
            &self.abstract_module_source_ctor,
        ] {
            if let Some(id) = ctor.as_ref().and_then(JsValue::as_object_id) {
                worklist.push(id);
            }
        }
        if let Some(id) = self
            .throw_type_error
            .as_ref()
            .and_then(JsValue::as_object_id)
        {
            worklist.push(id);
        }
        if let Some(id) = self
            .array_prototype_values
            .as_ref()
            .and_then(JsValue::as_object_id)
        {
            worklist.push(id);
        }
        worklist.extend(self.iterator_next_intrinsics.roots());
        for intrinsic in [
            &self.promise_constructor,
            &self.promise_then,
            &self.promise_species_getter,
        ] {
            if let Some(id) = intrinsic.as_ref().and_then(JsValue::as_object_id) {
                worklist.push(id);
            }
        }
        if let Some(id) = self
            .object_prototype_tostring
            .as_ref()
            .and_then(JsValue::as_object_id)
        {
            worklist.push(id);
        }
        if let Some(id) = self
            .sloppy_caller_getter
            .as_ref()
            .and_then(JsValue::as_object_id)
        {
            worklist.push(id);
        }
        if let Some(id) = self
            .sloppy_arguments_getter
            .as_ref()
            .and_then(JsValue::as_object_id)
        {
            worklist.push(id);
        }
        for &obj_id in self.template_cache.values() {
            worklist.push(obj_id);
        }
    }
}

pub(crate) struct Environment {
    // Binding names come from script source. Keep randomized hashing here so
    // deterministic collision sets cannot flood declaration and lookup paths.
    pub(crate) bindings: BindingMap,
    /// §9.1.1.3 [[ThisValue]]/[[ThisBindingStatus]] of a function (or the
    /// global/module) scope. Kept out of `bindings` so a call binds `this`
    /// without hashing or allocating a name.
    ///
    /// Private: every write goes through a method that clears the clean mark.
    this_binding: Option<ThisBinding>,
    pub(crate) parent: Option<EnvRef>,
    pub strict: bool,
    pub(crate) is_function_scope: bool,
    pub(crate) is_arrow_scope: bool,
    pub(crate) with_object: Option<WithObject>,
    dispose_stack: Option<Vec<DisposableResource>>,
    /// Slab id of the realm's global object. The canonical identity reference.
    /// External callers reach the global object via `Interpreter::env_*`
    /// wrappers in `env_helpers.rs`, never by holding a borrow here.
    pub(crate) global_object_id: Option<u64>,
    // Annex B.3.3: names registered for block-level function var hoisting
    pub(crate) annexb_function_names: Option<Vec<String>>,
    pub(crate) class_private_names: Option<Rc<PrivateNameScope>>,
    pub(crate) is_field_initializer: bool,
    pub(crate) arguments_immutable: bool,
    pub(crate) has_parameter_expressions: bool,
    pub(crate) has_simple_params: bool,
    pub(crate) is_simple_catch_scope: bool,
    pub(crate) is_derived_constructor_scope: bool,
    // §9.1.1.5.5 CreateImportBinding: indirect bindings for module imports
    pub(crate) indirect_bindings: Option<HashMap<String, (EnvRef, String)>>,
    // Module path for import.meta resolution (§16.2.1.5.2 GetActiveScriptOrModule)
    pub(crate) module_path: Option<super::ModuleKey>,
    /// Epoch of the last collection walk that visited this environment
    /// (`EnvVisits`); never reset, since an epoch is never reused.
    gc_epoch: Cell<u64>,
    /// Whether the collector's registry already holds this environment.
    gc_registered: Cell<bool>,
}

static NEXT_ENV_VISIT_EPOCH: AtomicU64 = AtomicU64::new(1);

/// The environments one collection has already walked. Membership is a stamp
/// on the environment itself rather than a hash-set probe, so a closure-heavy
/// heap pays a load and a store per environment visit.
///
/// A minor collection also skips the bindings of a clean environment (see
/// `BindingMap::is_clean`) and records every environment it did scan in
/// `scanned`, so the collector can afterwards mark each one clean or leave it
/// for the next minor collection.
pub(crate) struct EnvVisits {
    epoch: u64,
    pub(crate) minor: bool,
    pub(crate) scanned: Vec<EnvRef>,
    #[cfg(feature = "perf-counters")]
    pub(crate) probes: u64,
    #[cfg(feature = "perf-counters")]
    pub(crate) first_visits: u64,
    #[cfg(feature = "perf-counters")]
    pub(crate) clean_skips: u64,
}

impl EnvVisits {
    pub(crate) fn new(minor: bool) -> Self {
        Self {
            epoch: NEXT_ENV_VISIT_EPOCH.fetch_add(1, Ordering::Relaxed),
            minor,
            scanned: Vec::new(),
            #[cfg(feature = "perf-counters")]
            probes: 0,
            #[cfg(feature = "perf-counters")]
            first_visits: 0,
            #[cfg(feature = "perf-counters")]
            clean_skips: 0,
        }
    }

    #[cfg(debug_assertions)]
    pub(crate) fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Stamps `env` and reports whether this is its first visit.
    #[inline]
    pub(crate) fn first_visit(&mut self, env: &Environment) -> bool {
        let first = env.gc_epoch.get() != self.epoch;
        if first {
            env.gc_epoch.set(self.epoch);
        }
        #[cfg(feature = "perf-counters")]
        {
            self.probes += 1;
            self.first_visits += u64::from(first);
        }
        first
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DisposeHint {
    Sync,
    Async,
}

impl DisposeHint {
    /// The hint a `using` / `await using` declaration disposes its resources
    /// with. Non-`using` kinds never reach this.
    pub(crate) fn for_var_kind(kind: VarKind) -> Self {
        if kind == VarKind::AwaitUsing {
            Self::Async
        } else {
            Self::Sync
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DisposableResource {
    pub(crate) value: JsValue,
    pub(crate) hint: DisposeHint,
    pub(crate) dispose_method: JsValue,
}

impl std::fmt::Debug for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Environment")
            .field("bindings", &self.bindings)
            .field("strict", &self.strict)
            .field("has_with_object", &self.with_object.is_some())
            .finish()
    }
}

pub(crate) struct ThisBinding {
    pub(crate) value: JsValue,
    pub(crate) initialized: bool,
    /// §9.1.1.3 [[NewTarget]]: the constructor a [[Construct]] call was made
    /// through, undefined for [[Call]] and for scopes that are not functions.
    pub(crate) new_target: JsValue,
}

pub(crate) struct WithObject {
    pub(crate) obj_id: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct Binding {
    pub(crate) value: JsValue,
    pub(crate) kind: BindingKind,
    pub(crate) initialized: bool,
    pub(crate) deletable: bool,
}

impl Binding {
    pub(crate) fn new(value: JsValue, kind: BindingKind, initialized: bool) -> Self {
        Self {
            value,
            kind,
            initialized,
            deletable: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum BindingKind {
    Var,
    Let,
    Const,
    FunctionName,
    /// Non-writable global value property (NaN, Infinity, undefined).
    /// Strict mode: throws TypeError. Sloppy mode: silently fails.
    ImmutableValue,
}

impl BindingKind {
    /// The kind of a `let`/`const`/class binding created in TDZ.
    pub(crate) fn lexical(is_const: bool) -> Self {
        if is_const { Self::Const } else { Self::Let }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum SetBindingCheck {
    Ok,
    ConstAssign,
    FunctionNameAssign,
    TdzError,
    Unresolvable,
}

impl Environment {
    /// Whether `found` holds for the id of an object this environment
    /// references: the values of its bindings, `this` and `new.target`, a
    /// `with` target or a pending disposable resource.
    pub(crate) fn any_root_id(&self, mut found: impl FnMut(u64) -> bool) -> bool {
        for binding in self.bindings.values() {
            if let Some(id) = binding.value.as_object_id()
                && found(id)
            {
                return true;
            }
        }
        if let Some(this) = &self.this_binding {
            for value in [&this.value, &this.new_target] {
                if let Some(id) = value.as_object_id()
                    && found(id)
                {
                    return true;
                }
            }
        }
        if let Some(with) = &self.with_object
            && found(with.obj_id)
        {
            return true;
        }
        if let Some(stack) = &self.dispose_stack {
            for resource in stack {
                for value in [&resource.value, &resource.dispose_method] {
                    if let Some(id) = value.as_object_id()
                        && found(id)
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Calls `f` with the id of every object this environment references.
    pub(crate) fn for_each_root_id(&self, mut f: impl FnMut(u64)) {
        self.any_root_id(|id| {
            f(id);
            false
        });
    }

    /// An environment with every field at its initial value except the ones
    /// a constructor chooses; `release_to_pool` restores the same state.
    fn blank(
        bindings: BindingMap,
        parent: Option<EnvRef>,
        strict: bool,
        is_function_scope: bool,
        with_object: Option<WithObject>,
    ) -> Environment {
        Environment {
            bindings,
            this_binding: None,
            parent,
            strict,
            is_function_scope,
            is_arrow_scope: false,
            with_object,
            dispose_stack: None,
            global_object_id: None,
            annexb_function_names: None,
            class_private_names: None,
            is_field_initializer: false,
            arguments_immutable: false,
            has_parameter_expressions: false,
            has_simple_params: true,
            is_simple_catch_scope: false,
            is_derived_constructor_scope: false,
            indirect_bindings: None,
            module_path: None,
            gc_epoch: Cell::new(0),
            gc_registered: Cell::new(false),
        }
    }

    pub(crate) fn new(parent: Option<EnvRef>) -> EnvRef {
        let strict = parent.as_ref().is_some_and(|p| p.borrow().strict);
        Rc::new(RefCell::new(Self::blank(
            BindingMap::new(),
            parent,
            strict,
            false,
            None,
        )))
    }

    /// §9.1.2.3 NewObjectEnvironment: a with-environment over `obj_id`, whose
    /// `[[OuterEnv]]` is `parent`.
    pub(crate) fn new_with_object(parent: EnvRef, obj_id: u64) -> EnvRef {
        let strict = parent.borrow().strict;
        Rc::new(RefCell::new(Self::blank(
            BindingMap::new(),
            Some(parent),
            strict,
            false,
            Some(WithObject { obj_id }),
        )))
    }

    pub(crate) fn new_function_scope(parent: Option<EnvRef>) -> EnvRef {
        Self::new_function_scope_with_capacity(parent, 0)
    }

    pub(crate) fn new_function_scope_with_capacity(
        parent: Option<EnvRef>,
        binding_capacity: usize,
    ) -> EnvRef {
        let strict = parent.as_ref().is_some_and(|p| p.borrow().strict);
        Rc::new(RefCell::new(Self::blank(
            BindingMap::with_capacity(binding_capacity),
            parent,
            strict,
            true,
            None,
        )))
    }

    /// Returns this environment to the state a pooled one holds: no
    /// bindings (their storage is kept), no parent, no `this`, and every
    /// other field as `Environment::new` builds it. Paired with
    /// [`Environment::adopt_pooled`], which then only sets what differs per
    /// use, so a call resets the environment once instead of on both sides of
    /// the pool.
    pub(crate) fn release_to_pool(&mut self) {
        // Naming every field makes a new one a compile error here, so the
        // pooled state cannot silently miss it.
        let Environment {
            bindings,
            this_binding,
            parent,
            strict,
            is_function_scope,
            is_arrow_scope,
            with_object,
            dispose_stack,
            global_object_id,
            annexb_function_names,
            class_private_names,
            is_field_initializer,
            arguments_immutable,
            has_parameter_expressions,
            has_simple_params,
            is_simple_catch_scope,
            is_derived_constructor_scope,
            indirect_bindings,
            module_path,
            gc_epoch: _,
            gc_registered: _,
        } = self;
        bindings.clear();
        *this_binding = None;
        *parent = None;
        *strict = false;
        *is_function_scope = false;
        *is_arrow_scope = false;
        *with_object = None;
        *dispose_stack = None;
        *global_object_id = None;
        *annexb_function_names = None;
        *class_private_names = None;
        *is_field_initializer = false;
        *arguments_immutable = false;
        *has_parameter_expressions = false;
        *has_simple_params = true;
        *is_simple_catch_scope = false;
        *is_derived_constructor_scope = false;
        *indirect_bindings = None;
        *module_path = None;
    }

    /// Makes a pooled environment (see [`Environment::release_to_pool`]) a
    /// fresh scope under `parent`, as `Environment::new` or
    /// `new_function_scope_with_capacity` would build one.
    pub(crate) fn adopt_pooled(
        &mut self,
        parent: Option<EnvRef>,
        binding_capacity: usize,
        is_function_scope: bool,
    ) {
        self.strict = parent.as_ref().is_some_and(|p| p.borrow().strict);
        self.parent = parent;
        self.is_function_scope = is_function_scope;
        self.bindings.reserve(binding_capacity);
    }

    pub(crate) fn find_module_path(env: &EnvRef) -> Option<super::ModuleKey> {
        if let Some(ref mp) = env.borrow().module_path {
            return Some(mp.clone());
        }
        if let Some(ref parent) = env.borrow().parent {
            return Self::find_module_path(parent);
        }
        None
    }

    /// Find the nearest function scope (for var hoisting).
    /// Returns self if this is a function scope, otherwise traverses up.
    pub(crate) fn find_var_scope(env: &EnvRef) -> EnvRef {
        if env.borrow().is_function_scope || env.borrow().global_object_id.is_some() {
            return env.clone();
        }
        if let Some(ref parent) = env.borrow().parent {
            return Self::find_var_scope(parent);
        }
        env.clone()
    }

    /// Binds `this` to an initialized value (§9.1.1.3.1 BindThisValue).
    /// An existing [[NewTarget]] is kept, so a derived constructor's `super()`
    /// binds `this` without losing it.
    pub(crate) fn set_this(&mut self, value: JsValue) {
        let new_target = self
            .this_binding
            .take()
            .map_or(JsValue::UNDEFINED, |t| t.new_target);
        self.bindings.mark_written();
        self.this_binding = Some(ThisBinding {
            value,
            initialized: true,
            new_target,
        });
    }

    /// Binds `this` and [[NewTarget]] of a function environment being created
    /// for a call.
    pub(crate) fn bind_this_for_call(&mut self, value: JsValue, new_target: JsValue) {
        self.bindings.mark_written();
        self.this_binding = Some(ThisBinding {
            value,
            initialized: true,
            new_target,
        });
    }

    /// Creates the `this` binding uninitialized: a derived constructor's TDZ
    /// until `super()` returns.
    pub(crate) fn declare_this_uninit(&mut self, new_target: JsValue) {
        self.bindings.mark_written();
        self.this_binding = Some(ThisBinding {
            value: JsValue::UNDEFINED,
            initialized: false,
            new_target,
        });
    }

    /// The `this` binding this environment itself holds, if it has one.
    #[inline]
    pub(crate) fn this_binding(&self) -> Option<&ThisBinding> {
        self.this_binding.as_ref()
    }

    pub(crate) fn has_pending_disposables(&self) -> bool {
        self.dispose_stack
            .as_ref()
            .is_some_and(|stack| !stack.is_empty())
    }

    /// Whether a disposable-resource stack exists, even an empty one.
    pub(crate) fn has_dispose_stack(&self) -> bool {
        self.dispose_stack.is_some()
    }

    /// Queues a `using` resource for the block's DisposeResources.
    pub(crate) fn push_disposable(&mut self, resource: DisposableResource) {
        self.bindings.mark_written();
        self.dispose_stack
            .get_or_insert_with(Vec::new)
            .push(resource);
    }

    pub(crate) fn take_disposables(&mut self) -> Option<Vec<DisposableResource>> {
        self.dispose_stack.take()
    }

    #[cfg(debug_assertions)]
    pub(crate) fn gc_epoch(&self) -> u64 {
        self.gc_epoch.get()
    }

    /// Whether a scan found no nursery object in this environment and nothing
    /// has been written to it since.
    #[inline]
    pub(crate) fn is_clean(&self) -> bool {
        self.bindings.is_clean()
    }

    /// The collector's: a scan found no nursery object here. Callers register
    /// the environment (`register_with_collector`) so a minor collection finds
    /// it again once it is written.
    pub(crate) fn mark_clean(&self) {
        self.bindings.mark_clean();
    }

    /// Clears the clean mark.
    pub(crate) fn mark_written(&self) {
        self.bindings.mark_written();
    }

    pub(crate) fn is_registered(&self) -> bool {
        self.gc_registered.get()
    }

    /// Reports whether the collector's registry already held this
    /// environment, registering it.
    pub(crate) fn register_with_collector(&self) -> bool {
        self.gc_registered.replace(true)
    }

    /// `GetNewTarget` (§9.4.3): the [[NewTarget]] of the nearest function
    /// environment up the chain. Arrow environments carry no `this` binding, so
    /// they and direct eval resolve to the enclosing function; field
    /// initializers, static blocks and the global scope yield undefined.
    pub(crate) fn get_new_target(&self) -> JsValue {
        if let Some(this) = &self.this_binding {
            return this.new_target.clone();
        }
        self.parent
            .as_ref()
            .map_or(JsValue::UNDEFINED, |p| p.borrow().get_new_target())
    }

    /// The value of the nearest `this` binding up the chain, or `None` when
    /// that binding is uninitialized (or absent).
    /// The `this` value this environment itself binds, when it is initialized.
    #[inline]
    pub(crate) fn bound_this(&self) -> Option<JsValue> {
        let this = self.this_binding.as_ref()?;
        this.initialized.then(|| this.value.clone())
    }

    pub(crate) fn get_this(&self) -> Option<JsValue> {
        if self.this_binding.is_some() {
            return self.bound_this();
        }
        self.parent.as_ref().and_then(|p| p.borrow().get_this())
    }

    /// `declare` for an already-interned name: no hashing, no allocation.
    pub(crate) fn declare_atom(&mut self, name: &Atom, kind: BindingKind) {
        self.bindings.insert_atom(
            name,
            Binding {
                value: JsValue::UNDEFINED,
                kind,
                initialized: kind == BindingKind::Var,
                deletable: false,
            },
        );
    }

    pub(crate) fn declare(&mut self, name: &str, kind: BindingKind) {
        self.bindings.insert(
            name,
            Binding {
                value: JsValue::UNDEFINED,
                kind,
                initialized: kind == BindingKind::Var,
                deletable: false,
            },
        );
    }

    /// §9.1.1.1.4 InitializeBinding (no TDZ check)
    pub(crate) fn initialize_binding(&mut self, name: &str, value: JsValue) {
        if let Some(binding) = self.bindings.get_mut(name) {
            binding.value = value;
            binding.initialized = true;
        }
    }

    /// §9.1.1.5.5 CreateImportBinding: an immutable indirect binding.
    pub(crate) fn create_import_binding(
        &mut self,
        local_name: &str,
        source_env: EnvRef,
        source_name: String,
    ) {
        let map = self.indirect_bindings.get_or_insert_with(HashMap::new);
        map.insert(local_name.to_string(), (source_env, source_name));
    }

    /// Resolve an indirect binding to its source environment's current value.
    pub(crate) fn resolve_indirect_binding(&self, name: &str) -> Option<Option<JsValue>> {
        if let Some(ref indirect) = self.indirect_bindings
            && let Some((source_env, source_name)) = indirect.get(name)
        {
            let source = source_env.borrow();
            if let Some(binding) = source.bindings.get(source_name.as_str()) {
                if !binding.initialized {
                    return Some(None); // TDZ
                }
                return Some(Some(binding.value.clone()));
            }
            // Binding not found in direct bindings, try its own indirect bindings
            if let Some(ref src_indirect) = source.indirect_bindings
                && let Some((src2_env, src2_name)) = src_indirect.get(source_name.as_str())
            {
                let src2 = src2_env.borrow();
                if let Some(binding) = src2.bindings.get(src2_name.as_str()) {
                    if !binding.initialized {
                        return Some(None); // TDZ
                    }
                    return Some(Some(binding.value.clone()));
                }
            }
            return Some(None); // binding not found = TDZ
        }
        None
    }

    /// Check if name is an indirect binding.
    pub(crate) fn is_indirect_binding(&self, name: &str) -> bool {
        self.indirect_bindings
            .as_ref()
            .is_some_and(|m| m.contains_key(name))
    }

    pub(crate) fn declare_deletable(&mut self, name: &str, kind: BindingKind) {
        self.bindings.insert(
            name,
            Binding {
                value: JsValue::UNDEFINED,
                kind,
                initialized: kind == BindingKind::Var,
                deletable: true,
            },
        );
    }

    /// Bindings-only set. Mirroring to the realm's global object lives in
    /// `Interpreter::env_set`. External callers should always use that wrapper.
    pub(crate) fn set(&mut self, name: &str, value: JsValue) -> Result<(), JsValue> {
        // Indirect bindings (module imports) are immutable
        if self.is_indirect_binding(name) {
            return Err(JsValue::from_str("Assignment to constant variable."));
        }
        if let Some(binding) = self.bindings.get_mut(name) {
            // TDZ: let/const bindings that haven't been initialized yet
            if !binding.initialized && matches!(binding.kind, BindingKind::Let | BindingKind::Const)
            {
                return Err(JsValue::from_str(&format!(
                    "Cannot access '{name}' before initialization"
                )));
            }
            if binding.kind == BindingKind::Const && binding.initialized {
                return Err(JsValue::from_str("Assignment to constant variable."));
            }
            if (binding.kind == BindingKind::FunctionName
                || binding.kind == BindingKind::ImmutableValue)
                && binding.initialized
            {
                if self.strict {
                    return Err(JsValue::from_str("Assignment to constant variable."));
                }
                return Ok(());
            }
            binding.value = value;
            binding.initialized = true;
            Ok(())
        } else if let Some(parent) = &self.parent {
            parent.borrow_mut().set(name, value)
        } else {
            self.bindings
                .insert(name, Binding::new(value, BindingKind::Var, true));
            Ok(())
        }
    }

    /// Bindings-only get. Falls through to `None` at the chain root; the
    /// global-object fall-through lives in `Interpreter::env_get` /
    /// `env_get_ref`.
    pub(crate) fn get(&self, name: &str) -> Option<JsValue> {
        // Check indirect bindings first (module imports)
        if let Some(resolved) = self.resolve_indirect_binding(name) {
            return resolved; // None = TDZ, Some(v) = value
        }
        if let Some(binding) = self.bindings.get(name) {
            if !binding.initialized {
                return None; // TDZ
            }
            Some(binding.value.clone())
        } else if let Some(parent) = &self.parent {
            parent.borrow().get(name)
        } else {
            None
        }
    }

    /// Check if a binding exists but is uninitialized (in TDZ).
    /// Only checks the current environment, not parents.
    pub(crate) fn is_in_tdz(&self, name: &str) -> bool {
        // Check indirect bindings first
        if let Some(resolved) = self.resolve_indirect_binding(name) {
            return resolved.is_none(); // None means TDZ
        }
        if let Some(binding) = self.bindings.get(name) {
            !binding.initialized
        } else {
            false
        }
    }

    /// Bindings-only `has`. Global-object fall-through lives in
    /// `Interpreter::env_has`.
    pub(crate) fn has(&self, name: &str) -> bool {
        if self.is_indirect_binding(name) || self.bindings.contains_key(name) {
            true
        } else if let Some(parent) = &self.parent {
            parent.borrow().has(name)
        } else {
            false
        }
    }

    pub(crate) fn find_binding_env<N: BindingName + ?Sized>(
        env: &EnvRef,
        name: &N,
    ) -> Option<EnvRef> {
        let e = env.borrow();
        if e.is_indirect_binding(name.as_name()) || e.bindings.contains_key(name) {
            return Some(env.clone());
        }
        if e.global_object_id.is_some() {
            return Some(env.clone());
        }
        if let Some(ref parent) = e.parent {
            let parent_clone = parent.clone();
            drop(e);
            return Self::find_binding_env(&parent_clone, name);
        }
        None
    }
}

pub(crate) enum JsFunction {
    User {
        name: Option<String>,
        params: Rc<Vec<Pattern>>,
        body: Body,
        closure: EnvRef,
        is_arrow: bool,
        is_strict: bool,
        is_generator: bool,
        is_async: bool,
        is_method: bool,
        source_text: Option<SourceText>,
        uses_arguments: bool,
        /// Cached `IsSimpleParameterList` (§15.1.3): all params are plain
        /// identifiers (no rest/defaults/destructuring). Computed once at
        /// creation time from `params` so the hot call path need not rescan.
        has_simple_params: bool,
        /// Compile verdict for `body`, filled on the first call and shared by
        /// every closure of the literal (see `shared_bytecode_verdict`).
        bytecode_cache: super::bytecode::BytecodeCacheState,
        /// Call-frame facts derived from the literal, shared by its closures
        /// (#1062). Functions built without a literal carry their own.
        frame: Rc<super::literal_frame::LiteralFrame>,
    },
    Native(
        String,
        usize,
        Rc<dyn Fn(&mut super::Interpreter, &JsValue, &[JsValue]) -> Completion>,
        bool, // is_constructor
    ),
}

impl JsFunction {
    pub(crate) fn native(
        name: String,
        arity: usize,
        f: impl Fn(&mut super::Interpreter, &JsValue, &[JsValue]) -> Completion + 'static,
    ) -> Self {
        JsFunction::Native(name, arity, Rc::new(f), false)
    }

    pub(crate) fn constructor(
        name: String,
        arity: usize,
        f: impl Fn(&mut super::Interpreter, &JsValue, &[JsValue]) -> Completion + 'static,
    ) -> Self {
        JsFunction::Native(name, arity, Rc::new(f), true)
    }
}

impl Clone for JsFunction {
    fn clone(&self) -> Self {
        match self {
            JsFunction::User {
                name,
                params,
                body,
                closure,
                is_arrow,
                is_strict,
                is_generator,
                is_async,
                is_method,
                source_text,
                uses_arguments,
                has_simple_params,
                bytecode_cache,
                frame,
            } => JsFunction::User {
                name: name.clone(),
                params: params.clone(),
                body: body.clone(),
                closure: closure.clone(),
                is_arrow: *is_arrow,
                is_strict: *is_strict,
                is_generator: *is_generator,
                is_async: *is_async,
                is_method: *is_method,
                source_text: source_text.clone(),
                uses_arguments: *uses_arguments,
                has_simple_params: *has_simple_params,
                bytecode_cache: bytecode_cache.clone(),
                frame: frame.clone(),
            },
            JsFunction::Native(name, arity, f, is_ctor) => {
                JsFunction::Native(name.clone(), *arity, f.clone(), *is_ctor)
            }
        }
    }
}

impl std::fmt::Debug for JsFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsFunction::User { name, .. } => write!(f, "JsFunction::User({name:?})"),
            JsFunction::Native(name, arity, _, is_ctor) => {
                write!(f, "JsFunction::Native({name:?}, {arity}, ctor={is_ctor})")
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PropertyDescriptor {
    pub value: Option<JsValue>,
    pub writable: Option<bool>,
    pub get: Option<JsValue>,
    pub set: Option<JsValue>,
    pub enumerable: Option<bool>,
    pub configurable: Option<bool>,
}

impl PropertyDescriptor {
    pub(crate) fn data(
        value: JsValue,
        writable: bool,
        enumerable: bool,
        configurable: bool,
    ) -> Self {
        Self {
            value: Some(value),
            writable: Some(writable),
            get: None,
            set: None,
            enumerable: Some(enumerable),
            configurable: Some(configurable),
        }
    }

    pub(crate) fn data_default(value: JsValue) -> Self {
        Self::data(value, true, true, true)
    }

    pub(crate) fn accessor(
        get: Option<JsValue>,
        set: Option<JsValue>,
        enumerable: bool,
        configurable: bool,
    ) -> Self {
        Self {
            value: None,
            writable: None,
            get,
            set,
            enumerable: Some(enumerable),
            configurable: Some(configurable),
        }
    }

    pub(crate) fn is_data_descriptor(&self) -> bool {
        self.value.is_some() || self.writable.is_some()
    }

    pub(crate) fn is_accessor_descriptor(&self) -> bool {
        self.get.is_some() || self.set.is_some()
    }
}

#[derive(Debug, Clone)]
pub(crate) enum PrivateFieldDef {
    Field {
        name: PrivateId,
        /// Source spelling without the `#`, for NamedEvaluation of an
        /// anonymous function initializer.
        source_name: String,
        initializer: Option<Expression>,
    },
    Method {
        name: PrivateId,
        value: JsValue,
    },
    Accessor {
        name: PrivateId,
        get: Option<JsValue>,
        set: Option<JsValue>,
    },
}

/// A Private Name (`sec-private-names`): unique per class evaluation and
/// source name, so two evaluations of one class body (or two classes spelling
/// `#x`) never alias. `0` is reserved for an unresolvable name, which no
/// object ever holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PrivateId(pub(crate) u64);

impl PrivateId {
    pub(crate) const UNRESOLVED: PrivateId = PrivateId(0);
    /// Engine-internal slots (`JsObjectData::hidden_slot`) take ids from the
    /// top of the range, which the class-evaluation counter never reaches.
    pub(crate) const BOUND_COMPARE: PrivateId = PrivateId(u64::MAX);
    pub(crate) const BOUND_FORMAT: PrivateId = PrivateId(u64::MAX - 1);
}

/// The Private Environment Record of one class evaluation: source name to
/// the Private Name that evaluation created.
#[derive(Debug, Default)]
pub(crate) struct PrivateNameScope {
    entries: Vec<(String, PrivateId)>,
}

impl PrivateNameScope {
    pub(crate) fn insert_if_absent(&mut self, name: &str, fresh: impl FnOnce() -> PrivateId) {
        if self.get(name).is_none() {
            self.entries.push((name.to_string(), fresh()));
        }
    }

    #[inline]
    pub(crate) fn get(&self, name: &str) -> Option<PrivateId> {
        self.entries
            .iter()
            .find_map(|(n, id)| (n == name).then_some(*id))
    }

    pub(crate) fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(n, _)| n.as_str())
    }
}

/// An object's [[PrivateElements]]. Objects hold a handful, so a linear scan
/// over integer ids beats hashing.
#[derive(Debug, Default)]
pub(crate) struct PrivateFields {
    entries: Vec<(PrivateId, PrivateElement)>,
}

impl PrivateFields {
    #[inline]
    pub(crate) fn get(&self, id: PrivateId) -> Option<&PrivateElement> {
        self.entries.iter().find(|(k, _)| *k == id).map(|(_, e)| e)
    }

    #[inline]
    pub(crate) fn get_mut(&mut self, id: PrivateId) -> Option<&mut PrivateElement> {
        self.entries
            .iter_mut()
            .find(|(k, _)| *k == id)
            .map(|(_, e)| e)
    }

    pub(crate) fn insert(&mut self, id: PrivateId, elem: PrivateElement) {
        match self.get_mut(id) {
            Some(slot) => *slot = elem,
            None => self.entries.push((id, elem)),
        }
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &PrivateElement> {
        self.entries.iter().map(|(_, e)| e)
    }
}

/// Unified ordered instance field definition (private or public), preserving source order.
#[derive(Debug, Clone)]
pub(crate) enum InstanceFieldDef {
    Private(PrivateFieldDef),
    Public(JsPropertyKey, Option<Expression>),
    /// Auto accessor backing storage initialization: (storage_slot_name, initializer)
    AutoAccessorStorage(PrivateId, Option<Expression>),
}

#[derive(Debug, Clone)]
pub(crate) enum PrivateElement {
    Field(JsValue),
    Method(JsValue),
    Accessor {
        get: Option<JsValue>,
        set: Option<JsValue>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum IteratorKind {
    Key,
    Value,
    KeyValue,
}

#[derive(Debug, Clone)]
pub(crate) enum IteratorState {
    ArrayIterator {
        array_id: u64,
        index: usize,
        kind: IteratorKind,
        done: bool,
    },
    StringIterator {
        string: JsString,
        position: usize,
        done: bool,
    },
    MapIterator {
        map_id: u64,
        /// Spec index into [[MapData]], stepped by `KeyedEntries::next_live`;
        /// not a position in the physical entry storage.
        index: usize,
        kind: IteratorKind,
        done: bool,
    },
    SetIterator {
        set_id: u64,
        /// Spec index into [[SetData]]; see `MapIterator::index`.
        index: usize,
        kind: IteratorKind,
        done: bool,
    },
    Generator {
        body: Body,
        func_env: EnvRef,
        is_strict: bool,
        execution_state: GeneratorExecutionState,
    },
    StateMachineGenerator {
        state_machine: Rc<GeneratorStateMachine>,
        func_env: EnvRef,
        is_strict: bool,
        execution_state: StateMachineExecutionState,
        _sent_value: JsValue,
        try_stack: Vec<TryContextInfo>,
        pending_binding: Option<SentValueBinding>,
        delegated_iterator: Option<DelegatedIteratorInfo>,
        pending_exception: Option<JsValue>,
        pending_return: Option<JsValue>,
    },
    /// A generator or async generator whose body is a bytecode chunk
    /// (`Op::Yield`).
    BytecodeGenerator(Box<BytecodeGenerator>),
    StateMachineAsyncGenerator {
        state_machine: Rc<GeneratorStateMachine>,
        func_env: EnvRef,
        is_strict: bool,
        execution_state: StateMachineExecutionState,
        _sent_value: JsValue,
        try_stack: Vec<TryContextInfo>,
        pending_binding: Option<SentValueBinding>,
        delegated_iterator: Option<DelegatedIteratorInfo>,
        pending_exception: Option<JsValue>,
        pending_return: Option<JsValue>,
    },
    RegExpStringIterator {
        string: JsString,
        global: bool,
        full_unicode: bool,
        state: BytecodeGeneratorStatus,
        matcher_id: u64,
    },
    TypedArrayIterator {
        typed_array_id: u64,
        index: usize,
        kind: IteratorKind,
        done: bool,
    },
    SegmentIterator {
        segments: Vec<(Vec<u16>, usize, bool)>, // (segment, index, isWordLike)
        input: Rc<Vec<u16>>,
        position: usize,
        done: bool,
    },
    /// §14.7.5.10 the internal iterator `EnumerateObjectProperties` yields for
    /// for-in. It is never reachable from script; the state-machine drivers
    /// step it through `%ForInIteratorPrototype%.next`.
    ForInEnumerator {
        obj_id: Option<u64>,
        keys: Vec<JsPropertyKey>,
        index: usize,
    },
}

impl IteratorState {
    /// The completed (fully drained) terminal state of a **sync** state-machine
    /// generator. Every resumption field is fixed at its completed default —
    /// `execution_state = Completed`, `_sent_value = UNDEFINED`, an empty
    /// `try_stack`, and every `pending_*`/`delegated_iterator` set to `None` —
    /// so a caller supplies only the three fields a finished generator still
    /// owns. Replaces the byte-identical 10-field literal previously inlined at
    /// every completion site.
    pub(crate) fn completed_state_machine_generator(
        state_machine: Rc<GeneratorStateMachine>,
        func_env: EnvRef,
        is_strict: bool,
    ) -> IteratorState {
        IteratorState::StateMachineGenerator {
            state_machine,
            func_env,
            is_strict,
            execution_state: StateMachineExecutionState::Completed,
            _sent_value: JsValue::UNDEFINED,
            try_stack: Vec::new(),
            pending_binding: None,
            delegated_iterator: None,
            pending_exception: None,
            pending_return: None,
        }
    }

    /// The completed terminal state of an **async** state-machine generator;
    /// identical field defaults to [`Self::completed_state_machine_generator`],
    /// distinguished only by the variant tag.
    pub(crate) fn completed_state_machine_async_generator(
        state_machine: Rc<GeneratorStateMachine>,
        func_env: EnvRef,
        is_strict: bool,
    ) -> IteratorState {
        IteratorState::StateMachineAsyncGenerator {
            state_machine,
            func_env,
            is_strict,
            execution_state: StateMachineExecutionState::Completed,
            _sent_value: JsValue::UNDEFINED,
            try_stack: Vec::new(),
            pending_binding: None,
            delegated_iterator: None,
            pending_exception: None,
            pending_return: None,
        }
    }
}

#[cfg(test)]
#[path = "types/generator_tests.rs"]
mod completed_state_machine_generator_tests;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum TypedArrayKind {
    Int8,
    Uint8,
    Uint8Clamped,
    Int16,
    Uint16,
    Int32,
    Uint32,
    Float16,
    Float32,
    Float64,
    BigInt64,
    BigUint64,
}

impl TypedArrayKind {
    pub(crate) fn bytes_per_element(&self) -> usize {
        match self {
            TypedArrayKind::Int8 | TypedArrayKind::Uint8 | TypedArrayKind::Uint8Clamped => 1,
            TypedArrayKind::Int16 | TypedArrayKind::Uint16 | TypedArrayKind::Float16 => 2,
            TypedArrayKind::Int32 | TypedArrayKind::Uint32 | TypedArrayKind::Float32 => 4,
            TypedArrayKind::Float64 | TypedArrayKind::BigInt64 | TypedArrayKind::BigUint64 => 8,
        }
    }

    pub(crate) fn name(&self) -> &'static str {
        match self {
            TypedArrayKind::Int8 => "Int8Array",
            TypedArrayKind::Uint8 => "Uint8Array",
            TypedArrayKind::Uint8Clamped => "Uint8ClampedArray",
            TypedArrayKind::Int16 => "Int16Array",
            TypedArrayKind::Uint16 => "Uint16Array",
            TypedArrayKind::Int32 => "Int32Array",
            TypedArrayKind::Uint32 => "Uint32Array",
            TypedArrayKind::Float16 => "Float16Array",
            TypedArrayKind::Float32 => "Float32Array",
            TypedArrayKind::Float64 => "Float64Array",
            TypedArrayKind::BigInt64 => "BigInt64Array",
            TypedArrayKind::BigUint64 => "BigUint64Array",
        }
    }

    pub(crate) fn is_bigint(&self) -> bool {
        matches!(self, TypedArrayKind::BigInt64 | TypedArrayKind::BigUint64)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TypedArrayInfo {
    pub kind: TypedArrayKind,
    pub buffer: Rc<RefCell<BufferData>>,
    pub byte_offset: usize,
    pub byte_length: usize,
    pub array_length: usize,
    pub is_detached: Rc<Cell<bool>>,
    pub is_length_tracking: bool,
    /// Id of the ArrayBuffer (or SharedArrayBuffer) JS object that wraps `buffer`.
    /// `None` only on the rare construction path that bypasses an object wrapper.
    pub buffer_object_id: Option<u64>,
}

#[derive(Debug, Clone)]
pub(crate) struct DataViewInfo {
    pub buffer: Rc<RefCell<BufferData>>,
    pub byte_offset: usize,
    pub byte_length: usize,
    pub is_detached: Rc<Cell<bool>>,
    pub is_length_tracking: bool,
    /// Id of the ArrayBuffer (or SharedArrayBuffer) JS object that wraps `buffer`.
    pub buffer_object_id: Option<u64>,
    pub is_immutable: bool,
}

#[derive(Clone)]
pub(crate) enum IntlData {
    Locale {
        tag: String,
        language: String,
        script: Option<String>,
        region: Option<String>,
        variants: Option<String>,
        calendar: Option<String>,
        case_first: Option<String>,
        collation: Option<String>,
        hour_cycle: Option<String>,
        numbering_system: Option<String>,
        numeric: Option<bool>,
        first_day_of_week: Option<String>,
    },
    Collator {
        locale: String,
        usage: String,
        sensitivity: String,
        ignore_punctuation: bool,
        collation: String,
        numeric: bool,
        case_first: String,
    },
    NumberFormat {
        locale: String,
        numbering_system: String,
        style: String,
        currency: Option<String>,
        currency_display: Option<String>,
        currency_sign: Option<String>,
        unit: Option<String>,
        unit_display: Option<String>,
        notation: String,
        compact_display: Option<String>,
        sign_display: String,
        use_grouping: String,
        minimum_integer_digits: u32,
        minimum_fraction_digits: u32,
        maximum_fraction_digits: u32,
        minimum_significant_digits: Option<u32>,
        maximum_significant_digits: Option<u32>,
        rounding_mode: String,
        rounding_increment: u32,
        rounding_priority: String,
        trailing_zero_display: String,
    },
    PluralRules(std::rc::Rc<crate::interpreter::builtins::plural_rules_state::PluralRulesState>),
    ListFormat {
        locale: String,
        list_type: String,
        style: String,
    },
    RelativeTimeFormat {
        locale: String,
        style: String,
        numeric: String,
        numbering_system: String,
    },
    Segmenter {
        locale: String,
        granularity: String,
    },
    DisplayNames {
        locale: String,
        style: String,
        display_type: String,
        fallback: String,
        language_display: Option<String>,
    },
    DurationFormat {
        locale: String,
        numbering_system: String,
        style: String,
        years: String,
        years_display: String,
        months: String,
        months_display: String,
        weeks: String,
        weeks_display: String,
        days: String,
        days_display: String,
        hours: String,
        hours_display: String,
        minutes: String,
        minutes_display: String,
        seconds: String,
        seconds_display: String,
        milliseconds: String,
        milliseconds_display: String,
        microseconds: String,
        microseconds_display: String,
        nanoseconds: String,
        nanoseconds_display: String,
        fractional_digits: Option<u32>,
    },
    DateTimeFormat {
        locale: String,
        calendar: String,
        numbering_system: String,
        time_zone: String,
        hour_cycle: Option<String>,
        hour12: Option<bool>,
        weekday: Option<String>,
        era: Option<String>,
        year: Option<String>,
        month: Option<String>,
        day: Option<String>,
        day_period: Option<String>,
        hour: Option<String>,
        minute: Option<String>,
        second: Option<String>,
        fractional_second_digits: Option<u32>,
        time_zone_name: Option<String>,
        date_style: Option<String>,
        time_style: Option<String>,
        has_explicit_components: bool,
    },
}

#[derive(Clone, Debug)]
pub(crate) enum TemporalData {
    Duration {
        years: f64,
        months: f64,
        weeks: f64,
        days: f64,
        hours: f64,
        minutes: f64,
        seconds: f64,
        milliseconds: f64,
        microseconds: f64,
        nanoseconds: f64,
    },
    Instant {
        epoch_nanoseconds: num_bigint::BigInt,
    },
    PlainDate {
        iso_year: i32,
        iso_month: u8,
        iso_day: u8,
        calendar: String,
    },
    PlainTime {
        hour: u8,
        minute: u8,
        second: u8,
        millisecond: u16,
        microsecond: u16,
        nanosecond: u16,
    },
    PlainDateTime {
        iso_year: i32,
        iso_month: u8,
        iso_day: u8,
        hour: u8,
        minute: u8,
        second: u8,
        millisecond: u16,
        microsecond: u16,
        nanosecond: u16,
        calendar: String,
    },
    ZonedDateTime {
        epoch_nanoseconds: num_bigint::BigInt,
        time_zone: String,
        calendar: String,
    },
    PlainYearMonth {
        iso_year: i32,
        iso_month: u8,
        reference_iso_day: u8,
        calendar: String,
    },
    PlainMonthDay {
        iso_month: u8,
        iso_day: u8,
        reference_iso_year: i32,
        calendar: String,
    },
}

/// Which own keys `JsObjectData::ordinary_own_property_keys` returns.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum OwnKeyFilter {
    /// Every String key (enumerable or not), no Symbols.
    AllStrings,
    /// Every key, Strings first and then Symbols.
    AllKeys,
    /// Enumerable String keys only.
    EnumerableStrings,
}

/// Sentinel for an unset realm slot on `JsObjectData`.
const NO_REALM: u32 = u32::MAX;

/// See [`JsObjectData::extras`].
#[derive(Default)]
pub(crate) struct ObjectExtras {
    pub(crate) private_fields: PrivateFields,
    /// Shared with every construction (`Rc::clone`); `Rc::make_mut` in
    /// `instance_field_defs_mut` only copies if a construction still holds it,
    /// which class evaluation (the sole mutator) never races with.
    pub(crate) class_instance_field_defs: Rc<Vec<InstanceFieldDef>>,
    /// Values the collector must keep alive on behalf of this object's native
    /// closures (see `Interpreter::pin_native_root`).
    pub(crate) gc_native_roots: Vec<JsValue>,
}

pub(crate) struct JsObjectData {
    /// Slab index of this object. Set exactly once by
    /// `Interpreter::alloc_object` at allocation time and never reassigned.
    pub id: Option<u64>,
    /// Own properties in creation order.
    pub properties: PropertyMap,
    pub prototype_id: Option<u64>,
    pub callable: Option<Box<JsFunction>>,
    pub class_name: &'static str,
    /// Realm the function object was created in; `None` when it was not
    /// built by `create_function` (the call path then keeps the caller's realm).
    function_realm: u32,
    pub extensible: bool,
    pub primitive_value: Option<JsValue>,
    /// Rarely-used slots (class private elements, instance field
    /// definitions, native GC roots), allocated on first write so an ordinary
    /// object carries one pointer instead of the three containers.
    pub(crate) extras: Option<Box<ObjectExtras>>,
    pub is_raw_json: bool,
    pub constructor_kind: ConstructorKind,
    generator_realm: u32,
    pub(crate) is_htmldda: bool,
    pub(crate) is_immutable_prototype: bool,
    pub(crate) deferred_construct: bool,
    /// Per-object shape id. Bumped by `Interpreter::mutate_object_shape` on
    /// every structural mutation (property add/delete/attribute change,
    /// prototype mutation, proxy install). Pure value reassignment does NOT
    /// bump. `0` is a transient placeholder set by `JsObjectData::new` and
    /// overwritten by `alloc_object` before the object is observable; the
    /// global counter starts at `1`, so an IC slot can never legitimately
    /// hold `obj_shape_id == 0`. See issue #71.
    pub shape_id: u64,
    /// Strict kind discriminator. Populated alongside the legacy `Option<XData>`
    /// fields during Phase B1; subsequent slices migrate dispatch off the old
    /// fields and onto this enum. Defaults to `Ordinary` for plain objects.
    #[allow(dead_code)]
    pub(crate) kind: ObjectKind,
}

pub(crate) use super::module_namespace_data::ModuleNamespaceData;

#[derive(Clone, Debug)]
pub(crate) struct DisposableStackData {
    pub(crate) stack: Vec<DisposableResource>,
    pub(crate) disposed: bool,
}

/// ArrayBuffer / SharedArrayBuffer slot data. Present iff this object is one
/// of those two kinds. Six previously-loose fields with implicit "all
/// meaningful together" invariants are now grouped.
///
/// SAB vs AB is encoded by `is_shared` + presence of `sab_shared`; immutable
/// ABs set `is_immutable`. `detached` is `None` for SAB (which cannot detach)
/// and `Some(Cell<bool>)` for regular AB.
#[derive(Clone, Debug)]
pub(crate) struct ArrayBufferData {
    pub data: Rc<RefCell<BufferData>>,
    pub detached: Option<Rc<Cell<bool>>>,
    pub max_byte_length: Option<usize>,
    pub is_shared: bool,
    pub is_immutable: bool,
    pub sab_shared: Option<Arc<SharedBufferInner>>,
}

/// The constructor flavor of a callable object. Replaces three previously
/// independent booleans (`is_class_constructor`, `is_derived_class_constructor`,
/// `is_default_derived_constructor`) whose set of valid combinations was
/// expressible only by convention — e.g. `default && !derived` was nonsense
/// but compiled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConstructorKind {
    /// A regular function — or the constructor slot is unused.
    Function,
    /// A base class constructor (`class C { ... }` without `extends`).
    Class,
    /// An explicit derived class constructor (`class C extends B { constructor() { ... } }`).
    DerivedClass,
    /// A synthesized default derived constructor — derived class with no
    /// `constructor` method (`class C extends B {}`). Construction bypasses
    /// the synthetic body to avoid Symbol.iterator on the rest parameter
    /// (spec §15.7.14).
    DefaultDerivedClass,
}

/// Outcome of [`JsObjectData::dense_array_set_plan`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum DenseArraySet {
    /// The element is present: the own data property is updated in place.
    Overwrite,
    /// The element is a hole or one past the end: it is created, which is
    /// only sound when no prototype owns the index.
    Create,
}

/// Array-exotic internal data.
///
/// `extra_string_property_order` mirrors only the subset of
/// the property map's creation order needed by the Node-compatible inspector.
/// Maintaining that subset as properties change lets inspection find named
/// Array properties without scanning descriptor-backed element indices.
#[derive(Default)]
pub(crate) struct ArrayData {
    elements: ElementVec,
    /// Allocated on the first extra String key, so a plain Array pays one
    /// pointer.
    #[allow(clippy::box_collection)] // a pointer, not the 24-byte Vec, per Array
    extra_string_property_order: Option<Box<Vec<JsPropertyKey>>>,
    /// Dirty-card bitmap for the generational collector: bit `c` is set when
    /// elements `c * CARD_ELEMENTS ..` (one card) may reference a nursery
    /// object. `None` means every element may, which is the safe state: a new
    /// Array starts there and `ObjectKind::Array`'s only mutable accessor to
    /// the vector (`JsObjectData::array_elements_mut`) returns to it, so an
    /// operation that moves or rewrites elements without knowing which cards
    /// it touched can never leave a stale bitmap behind. Only a minor
    /// collection that has just scanned the whole vector replaces it with a
    /// bitmap, and only a precise single-slot store marks a bit.
    #[allow(clippy::box_collection)] // a pointer per Array, not the 24-byte Vec
    dirty_cards: Option<Box<Vec<u64>>>,
}

impl Clone for ArrayData {
    /// A copy is a new object that no collection has scanned, so it starts with
    /// every card dirty rather than inheriting the original's bitmap.
    fn clone(&self) -> Self {
        Self {
            elements: self.elements.clone(),
            extra_string_property_order: self.extra_string_property_order.clone(),
            dirty_cards: None,
        }
    }
}

/// Elements per card of an Array's dirty-card bitmap. Debug builds use tiny
/// cards so that unit tests and stress runs cross card boundaries constantly.
pub(crate) const CARD_ELEMENTS: usize = if cfg!(debug_assertions) { 4 } else { 512 };

/// Set the bit of the card holding `index` in a dirty-card bitmap.
#[inline]
pub(crate) fn mark_card_bit(bits: &mut Vec<u64>, index: usize) {
    let card = index / CARD_ELEMENTS;
    let word = card / 64;
    if word >= bits.len() {
        bits.resize(word + 1, 0);
    }
    bits[word] |= 1 << (card % 64);
}

/// Fill `bits` with exactly the listed `cards`, sized for `len` elements.
pub(crate) fn fill_card_bits(bits: &mut Vec<u64>, len: usize, cards: &[usize]) {
    bits.clear();
    bits.resize(len.div_ceil(CARD_ELEMENTS).div_ceil(64), 0);
    for &card in cards {
        bits[card / 64] |= 1 << (card % 64);
    }
}

impl ArrayData {
    #[cfg(test)]
    pub(crate) fn new(elements: Vec<JsValue>) -> Self {
        Self {
            elements: elements.into(),
            extra_string_property_order: None,
            dirty_cards: None,
        }
    }
}

/// Discriminator for the kind-specific data attached to a JsObjectData.
///
/// Each variant either is the kind in isolation (Ordinary), or carries the
/// `XData` cluster previously held in an `Option<XData>` field. Variants are
/// **strictly disjoint** — at most one applies to any given object. Cross-cutting
/// aspects (callable closure, prototype chain, property bag) stay as orthogonal
/// fields on JsObjectData; the kind enum captures only the exotic-shape data
/// the spec demands a particular object carry.
///
/// Introduced *alongside* the existing `Option<XData>` fields (both populated
/// on construction); later slices migrate readers (B2), call sites (B3), then
/// delete the vestigial Options (B4).
#[derive(Clone, Default)]
#[allow(dead_code)] // Variants populated as Phase B2/B3 migrate call sites onto this enum.
pub(crate) enum ObjectKind {
    #[default]
    Ordinary,
    Proxy(Box<ProxyData>),
    RegExp(RegExpData),
    BoundFunction(Box<BoundFunctionData>),
    WrappedFunction(WrappedFunctionData),
    IterHelper(IterHelperData),
    ArrayBuffer(Box<ArrayBufferData>),
    TypedArray(Box<TypedArrayInfo>),
    DataView(Box<DataViewInfo>),
    Promise(Box<PromiseData>),
    /// [[MapData]]; see [`KeyedEntries`].
    Map(Box<KeyedEntries<(JsValue, JsValue)>>),
    /// [[SetData]]; see [`KeyedEntries`].
    Set(Box<KeyedEntries<JsValue>>),
    /// FinalizationRegistry cells. Two parallel slot lists:
    ///   `cells[i]`  = Some((target, heldValue)) or None (tombstone)
    ///   `tokens[i]` = Some(unregisterToken) or None
    FinalizationRegistry(Box<FinalizationRegistryData>),
    /// [[WeakRefTarget]]. Held strongly by minor collections and cleared to
    /// `undefined` by the major collection that finds it unreachable.
    WeakRef(JsValue),
    Iterator(Box<IteratorState>),
    /// `parameter_map` for an Arguments exotic object.
    #[allow(clippy::box_collection)] // keeps `ObjectKind` small
    Arguments(Box<ParameterMap>),
    /// Element storage and bounded named-property metadata for an Array exotic object.
    Array(ArrayData),
    /// Primitive value wrapped by `Object(primitive)` (String/Number/Boolean/Symbol/BigInt).
    PrimitiveWrapper(JsValue),
    ModuleNamespace(Box<ModuleNamespaceData>),
    DisposableStack(DisposableStackData),
    Temporal(Box<TemporalData>),
    /// Boxed because `IntlData` is the largest variant (~560 bytes). Boxing keeps
    /// `JsObjectData::kind` compact for the common Ordinary case at the cost of one
    /// indirection on rare Intl-instance access. See clippy `large_enum_variant`.
    Intl(Box<IntlData>),
    /// ShadowRealm instance — carries the realm id of the shadow realm itself.
    ShadowRealm(usize),
}

/// [[ParameterMap]] of a mapped arguments object (§10.4.4.7): which argument
/// indices still alias the binding of the parameter at the same position.
/// Indices are tracked in a flag per parameter; the names are the literal's
/// shared parameter atoms.
#[derive(Debug, Clone)]
pub(crate) struct ParameterMap {
    env: EnvRef,
    names: Rc<[Atom]>,
    live: smallvec::SmallVec<[bool; 8]>,
}

impl ParameterMap {
    /// Maps each index below `argc` whose parameter is the last of its name
    /// (`last_occurrence`, parallel to `names`). `None` when nothing is mapped.
    pub(crate) fn new(
        env: EnvRef,
        names: Rc<[Atom]>,
        last_occurrence: &[bool],
        argc: usize,
    ) -> Option<Self> {
        let live: smallvec::SmallVec<[bool; 8]> =
            last_occurrence.iter().copied().take(argc).collect();
        live.iter()
            .any(|&mapped| mapped)
            .then_some(Self { env, names, live })
    }

    pub(crate) fn env(&self) -> &EnvRef {
        &self.env
    }

    fn mapped_index(&self, key: &str) -> Option<usize> {
        let index = array_element_index(key)?;
        self.live
            .get(index)
            .copied()
            .unwrap_or(false)
            .then_some(index)
    }

    pub(crate) fn contains_key(&self, key: &str) -> bool {
        self.mapped_index(key).is_some()
    }

    pub(crate) fn remove(&mut self, key: &str) {
        if let Some(index) = self.mapped_index(key) {
            self.live[index] = false;
        }
    }

    pub(crate) fn value(&self, key: &str) -> Option<JsValue> {
        self.value_at(self.mapped_index(key)?)
    }

    /// The live value of the parameter mapped to `index`; `None` when that
    /// index is not mapped.
    pub(crate) fn value_at(&self, index: usize) -> Option<JsValue> {
        if !self.live.get(index).copied().unwrap_or(false) {
            return None;
        }
        let env = self.env.borrow();
        let binding = env.bindings.get(&self.names[index])?;
        binding.initialized.then(|| binding.value.clone())
    }

    pub(crate) fn set_value(&self, key: &str, value: &JsValue) {
        if let Some(index) = self.mapped_index(key)
            && let Some(binding) = self.env.borrow_mut().bindings.get_mut(&self.names[index])
        {
            binding.value = value.clone();
        }
    }
}

impl ObjectKind {
    /// Kinds whose named property access is not the ordinary algorithm for
    /// every key. Exhaustive on purpose: a new kind must be classified here.
    /// An Arguments object answers only Array-index keys itself (through its
    /// parameter map), and every named fast path already sends those to the
    /// generic path, as it does for Arrays.
    #[inline(always)]
    pub(crate) fn is_named_access_exotic(&self) -> bool {
        match self {
            ObjectKind::Proxy(_) | ObjectKind::ModuleNamespace(_) | ObjectKind::TypedArray(_) => {
                true
            }
            ObjectKind::Ordinary
            | ObjectKind::Arguments(_)
            | ObjectKind::RegExp(_)
            | ObjectKind::BoundFunction(_)
            | ObjectKind::WrappedFunction(_)
            | ObjectKind::IterHelper(_)
            | ObjectKind::ArrayBuffer(_)
            | ObjectKind::DataView(_)
            | ObjectKind::Promise(_)
            | ObjectKind::Map(_)
            | ObjectKind::Set(_)
            | ObjectKind::FinalizationRegistry(_)
            | ObjectKind::WeakRef(_)
            | ObjectKind::Iterator(_)
            | ObjectKind::Array(_)
            | ObjectKind::PrimitiveWrapper(_)
            | ObjectKind::DisposableStack(_)
            | ObjectKind::Temporal(_)
            | ObjectKind::Intl(_)
            | ObjectKind::ShadowRealm(_) => false,
        }
    }
}

/// Whether `key` could name an Array element (it starts with a digit).
#[inline(always)]
pub(crate) fn key_may_be_array_index(key: &JsPropertyKey) -> bool {
    key.as_bytes().first().is_some_and(u8::is_ascii_digit)
}

/// Outcome of [`JsObjectData::store_named_fast`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum NamedStore {
    Stored,
    Absent,
    Bail,
}

/// Iterator-helper or wrap-delegation slot data. The two cases are mutually
/// exclusive at the object level — an `Iterator.from(x)` wrapper carries
/// `Delegation`, while every `Iterator.prototype.{map,filter,take,drop,flatMap}`
/// result carries `Helper`. Before this bundle they shared the same object via
/// disjoint `Option` fields (`wrap_iter_record` vs `helper_*`), and the
/// disjunction was enforced only by convention.
#[derive(Clone, Debug)]
pub(crate) enum IterHelperData {
    Delegation {
        iter: JsValue,
        next: JsValue,
    },
    Helper {
        next: JsValue,
        return_closure: JsValue,
        gen_state: Rc<Cell<u8>>,
    },
}

/// Wrapped-function slot data. Present iff this object is the wrapper a
/// ShadowRealm creates when a function crosses the realm boundary (§24.5.3).
/// Both fields were always set together; making that a non-optional pair
/// removes the implicit "is the target set?" invariant.
#[derive(Clone, Debug)]
pub(crate) struct WrappedFunctionData {
    pub target_id: u64,
    pub caller_realm_id: usize,
}

/// Bound-function slot data. Present iff this object is a bound function
/// (created by `Function.prototype.bind`). The three slots — target, bound
/// `this`, and pre-bound positional args — were previously held as three
/// independent `Option` fields that were always written together; this struct
/// makes that pairing typed.
#[derive(Clone, Debug)]
pub(crate) struct BoundFunctionData {
    pub target: JsValue,
    pub this: JsValue,
    pub args: Vec<JsValue>,
}

/// RegExp instance slot data. Present iff the object is a RegExp instance
/// (constructed via the literal syntax or `new RegExp(...)`).
///
/// Source and flags are stored as the *original* pattern and flag strings —
/// what `RegExp.prototype.source` / `RegExp.prototype.flags` must return.
#[derive(Clone, Debug)]
pub(crate) struct RegExpData {
    pub source: JsString,
    pub flags: JsString,
    /// Compiled matcher and decoded flag bits, filled on first `exec`. Always
    /// derived from `source`/`flags`, which are only ever replaced together
    /// with the whole `RegExpData` (`RegExp.prototype.compile`), so it can
    /// never go stale.
    pub exec_plan: RegExpExecPlanCell,
}

impl RegExpData {
    pub(crate) fn new(source: JsString, flags: JsString) -> Self {
        Self {
            source,
            flags,
            exec_plan: RegExpExecPlanCell::default(),
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct RegExpExecPlanCell(
    pub(crate) std::cell::OnceCell<std::rc::Rc<crate::interpreter::builtins::regexp::ExecPlan>>,
);

impl std::fmt::Debug for RegExpExecPlanCell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RegExpExecPlanCell")
    }
}

/// Proxy slot data. Present on an object iff it is (or was) a Proxy.
///
/// Invariants:
/// - Constructed via `ProxyData::active(target, handler)` — both ids `Some`, `revoked == false`.
/// - After `revoke()`, both ids are `None` and `revoked == true`. Once revoked, never reactivated.
/// - `JsObjectData::is_proxy()` returns true only for the *active* state, matching the engine's
///   pre-existing predicate semantics. Use `is_proxy_revoked()` for the revoked branch.
#[derive(Clone, Debug)]
pub(crate) struct ProxyData {
    pub target_id: Option<u64>,
    pub handler_id: Option<u64>,
    pub revoked: bool,
}

impl ProxyData {
    pub(crate) fn active(target_id: u64, handler_id: u64) -> Self {
        Self {
            target_id: Some(target_id),
            handler_id: Some(handler_id),
            revoked: false,
        }
    }

    pub(crate) fn revoke(&mut self) {
        self.target_id = None;
        self.handler_id = None;
        self.revoked = true;
    }
}

/// Generate `pub(crate) fn $name(&self) -> Option<&$ty>` and optionally a
/// paired `fn $mut_name(&mut self) -> Option<&mut $ty>` for the common
/// `ObjectKind::$variant(T)` accessor pattern.
///
/// Avoids 30+ structurally identical methods; each variant that needs a
/// different shape (boxed deref, struct variant, Copy return) is written out
/// by hand below.
macro_rules! kind_accessor {
    // Immutable + mutable pair.
    ($name:ident, $mut_name:ident, $variant:ident, $ty:ty) => {
        pub(crate) fn $name(&self) -> Option<&$ty> {
            if let ObjectKind::$variant(ref x) = self.kind {
                Some(x)
            } else {
                None
            }
        }
        pub(crate) fn $mut_name(&mut self) -> Option<&mut $ty> {
            if let ObjectKind::$variant(ref mut x) = self.kind {
                Some(x)
            } else {
                None
            }
        }
    };
    // Immutable-only.
    ($name:ident, $variant:ident, $ty:ty) => {
        pub(crate) fn $name(&self) -> Option<&$ty> {
            if let ObjectKind::$variant(ref x) = self.kind {
                Some(x)
            } else {
                None
            }
        }
    };
    // Boxed payload: immutable + mutable pair.
    (boxed $name:ident, $mut_name:ident, $variant:ident, $ty:ty) => {
        pub(crate) fn $name(&self) -> Option<&$ty> {
            if let ObjectKind::$variant(ref x) = self.kind {
                Some(&**x)
            } else {
                None
            }
        }
        pub(crate) fn $mut_name(&mut self) -> Option<&mut $ty> {
            if let ObjectKind::$variant(ref mut x) = self.kind {
                Some(&mut **x)
            } else {
                None
            }
        }
    };
    // Boxed payload: immutable-only.
    (boxed $name:ident, $variant:ident, $ty:ty) => {
        pub(crate) fn $name(&self) -> Option<&$ty> {
            if let ObjectKind::$variant(ref x) = self.kind {
                Some(&**x)
            } else {
                None
            }
        }
    };
}

impl JsObjectData {
    pub(crate) fn new() -> Self {
        Self::with_properties(PropertyMap::new())
    }

    #[inline]
    pub(crate) fn with_properties(properties: PropertyMap) -> Self {
        Self {
            id: None,
            properties,
            prototype_id: None,
            callable: None,
            class_name: "Object",
            function_realm: NO_REALM,
            extensible: true,
            primitive_value: None,
            extras: None,
            is_raw_json: false,
            constructor_kind: ConstructorKind::Function,
            generator_realm: NO_REALM,
            is_htmldda: false,
            is_immutable_prototype: false,
            deferred_construct: false,
            shape_id: 0,
            kind: ObjectKind::Ordinary,
        }
    }

    kind_accessor!(boxed proxy, Proxy, ProxyData);
    kind_accessor!(boxed arraybuffer, arraybuffer_mut, ArrayBuffer, ArrayBufferData);
    kind_accessor!(boxed data_view_info, DataView, DataViewInfo);
    kind_accessor!(boxed typed_array_info, TypedArray, TypedArrayInfo);
    kind_accessor!(boxed iterator_state, iterator_state_mut, Iterator, IteratorState);
    kind_accessor!(boxed promise_data, promise_data_mut, Promise, PromiseData);
    kind_accessor!(boxed parameter_map, parameter_map_mut, Arguments, ParameterMap);
    kind_accessor!(array_data, array_data_mut, Array, ArrayData);
    kind_accessor!(
        boxed map_data,
        map_data_mut,
        Map,
        KeyedEntries<(JsValue, JsValue)>
    );
    kind_accessor!(boxed set_data, set_data_mut, Set, KeyedEntries<JsValue>);
    kind_accessor!(boxed temporal_data, Temporal, TemporalData);
    kind_accessor!(regexp, RegExp, RegExpData);
    kind_accessor!(wrapped, WrappedFunction, WrappedFunctionData);
    kind_accessor!(boxed bound, BoundFunction, BoundFunctionData);
    kind_accessor!(iter_helper, IterHelper, IterHelperData);
    kind_accessor!(
        boxed module_namespace,
        module_namespace_mut,
        ModuleNamespace,
        ModuleNamespaceData
    );
    kind_accessor!(
        disposable_stack,
        disposable_stack_mut,
        DisposableStack,
        DisposableStackData
    );

    pub(crate) fn array_elements(&self) -> Option<&ElementVec> {
        self.array_data().map(|data| &data.elements)
    }

    /// The element vector for arbitrary mutation. The caller's writes are
    /// unknown, so every card becomes dirty; precise single-slot stores use
    /// [`Self::overwrite_array_element`] and [`Self::mark_element_card`].
    pub(crate) fn array_elements_mut(&mut self) -> Option<&mut ElementVec> {
        self.array_data_mut().map(|data| {
            data.dirty_cards = None;
            &mut data.elements
        })
    }

    /// Record that element `index` may now reference a nursery object. A
    /// no-op while every card is already dirty.
    #[inline]
    pub(crate) fn mark_element_card(&mut self, index: usize) {
        let Some(bits) = self
            .array_data_mut()
            .and_then(|data| data.dirty_cards.as_deref_mut())
        else {
            return;
        };
        mark_card_bit(bits, index);
    }

    /// Keeps the card bitmap: the caller ran `gc_write_barrier_element`.
    #[inline]
    pub(crate) fn overwrite_array_element(&mut self, index: usize, value: &JsValue) -> bool {
        let Some(slot) = self
            .array_data_mut()
            .and_then(|data| data.elements.get_mut(index))
        else {
            return false;
        };
        *slot = value.clone();
        true
    }

    /// Keeps the card bitmap: the caller ran `gc_write_barrier_element` per index.
    pub(crate) fn extend_array_elements_precise(&mut self, values: &[JsValue]) {
        if let Some(data) = self.array_data_mut() {
            data.elements.extend_from_slice(values);
        }
    }

    pub(crate) fn array_elements_and_cards(&self) -> Option<(&[JsValue], Option<&[u64]>)> {
        self.array_data().map(|data| {
            (
                &data.elements[..],
                data.dirty_cards.as_deref().map(Vec::as_slice),
            )
        })
    }

    /// Whether a minor collection scans this object's bulk storage (an
    /// Array's elements, a strong Map's or Set's entries) card by card.
    /// Weak containers are not carded: they are re-scanned whole.
    #[inline]
    pub(crate) fn has_card_tracking(&self) -> bool {
        match &self.kind {
            ObjectKind::Array(_) => true,
            ObjectKind::Map(_) => !self.is_weak_map(),
            ObjectKind::Set(_) => !self.is_weak_set(),
            _ => false,
        }
    }

    /// Called after a scan of every element or slot: only `cards` still hold
    /// a young reference.
    pub(crate) fn set_dirty_cards(&mut self, cards: &[usize]) {
        if !self.has_card_tracking() {
            return;
        }
        match &mut self.kind {
            ObjectKind::Array(data) => {
                let bits = data.dirty_cards.get_or_insert_with(Box::default);
                fill_card_bits(bits, data.elements.len(), cards);
            }
            ObjectKind::Map(entries) => entries.set_dirty_cards(cards),
            ObjectKind::Set(entries) => entries.set_dirty_cards(cards),
            _ => {}
        }
    }

    pub(crate) fn mark_all_cards_dirty(&mut self) {
        match &mut self.kind {
            ObjectKind::Array(data) => data.dirty_cards = None,
            ObjectKind::Map(entries) => entries.mark_all_cards_dirty(),
            ObjectKind::Set(entries) => entries.mark_all_cards_dirty(),
            _ => {}
        }
    }

    /// After a major collection nothing is young, so a large object's bitmap
    /// starts empty. Smaller ones are not tracked by card.
    pub(crate) fn reset_cards_after_major(&mut self) {
        if !self.has_card_tracking() {
            return;
        }
        match &mut self.kind {
            ObjectKind::Array(data) => {
                data.dirty_cards = (data.elements.len() > CARD_ELEMENTS).then(Box::default);
            }
            ObjectKind::Map(entries) => entries.reset_cards_after_major(),
            ObjectKind::Set(entries) => entries.reset_cards_after_major(),
            _ => {}
        }
    }

    /// Create a default data property at the end of a dense Array without
    /// storing the same index in the ordinary property map. Undefined values
    /// need a descriptor there because the element vector uses undefined as
    /// its hole marker.
    pub(crate) fn try_append_dense_array_data_property(
        &mut self,
        key: &JsPropertyKey,
        value: &JsValue,
    ) -> bool {
        if value.is_undefined()
            || !self.extensible
            || !is_array_index_property_key(key)
            || self.properties.contains_key(key)
        {
            return false;
        }
        let index = parse_array_index(key).unwrap() as usize;
        self.push_dense_array_element(index, value)
    }

    /// Appends `value` as element `index` when `index` is exactly the current
    /// element count and a non-writable `length` does not forbid growth.
    /// Callers have already checked the value, extensibility and that no
    /// descriptor owns `index`.
    fn push_dense_array_element(&mut self, index: usize, value: &JsValue) -> bool {
        let pushed = self.push_dense_array_element_raw(index, value);
        if pushed {
            self.mark_all_cards_dirty();
        }
        pushed
    }

    /// [`Self::push_dense_array_element`], leaving the card bitmap alone: the
    /// caller has already marked the new element's card.
    fn push_dense_array_element_raw(&mut self, index: usize, value: &JsValue) -> bool {
        if self
            .array_elements()
            .is_none_or(|elements| elements.len() != index)
        {
            return false;
        }
        let Some(length_desc) = self.properties.get("length") else {
            return false;
        };
        let length = length_desc
            .value()
            .and_then(JsValue::as_number)
            .unwrap_or(0.0);
        if index as f64 >= length && length_desc.writable() == Some(false) {
            return false;
        }

        self.array_data_mut().unwrap().elements.push(value.clone());
        if index as f64 >= length {
            self.properties
                .get_mut("length")
                .unwrap()
                .set_value(Some(JsValue::number((index + 1) as f64)));
        }
        self.shape_id = fresh_shape_id();
        true
    }

    /// Whether this is an exotic Array with no own property besides a
    /// writable data `length` equal to `len`, and exactly `len` backing
    /// elements. Every `undefined` slot is then a hole (a present `undefined`
    /// would own a descriptor in `properties`) and every other slot is a
    /// default-attribute data property, so element reads and writes cannot
    /// reach a getter, setter, trap or non-default attribute.
    #[inline]
    pub(crate) fn is_plain_dense_array(&self, len: usize) -> bool {
        let Some(elements) = self.array_elements() else {
            return false;
        };
        elements.len() == len && self.properties.len() == 1 && self.has_writable_array_length(len)
    }

    /// Whether `length` is an own writable data property equal to `len`.
    #[inline]
    pub(crate) fn has_writable_array_length(&self, len: usize) -> bool {
        self.properties.get("length").is_some_and(|d| {
            d.writable() != Some(false)
                && d.value().and_then(JsValue::as_number) == Some(len as f64)
        })
    }

    /// Sets the array's `length` data property and invalidates cached shapes,
    /// after a structural change made directly to the element vector.
    #[inline]
    pub(crate) fn set_dense_array_length(&mut self, len: usize) {
        if let Some(d) = self.properties.get_mut("length") {
            d.set_value(Some(JsValue::number(len as f64)));
        }
        self.shape_id = fresh_shape_id();
    }

    /// `CreateDataProperty(A, ToString(index), value)` for the next index of a
    /// plain dense Array: appends in place when `index` is exactly the current
    /// element count. Mirrors `try_append_dense_array_data_property` without
    /// the key round trip.
    pub(crate) fn try_append_dense_array_element(&mut self, index: usize, value: &JsValue) -> bool {
        if value.is_undefined()
            || !self.extensible
            || self.properties.len() != 1
            || index > 0xFFFF_FFFE
        {
            return false;
        }
        self.push_dense_array_element(index, value)
    }

    /// How `[[Set]](index, value)` can complete on this Array without the
    /// property MOP, or `None` when the generic path must run. Applies to an
    /// exotic Array that owns no array-index key outside its element vector,
    /// so every non-`undefined` slot is a writable, enumerable, configurable
    /// data property and every `undefined` slot is a hole. A present
    /// `undefined` is never stored here: the vector cannot tell it from a
    /// hole, so it needs the descriptor the generic path creates.
    #[inline(always)]
    pub(crate) fn dense_array_set_plan(
        &self,
        index: usize,
        value: &JsValue,
    ) -> Option<DenseArraySet> {
        if value.is_undefined() || self.properties.may_hold_index_key() {
            return None;
        }
        let elements = self.array_elements()?;
        match elements.get(index) {
            Some(slot) if !slot.is_undefined() => Some(DenseArraySet::Overwrite),
            slot => {
                if !self.extensible || index > elements.len() || index >= 0xFFFF_FFFF {
                    return None;
                }
                let length = self
                    .properties
                    .get("length")?
                    .value()
                    .and_then(JsValue::as_number)?;
                // A hole inside the vector must already be below `length`;
                // an append may grow it, which a read-only `length` forbids.
                if slot.is_some() && index as f64 >= length {
                    return None;
                }
                Some(DenseArraySet::Create)
            }
        }
    }

    /// Performs the write `plan` described. `dense_array_set_plan` must have
    /// returned `plan` for the same `index` and `value`, and for `Create` the
    /// caller must have verified that no prototype owns `index`. The card
    /// bitmap is left alone, so the caller must have run
    /// `Interpreter::gc_write_barrier_element` for `index` and `value`.
    #[inline]
    pub(crate) fn apply_dense_array_set(
        &mut self,
        index: usize,
        value: &JsValue,
        plan: DenseArraySet,
    ) -> bool {
        let creating = plan == DenseArraySet::Create;
        if creating && self.array_elements().is_some_and(|e| e.len() == index) {
            return self.push_dense_array_element_raw(index, value);
        }
        if !self.overwrite_array_element(index, value) {
            return false;
        }
        if creating {
            self.shape_id = fresh_shape_id();
        }
        true
    }

    pub(crate) fn array_extra_string_property_order(&self) -> Option<&[JsPropertyKey]> {
        self.array_data().map(|data| {
            data.extra_string_property_order
                .as_deref()
                .map_or(&[][..], Vec::as_slice)
        })
    }

    /// Whether property creation must be mirrored into the Array's
    /// `extra_string_property_order`.
    #[inline]
    fn tracks_array_keys(&self) -> bool {
        self.array_data().is_some()
    }

    /// Mirror the creation of `key` into an Array's extra-string-key list.
    /// Call it once per newly created own property, and only on Arrays
    /// (`tracks_array_keys`); creation order itself is the `PropertyMap`'s.
    fn record_array_key_creation(&mut self, key: &JsPropertyKey) {
        // `length` is permanently non-enumerable on Arrays and can never be
        // returned by the host hook. Excluding it keeps this Vec allocation-free
        // for Arrays that never receive an actual extra String property.
        if key.is_symbol() || key.eq_str("length") || is_array_index_property_key(key) {
            return;
        }
        if let Some(data) = self.array_data_mut() {
            data.extra_string_property_order
                .get_or_insert_with(Box::default)
                .push(key.clone());
        }
    }

    /// Insert or replace `key`, keeping an Array's extra-string-key list in
    /// step. One key lookup decides between replacing and appending.
    #[inline]
    fn insert_tracked(&mut self, key: JsPropertyKey, desc: PropertyDescriptor) {
        if self.tracks_array_keys() {
            let tracked = key.clone();
            if self.properties.insert(key, desc).is_none() {
                self.record_array_key_creation(&tracked);
            }
        } else {
            self.properties.insert(key, desc);
        }
    }

    /// [`insert_tracked`] for a key the caller knows is absent.
    #[inline]
    fn push_fresh_tracked(&mut self, key: JsPropertyKey, desc: PropertyDescriptor) {
        if self.tracks_array_keys() {
            let tracked = key.clone();
            self.properties.push_fresh(key, desc);
            self.record_array_key_creation(&tracked);
        } else {
            self.properties.push_fresh(key, desc);
        }
    }

    /// True iff this is an *active* (non-revoked) proxy. Preserves pre-bundling semantics.
    pub(crate) fn is_proxy(&self) -> bool {
        self.proxy().is_some_and(|p| !p.revoked)
    }

    /// Replace this object's iterator state, reusing the existing allocation
    /// when it already is an iterator (generators swap their state on every
    /// resumption).
    pub(crate) fn set_iterator_state(&mut self, state: IteratorState) {
        match &mut self.kind {
            ObjectKind::Iterator(slot) => **slot = state,
            kind => *kind = ObjectKind::Iterator(Box::new(state)),
        }
    }

    pub(crate) fn set_bytecode_cache(&mut self, state: &super::bytecode::BytecodeCacheState) {
        if let Some(JsFunction::User { bytecode_cache, .. }) = self.callable.as_deref_mut() {
            *bytecode_cache = state.clone();
        }
    }

    /// A WeakMap instance (as opposed to `WeakMap.prototype`, which shares the
    /// brand but holds no entries).
    #[inline]
    pub(crate) fn is_weak_map(&self) -> bool {
        matches!(self.kind, ObjectKind::Map(_)) && self.class_name == "WeakMap"
    }

    #[inline]
    pub(crate) fn is_weak_set(&self) -> bool {
        matches!(self.kind, ObjectKind::Set(_)) && self.class_name == "WeakSet"
    }

    #[inline]
    pub(crate) fn is_weak_container(&self) -> bool {
        self.is_weak_map() || self.is_weak_set()
    }

    /// True iff this object has been revoked.
    pub(crate) fn is_proxy_revoked(&self) -> bool {
        self.proxy().is_some_and(|p| p.revoked)
    }

    /// Target id for an active proxy, `None` otherwise (including revoked).
    pub(crate) fn proxy_target_id(&self) -> Option<u64> {
        self.proxy().and_then(|p| p.target_id)
    }

    /// True iff this object is a class constructor of any flavor.
    pub(crate) fn is_class_constructor(&self) -> bool {
        !matches!(self.constructor_kind, ConstructorKind::Function)
    }

    /// True iff this object is a derived class constructor (explicit or default).
    pub(crate) fn is_derived_class_constructor(&self) -> bool {
        matches!(
            self.constructor_kind,
            ConstructorKind::DerivedClass | ConstructorKind::DefaultDerivedClass
        )
    }

    /// True iff this is a synthesized default derived constructor.
    pub(crate) fn is_default_derived_constructor(&self) -> bool {
        matches!(self.constructor_kind, ConstructorKind::DefaultDerivedClass)
    }

    /// An engine-internal slot (a cached bound function, say) that script can
    /// neither enumerate, read nor overwrite. It lives with the private
    /// elements, which the collector already traces, so it is never an own
    /// property of the object.
    pub(crate) fn hidden_slot(&self, name: PrivateId) -> Option<JsValue> {
        match self.private_field(name) {
            Some(PrivateElement::Field(v)) => Some(v.clone()),
            _ => None,
        }
    }

    pub(crate) fn set_hidden_slot(&mut self, name: PrivateId, value: JsValue) {
        self.private_fields_mut()
            .insert(name, PrivateElement::Field(value));
    }

    #[inline]
    pub(crate) fn function_realm(&self) -> Option<usize> {
        (self.function_realm != NO_REALM).then_some(self.function_realm as usize)
    }

    #[inline]
    pub(crate) fn set_function_realm(&mut self, realm_id: usize) {
        debug_assert!(realm_id < NO_REALM as usize);
        self.function_realm = realm_id as u32;
    }

    #[inline]
    pub(crate) fn generator_realm_id(&self) -> Option<usize> {
        (self.generator_realm != NO_REALM).then_some(self.generator_realm as usize)
    }

    #[inline]
    pub(crate) fn set_generator_realm_id(&mut self, realm_id: usize) {
        debug_assert!(realm_id < NO_REALM as usize);
        self.generator_realm = realm_id as u32;
    }

    #[inline]
    pub(crate) fn extras_mut(&mut self) -> &mut ObjectExtras {
        self.extras.get_or_insert_with(Box::default)
    }

    #[inline]
    pub(crate) fn private_field(&self, name: PrivateId) -> Option<&PrivateElement> {
        self.extras.as_ref()?.private_fields.get(name)
    }

    #[inline]
    pub(crate) fn private_field_mut(&mut self, name: PrivateId) -> Option<&mut PrivateElement> {
        self.extras.as_mut()?.private_fields.get_mut(name)
    }

    #[inline]
    pub(crate) fn has_private_field(&self, name: PrivateId) -> bool {
        self.private_field(name).is_some()
    }

    pub(crate) fn private_fields_mut(&mut self) -> &mut PrivateFields {
        &mut self.extras_mut().private_fields
    }

    pub(crate) fn private_field_values(&self) -> impl Iterator<Item = &PrivateElement> {
        self.extras
            .iter()
            .flat_map(|extras| extras.private_fields.values())
    }

    #[inline]
    pub(crate) fn instance_field_defs(&self) -> &[InstanceFieldDef] {
        self.extras
            .as_ref()
            .map_or(&[], |extras| &extras.class_instance_field_defs)
    }

    #[inline]
    pub(crate) fn instance_field_defs_shared(&self) -> Option<Rc<Vec<InstanceFieldDef>>> {
        let defs = &self.extras.as_ref()?.class_instance_field_defs;
        (!defs.is_empty()).then(|| Rc::clone(defs))
    }

    pub(crate) fn instance_field_defs_mut(&mut self) -> &mut Vec<InstanceFieldDef> {
        Rc::make_mut(&mut self.extras_mut().class_instance_field_defs)
    }

    #[inline]
    pub(crate) fn gc_native_roots(&self) -> &[JsValue] {
        self.extras
            .as_ref()
            .map_or(&[], |extras| &extras.gc_native_roots)
    }

    pub(crate) fn gc_native_roots_mut(&mut self) -> &mut Vec<JsValue> {
        &mut self.extras_mut().gc_native_roots
    }

    /// Intl slot data (any of the Intl kinds).
    pub(crate) fn intl_data(&self) -> Option<&IntlData> {
        if let ObjectKind::Intl(ref i) = self.kind {
            Some(i.as_ref())
        } else {
            None
        }
    }

    /// Backing bytes for an ArrayBuffer / SharedArrayBuffer.
    pub(crate) fn arraybuffer_data(&self) -> Option<&Rc<RefCell<BufferData>>> {
        self.arraybuffer().map(|b| &b.data)
    }

    /// Detached cell for a regular ArrayBuffer. `None` for SAB or non-buffers.
    pub(crate) fn arraybuffer_detached(&self) -> Option<&Rc<Cell<bool>>> {
        self.arraybuffer().and_then(|b| b.detached.as_ref())
    }

    /// Max byte length for a resizable / growable AB; `None` for non-resizable.
    pub(crate) fn arraybuffer_max_byte_length(&self) -> Option<usize> {
        self.arraybuffer().and_then(|b| b.max_byte_length)
    }

    /// True iff this is a SharedArrayBuffer.
    pub(crate) fn arraybuffer_is_shared(&self) -> bool {
        self.arraybuffer().is_some_and(|b| b.is_shared)
    }

    /// True iff this ArrayBuffer is a view of a WebAssembly linear memory.
    pub(crate) fn arraybuffer_is_foreign(&self) -> bool {
        self.arraybuffer_data()
            .is_some_and(|d| d.borrow().is_foreign())
    }

    /// True iff this is an immutable ArrayBuffer (post-`sliceToImmutable`).
    pub(crate) fn arraybuffer_is_immutable(&self) -> bool {
        self.arraybuffer().is_some_and(|b| b.is_immutable)
    }

    /// Shadow-realm id for a ShadowRealm instance. Copy return, so cannot use the macro.
    pub(crate) fn shadow_realm_id(&self) -> Option<usize> {
        if let ObjectKind::ShadowRealm(id) = self.kind {
            Some(id)
        } else {
            None
        }
    }

    /// SAB shared inner state. `Some` iff this is a SharedArrayBuffer.
    pub(crate) fn sab_shared(&self) -> Option<&Arc<SharedBufferInner>> {
        self.arraybuffer().and_then(|b| b.sab_shared.as_ref())
    }

    /// Id of the ArrayBuffer wrapper object that backs this TypedArray or DataView,
    /// pulled from whichever kind slot is populated. `None` for non-view objects.
    pub(crate) fn view_buffer_object_id(&self) -> Option<u64> {
        self.typed_array_info()
            .as_ref()
            .and_then(|ta| ta.buffer_object_id)
            .or_else(|| self.data_view_info().and_then(|dv| dv.buffer_object_id))
    }

    fn string_exotic_value<K: PropertyKeyLike + ?Sized>(&self, key: &K) -> Option<JsValue> {
        let key = key.as_property_key_str()?;
        if self.class_name == "String"
            && let Some(s) = self.primitive_value.as_ref().and_then(JsValue::as_string)
        {
            let units = s.as_units();
            if key == "length" {
                return Some(JsValue::number(units.len() as f64));
            }
            if let Some(idx) = string_exotic_index(key, units.len()) {
                return Some(JsValue::string(crate::types::JsString::from_vec(vec![
                    units[idx],
                ])));
            }
        }
        None
    }

    // `get_property` and `get_property_descriptor` (chain-walking) have moved
    // to `Interpreter::get_property_on_id` / `get_property_descriptor_on_id`.
    // Callers that want own-property-only lookup use `own_property_lookup` /
    // `own_property_descriptor_lookup` directly on this `JsObjectData`.

    // Like get_property_descriptor but without prototype chain walk.
    // Includes parameter_map and array_elements handling.
    pub(crate) fn get_own_property_full<K: PropertyKeyLike + ?Sized>(
        &self,
        key: &K,
    ) -> Option<PropertyDescriptor> {
        if let Some(desc) = self.properties.get(key) {
            let mut d = desc.to_descriptor();
            if let Some(map) = self.parameter_map()
                && let Some(key_str) = key.as_property_key_str()
                && let Some(val) = map.value(key_str)
            {
                d.value = Some(val);
            }
            return Some(d);
        }
        self.get_own_exotic_property(key)
    }

    /// [`Self::get_own_property_full`] without its `properties` lookup: Array
    /// elements, TypedArray indices and String-wrapper `length`/indices.
    pub(crate) fn get_own_exotic_property<K: PropertyKeyLike + ?Sized>(
        &self,
        key: &K,
    ) -> Option<PropertyDescriptor> {
        if let Some(elems) = self.array_elements()
            && let Some(key_str) = key.as_property_key_str()
            && let Some(idx) = array_element_index(key_str)
            && idx < elems.len()
            && !elems[idx].is_undefined()
        {
            return Some(PropertyDescriptor {
                value: Some(elems[idx].clone()),
                writable: Some(true),
                enumerable: Some(true),
                configurable: Some(true),
                get: None,
                set: None,
            });
        }
        if let Some(ta) = self.typed_array_info()
            && let Some(index) = canonical_numeric_index_string(key)
        {
            if is_valid_integer_index(ta, index) {
                return Some(PropertyDescriptor {
                    value: Some(typed_array_get_index(ta, index as usize)),
                    writable: Some(true),
                    enumerable: Some(true),
                    configurable: Some(true),
                    get: None,
                    set: None,
                });
            }
            return None;
        }
        if let Some(s) = self.primitive_value.as_ref().and_then(JsValue::as_string)
            && self.class_name == "String"
        {
            let units = s.as_units();
            if key.as_property_key_bytes() == b"length" {
                return Some(PropertyDescriptor {
                    value: Some(JsValue::number(units.len() as f64)),
                    writable: Some(false),
                    enumerable: Some(false),
                    configurable: Some(false),
                    get: None,
                    set: None,
                });
            }
            if let Some(idx) = string_exotic_index(key, units.len()) {
                return Some(PropertyDescriptor {
                    value: Some(JsValue::string(crate::types::JsString::from_vec(vec![
                        units[idx],
                    ]))),
                    writable: Some(false),
                    enumerable: Some(true),
                    configurable: Some(false),
                    get: None,
                    set: None,
                });
            }
        }
        None
    }

    pub(crate) fn get_own_property<K: PropertyKeyLike + ?Sized>(
        &self,
        key: &K,
    ) -> Option<PropertyDescriptor> {
        // Module namespace exotic: §10.4.6.4 [[GetOwnProperty]]
        if let Some(ns_data) = self.module_namespace()
            && let Some(key_str) = key.as_property_key_str()
        {
            if ns_data.export_names.contains(key_str) {
                let val = if let Some(binding_name) = ns_data.export_to_binding.get(key_str) {
                    ns_data
                        .env
                        .borrow()
                        .get(binding_name)
                        .unwrap_or(JsValue::UNDEFINED)
                } else {
                    JsValue::UNDEFINED
                };
                return Some(PropertyDescriptor {
                    value: Some(val),
                    writable: Some(true),
                    enumerable: Some(true),
                    configurable: Some(false),
                    get: None,
                    set: None,
                });
            }
            // Not an export and not a symbol - check properties (@@toStringTag etc.)
            return self.properties.get(key).map(PropertyRecord::to_descriptor);
        }
        // Symbol keys: fall through to ordinary
        if let Some(desc) = self.properties.get(key) {
            let mut d = desc.to_descriptor();
            // Mapped arguments: update value from the live binding (§10.4.4.1)
            if let Some(map) = self.parameter_map()
                && let Some(key_str) = key.as_property_key_str()
                && let Some(val) = map.value(key_str)
            {
                d.value = Some(val);
            }
            // OrdinaryGetOwnProperty: complete accessor descriptors
            if d.is_accessor_descriptor() {
                if d.get.is_none() {
                    d.get = Some(JsValue::UNDEFINED);
                }
                if d.set.is_none() {
                    d.set = Some(JsValue::UNDEFINED);
                }
            }
            return Some(d);
        }
        if let Some(elems) = self.array_elements()
            && let Some(idx) = parse_array_index(key)
            && (idx as usize) < elems.len()
            && !elems[idx as usize].is_undefined()
        {
            return Some(PropertyDescriptor {
                value: Some(elems[idx as usize].clone()),
                writable: Some(true),
                enumerable: Some(true),
                configurable: Some(true),
                get: None,
                set: None,
            });
        }
        // TypedArray: §10.4.5.1
        if let Some(ta) = self.typed_array_info()
            && let Some(index) = canonical_numeric_index_string(key)
        {
            if is_valid_integer_index(ta, index) {
                return Some(PropertyDescriptor {
                    value: Some(typed_array_get_index(ta, index as usize)),
                    writable: Some(true),
                    enumerable: Some(true),
                    configurable: Some(true),
                    get: None,
                    set: None,
                });
            }
            return None;
        }
        // String exotic: §10.4.3.1
        if self.class_name == "String"
            && let Some(s) = self.primitive_value.as_ref().and_then(JsValue::as_string)
        {
            if key.as_property_key_bytes() == b"length" {
                return Some(PropertyDescriptor::data(
                    JsValue::number(s.len() as f64),
                    false,
                    false,
                    false,
                ));
            }
            if let Some(idx) = string_exotic_index(key, s.len()) {
                return Some(PropertyDescriptor::data(
                    JsValue::string(JsString::from_vec(vec![s.as_units()[idx]])),
                    false,
                    true,
                    false,
                ));
            }
        }
        None
    }

    pub(crate) fn has_own_property<K: PropertyKeyLike + ?Sized>(&self, key: &K) -> bool {
        // Module namespace exotic: [[HasProperty]] checks export list
        if let Some(ns_data) = self.module_namespace()
            && let Some(key_str) = key.as_property_key_str()
        {
            return ns_data.export_names.contains(key_str);
        }
        if self.properties.contains_key(key) {
            return true;
        }
        if let Some(elems) = self.array_elements()
            && let Some(idx) = parse_array_index(key)
            && (idx as usize) < elems.len()
            && !elems[idx as usize].is_undefined()
        {
            return true;
        }
        // TypedArray: §10.4.5.2
        if let Some(ta) = self.typed_array_info()
            && let Some(index) = canonical_numeric_index_string(key)
        {
            return is_valid_integer_index(ta, index);
        }
        if self.class_name == "String"
            && let Some(s) = self.primitive_value.as_ref().and_then(JsValue::as_string)
        {
            if key.as_property_key_bytes() == b"length" {
                return true;
            }
            return string_exotic_index(key, s.len()).is_some();
        }
        false
    }

    // `enumerable_keys_with_proto` and `has_property` (chain-walking) have moved
    // to `Interpreter::enumerable_keys_with_proto_on_id` /
    // `Interpreter::has_property_on_id`. Own-only variants live as
    // `own_enumerable_keys_with_shadow` / `own_has_property` on this impl.

    pub(crate) fn define_own_property<K: Into<JsPropertyKey>>(
        &mut self,
        key: K,
        mut desc: PropertyDescriptor,
    ) -> bool {
        let key = intern_js_key(key.into());
        // String exotic §10.4.3.3 [[DefineOwnProperty]]: reject changes to character index properties
        if self.class_name == "String"
            && let Some(s) = self.primitive_value.as_ref().and_then(JsValue::as_string)
            && let Some(idx) = string_exotic_index(&key, s.len())
        {
            // String index property: {value: char, writable: false, enumerable: true, configurable: false}
            // Only allow if desc is compatible (no changes to value, not setting writable/configurable)
            if desc.configurable == Some(true) {
                return false;
            }
            if desc.enumerable == Some(false) {
                return false;
            }
            if desc.get.is_some() || desc.set.is_some() {
                return false;
            }
            if desc.writable == Some(true) {
                return false;
            }
            if let Some(ref v) = desc.value {
                let char_val =
                    JsValue::string(crate::types::JsString::from_vec(vec![s.as_units()[idx]]));
                if !same_value(v, &char_val) {
                    return false;
                }
            }
            return true;
        }
        // Module namespace exotic: §10.4.6.5 [[DefineOwnProperty]]
        if self.module_namespace().is_some() {
            if let Some(current) = self.get_own_property(&key) {
                // If every field is absent, return true
                if desc.value.is_none()
                    && desc.writable.is_none()
                    && desc.get.is_none()
                    && desc.set.is_none()
                    && desc.enumerable.is_none()
                    && desc.configurable.is_none()
                {
                    return true;
                }
                // Check each present field matches current
                if let Some(ref v) = desc.value
                    && let Some(ref cv) = current.value
                    && !same_value(v, cv)
                {
                    return false;
                }
                if let Some(w) = desc.writable
                    && current.writable != Some(w)
                {
                    return false;
                }
                if let Some(e) = desc.enumerable
                    && current.enumerable != Some(e)
                {
                    return false;
                }
                if let Some(c) = desc.configurable
                    && current.configurable != Some(c)
                {
                    return false;
                }
                if desc.get.is_some() || desc.set.is_some() {
                    return false;
                }
                return true;
            }
            return false;
        }
        // TypedArray: §10.4.5.3 [[DefineOwnProperty]]
        if self.typed_array_info().is_some()
            && let Some(index) = canonical_numeric_index_string(&key)
        {
            let ta = self.typed_array_info().unwrap();
            if !is_valid_integer_index(ta, index) {
                return false;
            }
            // If accessor descriptor, reject
            if desc.get.is_some() || desc.set.is_some() {
                return false;
            }
            if desc.configurable == Some(false) {
                return false;
            }
            if desc.enumerable == Some(false) {
                return false;
            }
            if desc.writable == Some(false) {
                return false;
            }
            if let Some(ref value) = desc.value {
                let ta_clone = ta.clone();
                typed_array_set_index(&ta_clone, index as usize, value);
            }
            return true;
        }
        // A bare `{ [[Value]] }` on a writable data property only replaces
        // the value (mapped arguments objects take the general path).
        if desc.writable.is_none()
            && desc.get.is_none()
            && desc.set.is_none()
            && desc.enumerable.is_none()
            && desc.configurable.is_none()
            && desc.value.is_some()
            && self.parameter_map().is_none()
            && let Some(record) = self.properties.get_mut(&key)
            && record.writable() == Some(true)
            && !record.is_accessor_descriptor()
        {
            record.set_value(desc.value);
            self.shape_id = fresh_shape_id();
            return true;
        }
        // Check array_elements for existing array index properties
        let current_from_array = if !self.properties.contains_key(&key) {
            if let Some(elems) = self.array_elements()
                && let Some(idx) = array_element_index(&key)
                && idx < elems.len()
                && !elems[idx].is_undefined()
            {
                Some(PropertyDescriptor {
                    value: Some(elems[idx].clone()),
                    writable: Some(true),
                    enumerable: Some(true),
                    configurable: Some(true),
                    get: None,
                    set: None,
                })
            } else {
                None
            }
        } else {
            None
        };

        if let Some(current) = self
            .properties
            .get(&key)
            .cloned()
            .or(current_from_array.map(PropertyRecord::from))
        {
            // §10.1.6.3 step 2: if every field of desc is absent, return true
            if desc.value.is_none()
                && desc.writable.is_none()
                && desc.get.is_none()
                && desc.set.is_none()
                && desc.enumerable.is_none()
                && desc.configurable.is_none()
            {
                return true;
            }

            if current.configurable() == Some(false) {
                if desc.configurable == Some(true) {
                    return false;
                }
                if desc.enumerable.is_some() && desc.enumerable != current.enumerable() {
                    return false;
                }

                let current_is_data = current.is_data_descriptor();
                let current_is_accessor = current.is_accessor_descriptor();
                let desc_is_data = desc.is_data_descriptor();
                let desc_is_accessor = desc.is_accessor_descriptor();

                // Cannot change between data and accessor on non-configurable
                if current_is_data && !current_is_accessor && desc_is_accessor && !desc_is_data {
                    return false;
                }
                if current_is_accessor && !current_is_data && desc_is_data && !desc_is_accessor {
                    return false;
                }

                if current_is_data && !current_is_accessor {
                    // Non-configurable data property
                    if current.writable() == Some(false) {
                        if desc.writable == Some(true) {
                            return false;
                        }
                        if let Some(ref new_val) = desc.value {
                            if let Some(cur_val) = current.value() {
                                if !same_value(new_val, cur_val) {
                                    return false;
                                }
                            } else {
                                return false;
                            }
                        }
                    }
                } else if current_is_accessor {
                    // Non-configurable accessor property
                    if let Some(ref new_get) = desc.get {
                        let cur_get = current.getter().unwrap_or(JsValue::undefined_ref());
                        if !same_value(new_get, cur_get) {
                            return false;
                        }
                    }
                    if let Some(ref new_set) = desc.set {
                        let cur_set = current.setter().unwrap_or(JsValue::undefined_ref());
                        if !same_value(new_set, cur_set) {
                            return false;
                        }
                    }
                }
            }

            // Precompute type info before consuming desc
            let desc_is_data = desc.is_data_descriptor();
            let desc_is_accessor = desc.is_accessor_descriptor();
            let desc_has_get = desc.get.is_some();
            let desc_has_set = desc.set.is_some();
            let desc_writable = desc.writable;
            let desc_has_value = desc.value.is_some();

            // §10.4.4.3 step 4: if isMapped && data desc && writable:false but no value,
            // inject the current live binding value so the stored property gets the right value.
            if desc_is_data
                && !desc_is_accessor
                && desc_writable == Some(false)
                && !desc_has_value
                && let Some(key_str) = key.as_str()
                && let Some(map) = self.parameter_map()
                && let Some(live_val) = map.value(key_str)
            {
                desc.value = Some(live_val);
            }

            let current_is_data = current.is_data_descriptor();
            let current_is_accessor = current.is_accessor_descriptor();

            // Save original desc value for step 7 parameter_map handling (before it's moved into merged)
            let desc_value_for_step7: Option<JsValue> = if desc_has_value {
                desc.value.clone()
            } else {
                None
            };

            // Build merged descriptor
            let merged = if desc_is_data
                && !desc_is_accessor
                && current_is_accessor
                && !current_is_data
            {
                // Changing from accessor to data
                PropertyDescriptor {
                    value: desc.value.or(Some(JsValue::UNDEFINED)),
                    writable: desc.writable.or(Some(false)),
                    get: None,
                    set: None,
                    enumerable: desc.enumerable.or(current.enumerable()),
                    configurable: desc.configurable.or(current.configurable()),
                }
            } else if desc_is_accessor && !desc_is_data && current_is_data && !current_is_accessor {
                // Changing from data to accessor
                PropertyDescriptor {
                    value: None,
                    writable: None,
                    get: desc.get.or(Some(JsValue::UNDEFINED)),
                    set: desc.set.or(Some(JsValue::UNDEFINED)),
                    enumerable: desc.enumerable.or(current.enumerable()),
                    configurable: desc.configurable.or(current.configurable()),
                }
            } else {
                // Normal merge: unspecified fields retain current values
                // but don't leak accessor fields into data or vice versa
                let result_is_accessor = if desc_is_accessor {
                    true
                } else if desc_is_data {
                    false
                } else {
                    current_is_accessor
                };
                if result_is_accessor {
                    PropertyDescriptor {
                        value: None,
                        writable: None,
                        get: desc.get.or(current.getter().cloned()),
                        set: desc.set.or(current.setter().cloned()),
                        enumerable: desc.enumerable.or(current.enumerable()),
                        configurable: desc.configurable.or(current.configurable()),
                    }
                } else {
                    PropertyDescriptor {
                        value: desc.value.or(current.value().cloned()),
                        writable: desc.writable.or(current.writable()),
                        get: None,
                        set: None,
                        enumerable: desc.enumerable.or(current.enumerable()),
                        configurable: desc.configurable.or(current.configurable()),
                    }
                }
            };

            // An Array element can be current without a map entry, in which
            // case this descriptor creates one.
            let ikey = key.clone();
            // NOTE: Array length shrinking semantics (ArraySetLength §10.4.2.4) are
            // handled by array_set_length() in mod.rs, which calls this function.
            // Do NOT duplicate that logic here.
            // Save key before it's moved into insert
            let key_for_step7 = if self.parameter_map().is_some() {
                key.as_str().map(str::to_string)
            } else {
                None
            };
            self.insert_tracked(ikey, merged);

            // §10.4.4.3 step 7: post-define parameter map handling
            if let Some(key_ref) = key_for_step7
                && let Some(map) = self.parameter_map_mut()
                && map.contains_key(&key_ref)
            {
                if desc_has_get || desc_has_set {
                    // Accessor descriptor: delete mapping
                    map.remove(&key_ref);
                } else {
                    // Data descriptor
                    if desc_has_value && let Some(ref val) = desc_value_for_step7 {
                        // Update the live binding with the new value
                        map.set_value(&key_ref, val);
                    }
                    if desc_writable == Some(false) {
                        // Make non-writable: delete mapping
                        map.remove(&key_ref);
                    }
                }
            }
        } else {
            if !self.extensible {
                return false;
            }
            // Handle parameter map for new properties
            if let Some(map) = self.parameter_map()
                && let Some(key_str) = key.as_str()
                && let Some(ref val) = desc.value
            {
                map.set_value(key_str, val);
            }
            let ikey = key;
            // For new property, fill in defaults per spec
            let is_accessor = desc.is_accessor_descriptor();
            let new_desc = PropertyDescriptor {
                value: desc.value.or(if !is_accessor {
                    Some(JsValue::UNDEFINED)
                } else {
                    None
                }),
                writable: desc
                    .writable
                    .or(if !is_accessor { Some(false) } else { None }),
                get: desc.get,
                set: desc.set,
                enumerable: desc.enumerable.or(Some(false)),
                configurable: desc.configurable.or(Some(false)),
            };
            self.insert_tracked(ikey, new_desc);
        }
        // Issue #71 (Step 5): reaching this point means either an existing
        // property was redefined (line ~2296 `properties.insert(key, merged)`)
        // or a new property was added (line ~2350). Both are structural
        // mutations. Earlier `return true` paths (String exotic equality
        // check, module-namespace no-op, TypedArray element write,
        // all-absent-desc) are non-mutating and intentionally skip this bump.
        self.shape_id = fresh_shape_id();
        true
    }

    pub(crate) fn set_property_value<K: PropertyKeyLike + ?Sized>(
        &mut self,
        key: &K,
        value: JsValue,
    ) -> bool {
        if let Some(ta) = self.typed_array_info()
            && let Some(index) = canonical_numeric_index_string(key)
        {
            if is_valid_integer_index(ta, index) {
                let ta_clone = ta.clone();
                return typed_array_set_index(&ta_clone, index as usize, &value);
            }
            return false;
        }
        // ArraySetLength (spec §10.4.2.1): reducing length deletes configurable properties
        if self.class_name == "Array" && key.as_property_key_bytes() == b"length" {
            // Check if length is writable before allowing set
            let len_writable = self
                .properties
                .get("length")
                .map(|d| d.writable() != Some(false))
                .unwrap_or(true);
            if !len_writable {
                return false;
            }
            if let Some(new_len_f) = value.as_number() {
                let new_len_u32 = new_len_f as u32;
                if (new_len_u32 as f64) != new_len_f {
                    // ArraySetLength §10.4.2.4 step 5: invalid length
                    return false;
                }
                let mut actual_new_len = new_len_u32;
                let old_len = self
                    .properties
                    .get("length")
                    .and_then(|d| d.value())
                    .and_then(JsValue::as_number)
                    .map(|n| n as u32);
                if let Some(old_len) = old_len
                    && new_len_u32 < old_len
                {
                    let mut idx_keys: Vec<(u32, String)> = self
                        .properties
                        .keys()
                        .filter_map(|k| {
                            k.parse::<u64>()
                                .ok()
                                .filter(|&idx| idx <= 0xFFFF_FFFE && k.eq_str(&idx.to_string()))
                                .map(|idx| idx as u32)
                                .filter(|&idx| idx >= new_len_u32)
                                .map(|idx| (idx, k.to_string()))
                        })
                        .collect();
                    idx_keys.sort_by_key(|a| std::cmp::Reverse(a.0));

                    let mut did_remove = false;
                    for (idx, k) in &idx_keys {
                        let is_non_configurable = self
                            .properties
                            .get(k.as_str())
                            .is_some_and(|d| d.configurable() == Some(false));
                        if is_non_configurable {
                            actual_new_len = idx + 1;
                            break;
                        } else {
                            self.remove_property(k.as_str());
                            did_remove = true;
                        }
                    }
                    let mut did_truncate = false;
                    if let Some(elements) = self.array_elements_mut() {
                        let prev_len = elements.len();
                        elements.truncate(actual_new_len as usize);
                        did_truncate = elements.len() < prev_len;
                    }
                    if did_remove || did_truncate {
                        // Issue #71 (Step 5): Array length truncation deletes
                        // configurable indexed properties / shrinks
                        // array_elements — both are structural mutations.
                        self.shape_id = fresh_shape_id();
                    }
                }
                if let Some(desc) = self.properties.get_mut("length") {
                    desc.set_value(Some(JsValue::number(actual_new_len as f64)));
                    return true;
                }
            }
        }
        if let Some(map) = self.parameter_map()
            && let Some(key_str) = key.as_property_key_str()
        {
            map.set_value(key_str, &value);
        }
        if self.class_name == "Array"
            && !value.is_undefined()
            && !self.properties.contains_key(key)
            && let Some(idx_u32) = parse_array_index(key)
            && self.array_elements().is_some()
        {
            let idx = idx_u32 as usize;
            let old_len = self
                .properties
                .get("length")
                .and_then(|d| d.value())
                .and_then(JsValue::as_number)
                .map(|n| n as u32)
                .unwrap_or(0);
            let length_writable = self
                .properties
                .get("length")
                .map(|d| d.writable() != Some(false))
                .unwrap_or(true);
            let extensible = self.extensible;
            let elements = self.array_elements_mut().unwrap();
            if idx <= elements.len() + 1024 {
                let existed = idx < elements.len() && !elements[idx].is_undefined();
                if !existed && !extensible {
                    return false;
                }
                if idx_u32 >= old_len && !length_writable {
                    return false;
                }
                if idx < elements.len() {
                    elements[idx] = value;
                } else {
                    while elements.len() < idx {
                        elements.push(JsValue::UNDEFINED);
                    }
                    elements.push(value);
                }
                if idx_u32 >= old_len
                    && let Some(len_desc) = self.properties.get_mut("length")
                {
                    len_desc.set_value(Some(JsValue::number((idx_u32 + 1) as f64)));
                }
                if !existed {
                    self.shape_id = fresh_shape_id();
                }
                return true;
            }
        }
        // An element held only in the vector is an existing own property: it
        // stays writable on a non-extensible array and needs no creation
        // checks. Everything else is validated before the vector or `length`
        // is touched, so a rejected write leaves the array unchanged.
        let mut vector_only_element = false;
        if self.array_elements().is_some()
            && let Some(idx) = array_element_index(key)
        {
            if let Some(desc) = self.properties.get(key) {
                if desc.writable() == Some(false) {
                    return false;
                }
            } else {
                vector_only_element = self
                    .array_elements()
                    .and_then(|elements| elements.get(idx))
                    .is_some_and(|slot| !slot.is_undefined());
                if !vector_only_element {
                    if !self.extensible {
                        return false;
                    }
                    if let Some(length) = self.properties.get("length")
                        && length.writable() == Some(false)
                        && length
                            .value()
                            .and_then(JsValue::as_number)
                            .is_none_or(|len| idx as f64 >= len)
                    {
                        return false;
                    }
                }
            }
        }
        // Keep array_elements in sync with properties for numeric indices
        if let Some(elements) = self.array_elements_mut()
            && let Some(key_str) = key.as_property_key_str()
            && let Some(idx) = array_element_index(key_str)
        {
            // Valid array indices are 0 to 2^32-2 (spec §6.1.7)
            if idx < elements.len() {
                elements[idx] = value.clone();
            } else if idx < 0xFFFF_FFFF && idx <= elements.len() + 1024 {
                // Extend for small gaps and valid array indices only
                while elements.len() < idx {
                    elements.push(JsValue::UNDEFINED);
                }
                elements.push(value.clone());
                // Issue #71 (Step 5): array_elements grew — structural mutation.
                self.shape_id = fresh_shape_id();
                // Only update length if idx+1 exceeds the current property length
                let new_idx_len = (idx + 1) as f64;
                if let Some(len_desc) = self.properties.get_mut("length")
                    && let Some(cur_len) = len_desc.value().and_then(JsValue::as_number)
                    && new_idx_len > cur_len
                {
                    len_desc.set_value(Some(JsValue::number(new_idx_len)));
                }
            } else if idx < 0xFFFF_FFFF {
                // Valid array index but too sparse for array_elements — update length
                let new_len = (idx + 1) as f64;
                if let Some(len_desc) = self.properties.get_mut("length")
                    && let Some(cur_len) = len_desc.value().and_then(JsValue::as_number)
                    && new_len > cur_len
                {
                    len_desc.set_value(Some(JsValue::number(new_len)));
                }
            }
            // idx >= 0xFFFFFFFF: not a valid array index, stored as named property
        }
        if let Some(desc) = self.properties.get_mut(key) {
            if desc.writable() == Some(false) {
                return false;
            }
            desc.set_value(Some(value));
            true
        } else {
            // String exotic: length and index properties are non-writable (§10.4.3.2)
            if self.class_name == "String"
                && let Some(s) = self.primitive_value.as_ref().and_then(JsValue::as_string)
                && (key.as_property_key_bytes() == b"length"
                    || string_exotic_index(key, s.len()).is_some())
            {
                return false;
            }
            if !self.extensible && !vector_only_element {
                return false;
            }
            let ikey = key.to_interned_property_key();
            self.push_fresh_tracked(ikey, PropertyDescriptor::data_default(value));
            // Issue #71 (Step 5): new own property added — structural mutation.
            // The earlier `properties.get_mut(key)` branch (in-place update)
            // does NOT bump; only this insert branch does.
            self.shape_id = fresh_shape_id();
            true
        }
    }

    /// The own record of `key` when an in-place named operation may touch it
    /// directly: Array `length` runs ArraySetLength, Array-index-like names
    /// live outside the map for Arrays, and exotic kinds answer some names
    /// themselves.
    #[inline]
    fn own_named_record_mut(
        &mut self,
        key: &JsPropertyKey,
        hint: Option<&Cell<u32>>,
    ) -> Result<Option<&mut PropertyRecord>, ()> {
        if self.kind.is_named_access_exotic()
            || key_may_be_array_index(key)
            || (matches!(self.kind, ObjectKind::Array(_)) && key.as_bytes() == b"length")
        {
            return Err(());
        }
        Ok(self.properties.get_mut_hinted(key, hint))
    }

    /// The own half of the VM's named store: overwrite an existing own
    /// writable data property in place. `Absent` means there is no such own
    /// property (the caller may still add one); `Bail` means the generic
    /// [[Set]] must run.
    #[inline]
    pub(crate) fn store_named_fast(
        &mut self,
        key: &JsPropertyKey,
        value: &JsValue,
        hint: Option<&Cell<u32>>,
    ) -> NamedStore {
        match self.own_named_record_mut(key, hint) {
            Err(()) => NamedStore::Bail,
            Ok(None) => NamedStore::Absent,
            Ok(Some(record)) => {
                if record.overwrite_writable_data(value) {
                    NamedStore::Stored
                } else {
                    NamedStore::Bail
                }
            }
        }
    }

    /// `obj.key += delta` on an own writable Number data property, in place;
    /// the old and new values.
    #[inline]
    pub(crate) fn add_to_named_number(
        &mut self,
        key: &JsPropertyKey,
        delta: f64,
        hint: Option<&Cell<u32>>,
    ) -> Option<(f64, f64)> {
        self.own_named_record_mut(key, hint)
            .ok()??
            .add_to_writable_number(delta)
    }

    /// Whether a store of a name this object does not own may create an
    /// ordinary own data property here, given a prototype chain that does not
    /// mention it. Arrays keep extra key bookkeeping and String wrappers own
    /// exotic names, so they take the generic path.
    #[inline]
    pub(crate) fn can_add_named_fast(&self) -> bool {
        self.extensible
            && !matches!(self.kind, ObjectKind::Array(_))
            && self.named_miss_is_ordinary()
    }

    /// Appends a new own data property whose absence the caller has just
    /// established.
    #[inline]
    pub(crate) fn add_named_fast(&mut self, key: &JsPropertyKey, value: &JsValue) {
        self.push_fresh_tracked(key.clone(), PropertyDescriptor::data_default(value.clone()));
        self.shape_id = fresh_shape_id();
    }

    /// Whether a named [[Get]] that misses this object's own properties can
    /// continue to its prototype without an exotic own lookup.
    #[inline]
    pub(crate) fn named_miss_is_ordinary(&self) -> bool {
        !self.kind.is_named_access_exotic()
            && !self
                .primitive_value
                .as_ref()
                .is_some_and(JsValue::is_string)
    }

    pub(crate) fn insert_value<K: Into<JsPropertyKey>>(&mut self, key: K, value: JsValue) {
        let key = intern_js_key(key.into());
        self.insert_tracked(key, PropertyDescriptor::data_default(value));
    }

    /// `insert_value` for a key that is already a stored property name of
    /// another object (so it shares that name's allocation and needs no
    /// interner round trip). One map lookup decides between replacing the
    /// descriptor and appending a new entry.
    pub(crate) fn insert_value_prekeyed(&mut self, key: JsPropertyKey, value: JsValue) {
        self.insert_tracked(key, PropertyDescriptor::data_default(value));
    }

    /// `insert_value_prekeyed` for a key the caller knows this object does
    /// not have yet.
    pub(crate) fn push_fresh_value(&mut self, key: JsPropertyKey, value: JsValue) {
        debug_assert!(!self.properties.contains_key(&key));
        self.push_fresh_tracked(key, PropertyDescriptor::data_default(value));
    }

    /// Append the `length` property of a freshly built Array. `length` is
    /// never part of the Array's extra-string-key tracking, so this skips the
    /// interner and duplicate scan of `insert_property`.
    pub(crate) fn push_fresh_array_length(
        &mut self,
        key: &JsPropertyKey,
        desc: PropertyDescriptor,
    ) {
        debug_assert!(key.eq_str("length") && self.array_data().is_some());
        self.properties.push_fresh(key.clone(), desc);
    }

    pub(crate) fn insert_builtin<K: Into<JsPropertyKey>>(&mut self, key: K, value: JsValue) {
        let key = intern_js_key(key.into());
        self.insert_tracked(key, PropertyDescriptor::data(value, true, false, true));
    }

    /// Append an own property whose key the caller knows is absent and
    /// already interned (a pre-built key from `CommonKeys`). Skips the key
    /// interning and duplicate scan of [`insert_property`]; only valid on
    /// objects with no array exotic key tracking.
    pub(crate) fn push_fresh_property(&mut self, key: &JsPropertyKey, desc: PropertyDescriptor) {
        debug_assert!(self.array_data().is_none());
        self.properties.push_fresh(key.clone(), desc);
    }

    pub(crate) fn insert_property<K: Into<JsPropertyKey>>(
        &mut self,
        key: K,
        desc: PropertyDescriptor,
    ) {
        let key = intern_js_key(key.into());
        self.insert_tracked(key, desc);
    }

    /// Remove `key` from the object, keeping an Array's extra-string-key list
    /// in step. The mirror of [`insert_property`]. Returns the removed
    /// descriptor, or `None` if the key was absent.
    pub(crate) fn remove_property<K: PropertyKeyLike + ?Sized>(
        &mut self,
        key: &K,
    ) -> Option<PropertyDescriptor> {
        let removed = self.properties.remove(key);
        if removed.is_some()
            && let Some(data) = self.array_data_mut()
            && let Some(order) = data.extra_string_property_order.as_mut()
        {
            order.retain(|k| k.as_bytes() != key.as_property_key_bytes());
        }
        removed
    }

    pub(crate) fn get_property_value<K: PropertyKeyLike + ?Sized>(
        &self,
        key: &K,
    ) -> Option<JsValue> {
        self.properties.get(key).and_then(|d| d.value().cloned())
    }

    /// Own-property path of `get_property` with no prototype traversal.
    /// Returns `Some(value)` if the key is resolved on this object (including
    /// module-namespace live bindings, parameter_map, array_elements, typed_array
    /// canonical numeric indices, and string exotic indices). Returns `None` if
    /// the caller should continue walking the prototype chain.
    pub(crate) fn own_property_lookup<K: PropertyKeyLike + ?Sized>(
        &self,
        key: &K,
    ) -> Option<JsValue> {
        if let Some(ns_data) = self.module_namespace()
            && let Some(key_str) = key.as_property_key_str()
            && let Some(binding_name) = ns_data.export_to_binding.get(key_str)
        {
            return Some(
                ns_data
                    .env
                    .borrow()
                    .get(binding_name)
                    .unwrap_or(JsValue::UNDEFINED),
            );
        }
        if let Some(map) = self.parameter_map()
            && let Some(key_str) = key.as_property_key_str()
            && let Some(val) = map.value(key_str)
        {
            return Some(val);
        }
        if let Some(desc) = self.properties.get(key) {
            if let Some(val) = desc.value() {
                return Some(val.clone());
            }
            return Some(JsValue::UNDEFINED);
        }
        if let Some(elems) = self.array_elements()
            && let Some(key_str) = key.as_property_key_str()
            && let Some(idx) = array_element_index(key_str)
            && idx < elems.len()
            && !elems[idx].is_undefined()
        {
            return Some(elems[idx].clone());
        }
        if let Some(ta) = self.typed_array_info()
            && let Some(index) = canonical_numeric_index_string(key)
        {
            if is_valid_integer_index(ta, index) {
                return Some(typed_array_get_index(ta, index as usize));
            }
            return Some(JsValue::UNDEFINED);
        }
        if let Some(val) = self.string_exotic_value(key) {
            return Some(val);
        }
        None
    }

    /// Own-property path of `get_property_descriptor` with no prototype traversal.
    /// Mirrors the pre-chain-walk branches of that method exactly (including
    /// OrdinaryGetOwnProperty accessor completion and TypedArray canonical index
    /// with `configurable: false` per §10.4.5.2).
    pub(crate) fn own_property_descriptor_lookup<K: PropertyKeyLike + ?Sized>(
        &self,
        key: &K,
    ) -> Option<PropertyDescriptor> {
        if let Some(desc) = self.properties.get(key) {
            let mut d = desc.to_descriptor();
            if let Some(map) = self.parameter_map()
                && let Some(key_str) = key.as_property_key_str()
                && let Some(val) = map.value(key_str)
            {
                d.value = Some(val);
            }
            if d.is_accessor_descriptor() {
                if d.get.is_none() {
                    d.get = Some(JsValue::UNDEFINED);
                }
                if d.set.is_none() {
                    d.set = Some(JsValue::UNDEFINED);
                }
            }
            return Some(d);
        }
        if let Some(elems) = self.array_elements()
            && let Some(key_str) = key.as_property_key_str()
            && let Some(idx) = array_element_index(key_str)
            && idx < elems.len()
            && !elems[idx].is_undefined()
        {
            return Some(PropertyDescriptor {
                value: Some(elems[idx].clone()),
                writable: Some(true),
                enumerable: Some(true),
                configurable: Some(true),
                get: None,
                set: None,
            });
        }
        if let Some(ta) = self.typed_array_info()
            && let Some(index) = canonical_numeric_index_string(key)
        {
            if is_valid_integer_index(ta, index) {
                return Some(PropertyDescriptor {
                    value: Some(typed_array_get_index(ta, index as usize)),
                    writable: Some(true),
                    enumerable: Some(true),
                    configurable: Some(false),
                    get: None,
                    set: None,
                });
            }
            return None;
        }
        if self.class_name == "String"
            && let Some(s) = self.primitive_value.as_ref().and_then(JsValue::as_string)
        {
            let units = s.as_units();
            if key.as_property_key_bytes() == b"length" {
                return Some(PropertyDescriptor {
                    value: Some(JsValue::number(units.len() as f64)),
                    writable: Some(false),
                    enumerable: Some(false),
                    configurable: Some(false),
                    get: None,
                    set: None,
                });
            }
            if let Some(idx) = string_exotic_index(key, units.len()) {
                return Some(PropertyDescriptor {
                    value: Some(JsValue::string(crate::types::JsString::from_vec(vec![
                        units[idx],
                    ]))),
                    writable: Some(false),
                    enumerable: Some(true),
                    configurable: Some(false),
                    get: None,
                    set: None,
                });
            }
        }
        None
    }

    /// Own-property path of `has_property` with no prototype traversal.
    /// Returns `Some(true)` if found on this object, `Some(false)` for
    /// canonical-numeric-index on a typed array that's out of range (per
    /// §10.4.5.2, typed arrays never consult the prototype for these),
    /// and `None` to continue walking the chain.
    pub(crate) fn own_has_property<K: PropertyKeyLike + ?Sized>(&self, key: &K) -> Option<bool> {
        if let Some(ta) = self.typed_array_info()
            && let Some(index) = canonical_numeric_index_string(key)
        {
            return Some(is_valid_integer_index(ta, index));
        }
        if self.has_own_property(key) {
            return Some(true);
        }
        None
    }

    /// Whether this object has any String-keyed property that for-in could
    /// enumerate, answered without the key sets the full walk builds.
    /// `None` when String/TypedArray/namespace exotics synthesise keys that
    /// this cheap scan does not model; callers then take the general path.
    pub(crate) fn has_enumerable_string_key_hint(&self) -> Option<bool> {
        if self.proxy().is_some()
            || self.module_namespace().is_some()
            || self.typed_array_info().is_some()
            || (self.class_name == "String"
                && self
                    .primitive_value
                    .as_ref()
                    .is_some_and(JsValue::is_string))
        {
            return None;
        }
        if let Some(elems) = self.array_elements()
            && elems.iter().any(|v| !v.is_undefined())
        {
            return Some(true);
        }
        Some(
            self.properties
                .iter()
                .any(|(k, d)| !k.is_symbol() && d.enumerable() != Some(false)),
        )
    }

    /// [[OwnPropertyKeys]] (§10.1.11.1) for an object whose keys are exactly
    /// its element vector plus its property map: integer indices ascending,
    /// then String keys in creation order, then (for `AllKeys`) Symbols.
    /// `None` for exotics that synthesise keys or run traps (Proxy, module
    /// namespace, TypedArray, String wrapper); callers fall back to the
    /// general path. Never runs user code.
    ///
    /// `EnumerableStrings` additionally drops non-enumerable properties, which
    /// is exact for callers that run no user code between the snapshot and
    /// their use of it.
    pub(crate) fn ordinary_own_property_keys(
        &self,
        filter: OwnKeyFilter,
    ) -> Option<Vec<JsPropertyKey>> {
        if self.proxy().is_some()
            || self.module_namespace().is_some()
            || self.typed_array_info().is_some()
            || (self.class_name == "String"
                && self
                    .primitive_value
                    .as_ref()
                    .is_some_and(JsValue::is_string))
        {
            return None;
        }
        let enumerable_only = filter == OwnKeyFilter::EnumerableStrings;
        let elements = self.array_elements();
        let mut index_keys: Vec<(u32, JsPropertyKey)> = Vec::new();
        let mut string_keys: Vec<JsPropertyKey> = Vec::with_capacity(self.properties.len());
        let mut symbol_keys: Vec<JsPropertyKey> = Vec::new();
        let mut order_has_index = false;

        if let Some(elems) = elements {
            for (i, value) in elems.iter().enumerate() {
                if value.is_undefined() || i > 0xFFFF_FFFE {
                    continue;
                }
                let key = JsPropertyKey::from_index(i);
                // A mirrored entry in the property map is emitted (with its
                // own attributes) by the property-map pass below.
                if self.properties.contains_key(&key) {
                    continue;
                }
                index_keys.push((i as u32, key));
            }
        }

        for (k, d) in self.properties.iter() {
            if k.is_symbol() {
                if filter == OwnKeyFilter::AllKeys {
                    symbol_keys.push(k.clone());
                }
                continue;
            }
            if enumerable_only && d.enumerable() == Some(false) {
                continue;
            }
            if let Some(n) = parse_array_index(k) {
                index_keys.push((n, k.clone()));
                order_has_index = true;
            } else {
                string_keys.push(k.clone());
            }
        }

        if order_has_index {
            index_keys.sort_by_key(|(n, _)| *n);
        }
        let mut keys: Vec<JsPropertyKey> =
            Vec::with_capacity(index_keys.len() + string_keys.len() + symbol_keys.len());
        keys.extend(index_keys.into_iter().map(|(_, k)| k));
        keys.append(&mut string_keys);
        keys.append(&mut symbol_keys);
        Some(keys)
    }

    /// `Some(enumerable)` when `key` is an own property, without cloning the
    /// descriptor. Mirrors `get_own_property`'s presence rules exactly.
    #[inline]
    pub(crate) fn own_property_enumerable<K: PropertyKeyLike + ?Sized>(
        &self,
        key: &K,
    ) -> Option<bool> {
        if self.module_namespace().is_none()
            && let Some(d) = self.properties.get(key)
        {
            return Some(d.enumerable() != Some(false));
        }
        self.get_own_property(key)
            .map(|d| d.enumerable != Some(false))
    }

    /// Own-property path of `enumerable_keys_with_proto`: returns the own
    /// enumerable keys in spec order (integer indices ascending, then string
    /// keys in insertion order) and the full shadow set of all own keys
    /// (enumerable + non-enumerable) so the caller can suppress inherited
    /// properties with matching names.
    pub(crate) fn own_enumerable_keys_with_shadow(
        &self,
    ) -> (Vec<JsPropertyKey>, HashSet<JsPropertyKey>) {
        let mut seen = HashSet::default();
        let mut index_keys: Vec<(u32, JsPropertyKey)> = Vec::new();
        let mut string_keys: Vec<JsPropertyKey> = Vec::new();

        if self.class_name == "String"
            && let Some(s) = self.primitive_value.as_ref().and_then(JsValue::as_string)
        {
            let utf16_len = s.len();
            for i in 0..utf16_len {
                let k = JsPropertyKey::from(i.to_string());
                if seen.insert(k.clone()) {
                    index_keys.push((i as u32, k));
                }
            }
        }

        if let Some(ta) = self.typed_array_info() {
            let len = ta.array_length;
            for i in 0..len {
                let k = JsPropertyKey::from(i.to_string());
                if seen.insert(k.clone()) {
                    index_keys.push((i as u32, k));
                }
            }
        }

        if let Some(elems) = self.array_elements() {
            for (i, value) in elems.iter().enumerate() {
                if value.is_undefined() || i > 0xFFFF_FFFE {
                    continue;
                }
                let k = JsPropertyKey::from(i.to_string());
                if self.properties.contains_key(&k) {
                    continue;
                }
                if seen.insert(k.clone()) {
                    index_keys.push((i as u32, k));
                }
            }
        }

        for (k, desc) in self.properties.iter() {
            if k.is_symbol() {
                continue;
            }
            let is_enumerable = desc.enumerable() != Some(false);
            if seen.insert(k.clone()) && is_enumerable {
                if let Some(idx) = parse_array_index(k) {
                    index_keys.push((idx, k.clone()));
                } else {
                    string_keys.push(k.clone());
                }
            }
        }

        index_keys.sort_by_key(|(idx, _)| *idx);
        let mut keys: Vec<JsPropertyKey> = index_keys.into_iter().map(|(_, k)| k).collect();
        keys.extend(string_keys);

        (keys, seen)
    }
}

// §7.1.4.1 CanonicalNumericIndexString
/// Check if a string is an array index (non-negative integer < 2^32-1).
pub(crate) fn parse_array_index<K: crate::types::PropertyKeyLike + ?Sized>(key: &K) -> Option<u32> {
    let key = key.as_property_key_str()?;
    if key.is_empty() {
        return None;
    }
    // Rust's integer parser also accepts a leading `+`, which ECMAScript
    // array indices never have.
    if !key.as_bytes()[0].is_ascii_digit() {
        return None;
    }
    // No leading zeros (except "0" itself)
    if key.len() > 1 && key.starts_with('0') {
        return None;
    }
    let n: u32 = key.parse().ok()?;
    // Must be < 2^32 - 1 (0xFFFFFFFF is not an array index)
    if n == u32::MAX {
        return None;
    }
    Some(n)
}

/// [`parse_array_index`] widened for indexing an element vector.
pub(crate) fn array_element_index<K: crate::types::PropertyKeyLike + ?Sized>(
    key: &K,
) -> Option<usize> {
    parse_array_index(key).map(|i| i as usize)
}

/// Exact Array-index predicate for internal property-key bookkeeping.
///
/// `parse_array_index` currently accepts Rust's leading-`+` integer spelling;
/// ECMAScript does not. Canonical Array indices always begin with an ASCII
/// digit, so keep that guard at the narrow call sites that require exactness.
pub(crate) fn is_array_index_property_key<K: crate::types::PropertyKeyLike + ?Sized>(
    key: &K,
) -> bool {
    key.as_property_key_bytes()
        .first()
        .is_some_and(u8::is_ascii_digit)
        && parse_array_index(key).is_some()
}

/// 1 to 15 ASCII digits, no leading zero (or exactly `"0"`): always its own
/// canonical numeric spelling, so the `f64` parse and `ToString` round trip are skipped.
fn plain_decimal_integer(key: &str) -> Option<f64> {
    let bytes = key.as_bytes();
    if bytes.is_empty() || bytes.len() > 15 || (bytes[0] == b'0' && bytes.len() > 1) {
        return None;
    }
    let mut value: u64 = 0;
    for &b in bytes {
        if !b.is_ascii_digit() {
            return None;
        }
        value = value * 10 + u64::from(b - b'0');
    }
    Some(value as f64)
}

pub(crate) fn canonical_numeric_index_string<K: crate::types::PropertyKeyLike + ?Sized>(
    key: &K,
) -> Option<f64> {
    let key = key.as_property_key_str()?;
    if let Some(n) = plain_decimal_integer(key) {
        return Some(n);
    }
    if key == "-0" {
        return Some(-0.0_f64);
    }
    let n: f64 = key.parse().ok()?;
    if crate::types::number_ops::to_string(n) == key {
        Some(n)
    } else {
        None
    }
}

/// §10.4.3 String-exotic own indexed-property test (the index branch of
/// StringGetOwnProperty).
///
/// Returns `Some(i)` when `key` is the CanonicalNumericIndexString of an
/// integral index `i` with `0 <= i < len`, i.e. an own indexed character
/// property of a String whose UTF-16 length is `len`. Returns `None` for
/// non-canonical spellings (`"01"`, `"+1"`, `"1.0"`), `-0`, negative,
/// non-integral or non-finite indices, and indices `>= len`.
///
/// One canonical predicate keeps every String-exotic MOP operation ([[Get]],
/// [[GetOwnProperty]], [[HasProperty]], [[DefineOwnProperty]], [[Set]],
/// [[Delete]]) in agreement, instead of `str::parse::<usize>()`, which wrongly
/// accepts a leading `+` and leading zeros. `len` is the UTF-16 length.
pub(crate) fn string_exotic_index<K: crate::types::PropertyKeyLike + ?Sized>(
    key: &K,
    len: usize,
) -> Option<usize> {
    let n = canonical_numeric_index_string(key)?;
    // IsIntegralNumber(index) is false, or index is -0𝔽 -> not an own index.
    if !n.is_finite() || n.fract() != 0.0 {
        return None;
    }
    if n == 0.0 && n.is_sign_negative() {
        return None;
    }
    // index < 0 or len <= index -> not an own index. Range-check before the
    // cast so a huge finite index can never saturate into a valid `usize`.
    if n < 0.0 || n >= len as f64 {
        return None;
    }
    Some(n as usize)
}

pub(crate) fn typed_array_length(ta: &TypedArrayInfo) -> usize {
    if ta.is_detached.get() {
        return 0;
    }
    if ta.is_length_tracking {
        let buf_borrow = ta.buffer.borrow();
        let buf_len = (*buf_borrow).len();
        let remaining = buf_len.saturating_sub(ta.byte_offset);
        remaining / ta.kind.bytes_per_element()
    } else {
        let buf_borrow = ta.buffer.borrow();
        let buf_len = (*buf_borrow).len();
        if ta.byte_offset + ta.byte_length > buf_len {
            0
        } else {
            ta.array_length
        }
    }
}

/// §10.4.5 IsTypedArrayFixedLength(O)
/// Returns true only if the TA has an explicit length on a non-resizable buffer,
/// or an explicit length on a SharedArrayBuffer (which can only grow, not shrink).
pub(crate) fn is_typed_array_fixed_length(ta: &TypedArrayInfo, buffer_obj: &JsObjectData) -> bool {
    if !ta.is_length_tracking && buffer_obj.arraybuffer_max_byte_length().is_none() {
        return true;
    }
    if !ta.is_length_tracking && buffer_obj.arraybuffer_is_shared() {
        return true;
    }
    false
}

pub(crate) fn is_typed_array_out_of_bounds(ta: &TypedArrayInfo) -> bool {
    if ta.is_detached.get() {
        return true;
    }
    let buf_borrow = ta.buffer.borrow();
    let buf_len = (*buf_borrow).len();
    if ta.is_length_tracking {
        ta.byte_offset > buf_len
    } else {
        ta.byte_offset + ta.byte_length > buf_len
    }
}

pub(crate) fn typed_array_byte_length(ta: &TypedArrayInfo) -> usize {
    typed_array_length(ta) * ta.kind.bytes_per_element()
}

// §10.4.5.14 IsValidIntegerIndex
pub(crate) fn is_valid_integer_index(ta: &TypedArrayInfo, index: f64) -> bool {
    if ta.is_detached.get() {
        return false;
    }
    if is_typed_array_out_of_bounds(ta) {
        return false;
    }
    if index.is_nan() || index.is_infinite() {
        return false;
    }
    if index != index.trunc() {
        return false;
    }
    if index.is_sign_negative() && index == 0.0 {
        return false; // -0
    }
    let len = typed_array_length(ta) as f64;
    if index < 0.0 || index >= len {
        return false;
    }
    true
}

/// Decode one typed-array element of `kind` from `buf` at byte `offset`.
///
/// The single source of truth for the byte->JsValue mapping shared by every
/// buffer flavour (shared and non-shared). Returns `undefined` when the element
/// would read past the end of `buf`, matching the out-of-bounds behaviour the
/// exotic `[[Get]]` requires.
fn decode_ta_element(kind: TypedArrayKind, buf: &[u8], offset: usize) -> JsValue {
    if offset + kind.bytes_per_element() > buf.len() {
        return JsValue::UNDEFINED;
    }
    match kind {
        TypedArrayKind::Int8 => JsValue::number(buf[offset] as i8 as f64),
        TypedArrayKind::Uint8 | TypedArrayKind::Uint8Clamped => JsValue::number(buf[offset] as f64),
        TypedArrayKind::Int16 => {
            let v = i16::from_ne_bytes([buf[offset], buf[offset + 1]]);
            JsValue::number(v as f64)
        }
        TypedArrayKind::Uint16 => {
            let v = u16::from_ne_bytes([buf[offset], buf[offset + 1]]);
            JsValue::number(v as f64)
        }
        TypedArrayKind::Int32 => {
            let v = i32::from_ne_bytes([
                buf[offset],
                buf[offset + 1],
                buf[offset + 2],
                buf[offset + 3],
            ]);
            JsValue::number(v as f64)
        }
        TypedArrayKind::Uint32 => {
            let v = u32::from_ne_bytes([
                buf[offset],
                buf[offset + 1],
                buf[offset + 2],
                buf[offset + 3],
            ]);
            JsValue::number(v as f64)
        }
        TypedArrayKind::Float32 => {
            let v = f32::from_ne_bytes([
                buf[offset],
                buf[offset + 1],
                buf[offset + 2],
                buf[offset + 3],
            ]);
            JsValue::number(v as f64)
        }
        TypedArrayKind::Float64 => {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&buf[offset..offset + 8]);
            JsValue::number(f64::from_ne_bytes(bytes))
        }
        TypedArrayKind::BigInt64 => {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&buf[offset..offset + 8]);
            JsValue::bigint(crate::types::JsBigInt::new(num_bigint::BigInt::from(
                i64::from_ne_bytes(bytes),
            )))
        }
        TypedArrayKind::BigUint64 => {
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&buf[offset..offset + 8]);
            JsValue::bigint(crate::types::JsBigInt::new(num_bigint::BigInt::from(
                u64::from_ne_bytes(bytes),
            )))
        }
        TypedArrayKind::Float16 => {
            let bits = u16::from_ne_bytes([buf[offset], buf[offset + 1]]);
            JsValue::number(crate::interpreter::builtins::typedarray::dv_f16_to_f64(
                bits,
            ))
        }
    }
}

/// Encode `value` as one typed-array element of `kind` into `buf` at byte
/// `offset`, applying the element's numeric coercion (ToInt8 .. ToNumber).
///
/// The mirror of [`decode_ta_element`]: the single source of truth for the
/// JsValue->byte mapping shared by every buffer flavour. Returns `false`
/// (writing nothing) when the element would write past the end of `buf`.
fn encode_ta_element(kind: TypedArrayKind, buf: &mut [u8], offset: usize, value: &JsValue) -> bool {
    if offset + kind.bytes_per_element() > buf.len() {
        return false;
    }
    match kind {
        TypedArrayKind::Int8 => {
            buf[offset] = to_int8(value) as u8;
        }
        TypedArrayKind::Uint8 => {
            buf[offset] = to_uint8(value);
        }
        TypedArrayKind::Uint8Clamped => {
            buf[offset] = to_uint8_clamped(value);
        }
        TypedArrayKind::Int16 => {
            buf[offset..offset + 2].copy_from_slice(&to_int16(value).to_ne_bytes());
        }
        TypedArrayKind::Uint16 => {
            buf[offset..offset + 2].copy_from_slice(&to_uint16(value).to_ne_bytes());
        }
        TypedArrayKind::Int32 => {
            buf[offset..offset + 4].copy_from_slice(&to_int32(value).to_ne_bytes());
        }
        TypedArrayKind::Uint32 => {
            buf[offset..offset + 4].copy_from_slice(&to_uint32(value).to_ne_bytes());
        }
        TypedArrayKind::Float32 => {
            let n = to_number(value);
            buf[offset..offset + 4].copy_from_slice(&(n as f32).to_ne_bytes());
        }
        TypedArrayKind::Float64 => {
            let n = to_number(value);
            buf[offset..offset + 8].copy_from_slice(&n.to_ne_bytes());
        }
        TypedArrayKind::BigInt64 => {
            buf[offset..offset + 8].copy_from_slice(&to_bigint64(value).to_ne_bytes());
        }
        TypedArrayKind::BigUint64 => {
            buf[offset..offset + 8].copy_from_slice(&to_biguint64(value).to_ne_bytes());
        }
        TypedArrayKind::Float16 => {
            let n = to_number(value);
            let bits = crate::interpreter::builtins::typedarray::dv_f64_to_f16_bits(n);
            buf[offset..offset + 2].copy_from_slice(&bits.to_ne_bytes());
        }
    }
    true
}

fn typed_array_get_index_shared(
    kind: TypedArrayKind,
    sab: &SharedBufferInner,
    offset: usize,
) -> JsValue {
    sab.with_read(|buf| decode_ta_element(kind, buf, offset))
}

pub(crate) fn typed_array_get_index(ta: &TypedArrayInfo, idx: usize) -> JsValue {
    if ta.is_detached.get() || is_typed_array_out_of_bounds(ta) {
        return JsValue::UNDEFINED;
    }
    let offset = ta.byte_offset + idx * ta.kind.bytes_per_element();
    let buf_borrow = ta.buffer.borrow();
    if let BufferData::Shared(sab) = &*buf_borrow {
        return typed_array_get_index_shared(ta.kind, sab, offset);
    }
    buf_borrow.with_read(|buf| decode_ta_element(ta.kind, buf, offset))
}

fn typed_array_set_index_shared(
    kind: TypedArrayKind,
    sab: &SharedBufferInner,
    offset: usize,
    value: &JsValue,
) -> bool {
    sab.with_write(|buf| encode_ta_element(kind, buf, offset, value))
}

pub(crate) fn typed_array_set_index(ta: &TypedArrayInfo, idx: usize, value: &JsValue) -> bool {
    let offset = ta.byte_offset + idx * ta.kind.bytes_per_element();
    let buf_borrow = ta.buffer.borrow();
    if let BufferData::Shared(sab) = &*buf_borrow {
        return typed_array_set_index_shared(ta.kind, sab, offset, value);
    }
    drop(buf_borrow);
    (*ta.buffer.borrow_mut()).with_write(|buf| encode_ta_element(ta.kind, buf, offset, value))
}

// Typed-array element writes coerce their value with the single canonical
// ToNumber (§7.1.4) in `helpers`; these encoders (ToInt8 … Float branches) apply
// ToInt32/ToUint32 etc. on top of it. `to_bigint64`/`to_biguint64` below still
// only read an already-`BigInt` value — a raw non-BigInt reaching them yields 0
// rather than running the throwing ToBigInt, a pre-existing gap that requires an
// error-propagating set-index signature to close and is out of scope here.
fn to_int8(v: &JsValue) -> i8 {
    number_ops::to_int32(to_number(v)) as i8
}
fn to_uint8(v: &JsValue) -> u8 {
    number_ops::to_uint32(to_number(v)) as u8
}
fn to_uint8_clamped(v: &JsValue) -> u8 {
    let n = to_number(v);
    if n.is_nan() || n <= 0.0 {
        return 0;
    }
    if n >= 255.0 {
        return 255;
    }
    let f = n.floor();
    let half = f + 0.5;
    if n < half {
        f as u8
    } else if n > half {
        (f + 1.0) as u8
    } else if (f as u64).is_multiple_of(2) {
        f as u8
    } else {
        (f + 1.0) as u8
    }
}
fn to_int16(v: &JsValue) -> i16 {
    number_ops::to_int32(to_number(v)) as i16
}
fn to_uint16(v: &JsValue) -> u16 {
    number_ops::to_uint32(to_number(v)) as u16
}
fn to_int32(v: &JsValue) -> i32 {
    number_ops::to_int32(to_number(v))
}
fn to_uint32(v: &JsValue) -> u32 {
    number_ops::to_uint32(to_number(v))
}
fn to_bigint64(v: &JsValue) -> i64 {
    v.with_bigint(|b| {
        i64::try_from(b).unwrap_or_else(|_| {
            // Truncate to 64 bits
            let bytes = b.to_signed_bytes_le();
            let mut result = [0u8; 8];
            let len = bytes.len().min(8);
            result[..len].copy_from_slice(&bytes[..len]);
            if bytes.len() < 8 && !bytes.is_empty() && (bytes[bytes.len() - 1] & 0x80) != 0 {
                for byte in result.iter_mut().skip(len) {
                    *byte = 0xFF;
                }
            }
            i64::from_le_bytes(result)
        })
    })
    .unwrap_or(0)
}
fn to_biguint64(v: &JsValue) -> u64 {
    v.with_bigint(|b| {
        u64::try_from(b).unwrap_or_else(|_| {
            let bytes = b.to_signed_bytes_le();
            let mut result = [0u8; 8];
            let len = bytes.len().min(8);
            result[..len].copy_from_slice(&bytes[..len]);
            // Sign-extend if negative (high bit of last byte is set)
            if bytes.len() < 8 && !bytes.is_empty() && (bytes[bytes.len() - 1] & 0x80) != 0 {
                for byte in result.iter_mut().skip(len) {
                    *byte = 0xFF;
                }
            }
            u64::from_le_bytes(result)
        })
    })
    .unwrap_or(0)
}

#[derive(Debug, Clone)]
pub(crate) enum PromiseState {
    Pending,
    Fulfilled(JsValue),
    Rejected(JsValue),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PromiseReactionType {
    Fulfill,
    Reject,
}

/// What a reaction job does with its handler's outcome.
#[derive(Debug, Clone)]
pub(crate) enum ReactionTarget {
    /// No result capability (Await, internal consumers).
    None,
    /// A result capability built from a non-intrinsic constructor, whose
    /// resolving functions are observable and are called as functions.
    Capability {
        promise_id: u64,
        resolve: JsValue,
        reject: JsValue,
    },
    /// The result capability of `%Promise%`: its resolving functions can never
    /// be observed, so the reaction settles the promise directly.
    Internal(u64),
    /// An `await` continuation: the reaction job resumes the suspended async
    /// function with the settled value (fulfilled) or reason (rejected)
    /// instead of calling a handler. Carries no handler function, so awaiting
    /// allocates no function objects.
    AsyncResume(u64),
    /// A continuation of the async-generator request at the front of
    /// generator `0`'s queue, resumed by the settlement of the promise it
    /// waits on. The front request is popped only after it settles, so the job
    /// recovers the request promise from there, and the queue being non-empty
    /// is also what keeps the generator alive. Carries no handler function.
    AsyncGen(u64, AsyncGenStep),
}

/// Which wait of an async-generator request an [`ReactionTarget::AsyncGen`]
/// reaction resumes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum AsyncGenStep {
    /// An `await` in the body (or a `for await` head): resume the state machine.
    Await,
    /// A `yield`'s Await of its operand: fulfilled completes the request with
    /// `{ value, done: false }`; rejected resumes the state machine with that
    /// exception thrown at the `yield`, like [`AsyncGenStep::Await`].
    Yield,
    /// AsyncGeneratorAwaitReturn: complete the request with `{ value, done: true }`,
    /// or reject it.
    AwaitReturn,
}

impl ReactionTarget {
    pub(crate) fn for_each_value(&self, mut visit: impl FnMut(&JsValue)) {
        match self {
            ReactionTarget::None => {}
            ReactionTarget::Capability {
                resolve, reject, ..
            } => {
                visit(resolve);
                visit(reject);
            }
            ReactionTarget::Internal(_)
            | ReactionTarget::AsyncResume(_)
            | ReactionTarget::AsyncGen(..) => {}
        }
    }

    pub(crate) fn promise_id(&self) -> Option<u64> {
        match self {
            ReactionTarget::None
            | ReactionTarget::AsyncResume(_)
            | ReactionTarget::AsyncGen(..) => None,
            ReactionTarget::Capability { promise_id, .. } => Some(*promise_id),
            ReactionTarget::Internal(id) => Some(*id),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PromiseReaction {
    pub handler: Option<JsValue>,
    pub target: ReactionTarget,
    pub reaction_type: PromiseReactionType,
}

/// `Internal` settles a `%Promise%`-built promise directly, without function objects.
#[derive(Debug, Clone)]
pub(crate) enum PromiseResolvers {
    Internal(u64),
    Functions { resolve: JsValue, reject: JsValue },
}

impl PromiseResolvers {
    pub(crate) fn for_each_value(&self, mut visit: impl FnMut(&JsValue)) {
        match self {
            PromiseResolvers::Internal(id) => visit(&JsValue::object(*id)),
            PromiseResolvers::Functions { resolve, reject } => {
                visit(resolve);
                visit(reject);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PromiseData {
    pub state: PromiseState,
    /// Resolver pairs created for this promise while it is pending. Native
    /// closures capture these values outside the traced Rust object graph, so
    /// the promise owns the reverse edges until settlement.
    pub resolving_functions: Vec<(JsValue, JsValue)>,
    pub fulfill_reactions: Vec<PromiseReaction>,
    pub reject_reactions: Vec<PromiseReaction>,
    pub is_handled: bool,
    /// The `[[AlreadyResolved]]` flag of the engine-owned (`Internal`)
    /// resolving functions. Exposed resolving functions keep their own flag.
    pub already_resolved: bool,
}

impl PromiseData {
    pub(crate) fn new() -> Self {
        Self {
            state: PromiseState::Pending,
            resolving_functions: Vec::new(),
            fulfill_reactions: Vec::new(),
            reject_reactions: Vec::new(),
            is_handled: false,
            already_resolved: false,
        }
    }
}

impl Default for PromiseData {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) const GC_INITIAL_THRESHOLD_BYTES: usize = 2 * 1024 * 1024;
pub(crate) const GC_NURSERY_THRESHOLD_BYTES: usize = 4 * 1024 * 1024;
pub(crate) const GC_MAJOR_MIN_THRESHOLD_BYTES: usize = 8 * 1024 * 1024;
pub(crate) const GC_GROWTH_FACTOR: usize = 2;
pub(crate) const GC_OBJECT_OVERHEAD: usize = 512;
pub(crate) const GC_PROMOTION_AGE: u8 = 2;

pub(crate) struct SetRecord {
    pub(crate) has: JsValue,
    pub(crate) keys: JsValue,
    pub(crate) size: f64,
}

#[cfg(test)]
mod shared_buffer_tests {
    use super::SharedBufferInner;

    #[test]
    fn shared_buffer_byte_views_survive_mutation_and_resize() {
        let buffer = SharedBufferInner::new((0..10).collect(), 1);
        assert_eq!(buffer.to_vec(), (0..10).collect::<Vec<_>>());

        buffer.with_write(|bytes| {
            bytes[0] = 10;
            bytes[9] = 19;
        });
        assert_eq!(buffer.to_vec(), vec![10, 1, 2, 3, 4, 5, 6, 7, 8, 19]);

        buffer.resize(13, 0xaa);
        assert_eq!(
            buffer.to_vec(),
            vec![10, 1, 2, 3, 4, 5, 6, 7, 8, 19, 0xaa, 0xaa, 0xaa]
        );

        buffer.resize(3, 0);
        assert_eq!(buffer.to_vec(), vec![10, 1, 2]);

        buffer.resize(10, 0xbb);
        assert_eq!(
            buffer.to_vec(),
            vec![10, 1, 2, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb, 0xbb]
        );
    }
}

#[cfg(test)]
mod kind_accessor_tests {
    use super::*;

    fn obj_with_kind(kind: ObjectKind) -> JsObjectData {
        let mut obj = JsObjectData::new();
        obj.kind = kind;
        obj
    }

    // --- array_elements / array_elements_mut ---

    #[test]
    fn array_elements_some_for_array_kind() {
        let obj = obj_with_kind(ObjectKind::Array(ArrayData::new(vec![JsValue::UNDEFINED])));
        assert!(obj.array_elements().is_some());
        assert_eq!(obj.array_elements().unwrap().len(), 1);
    }

    #[test]
    fn array_elements_none_for_ordinary() {
        let obj = JsObjectData::new();
        assert!(obj.array_elements().is_none());
    }

    #[test]
    fn array_elements_mut_allows_push() {
        let mut obj = obj_with_kind(ObjectKind::Array(ArrayData::default()));
        obj.array_elements_mut().unwrap().push(JsValue::TRUE);
        assert_eq!(obj.array_elements().unwrap().len(), 1);
    }

    #[test]
    fn array_named_property_order_excludes_indices_and_symbols() {
        let mut obj = obj_with_kind(ObjectKind::Array(ArrayData::default()));
        obj.insert_property(
            "length",
            PropertyDescriptor::data(JsValue::number(1024.0), true, false, false),
        );
        for index in 0..1024 {
            obj.insert_value(index.to_string(), JsValue::number(index as f64));
        }
        assert!(
            obj.array_data()
                .unwrap()
                .extra_string_property_order
                .is_none()
        );
        obj.insert_value("+1", JsValue::number(1.0));
        obj.insert_value("z", JsValue::number(2.0));
        obj.insert_value(
            JsPropertyKey::well_known_symbol("iterator"),
            JsValue::number(3.0),
        );

        let keys = obj
            .array_extra_string_property_order()
            .unwrap()
            .iter()
            .map(JsPropertyKey::to_string)
            .collect::<Vec<_>>();
        assert_eq!(keys, vec!["+1", "z"]);
        assert_eq!(obj.properties.len(), 1028);

        obj.remove_property("+1");
        obj.insert_value("+1", JsValue::number(4.0));
        let keys = obj
            .array_extra_string_property_order()
            .unwrap()
            .iter()
            .map(JsPropertyKey::to_string)
            .collect::<Vec<_>>();
        assert_eq!(keys, vec!["z", "+1"]);
    }

    #[test]
    fn ordinary_own_property_keys_orders_indices_strings_then_symbols() {
        let mut obj = JsObjectData::new();
        obj.insert_value("b", JsValue::number(1.0));
        obj.insert_value("10", JsValue::number(2.0));
        obj.insert_value(
            JsPropertyKey::well_known_symbol("iterator"),
            JsValue::number(3.0),
        );
        obj.insert_value("2", JsValue::number(4.0));
        obj.insert_value("01", JsValue::number(5.0));
        obj.insert_property(
            "hidden",
            PropertyDescriptor::data(JsValue::number(6.0), true, false, true),
        );
        let names = |keys: Vec<JsPropertyKey>| {
            keys.iter()
                .map(JsPropertyKey::to_string)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(
                obj.ordinary_own_property_keys(OwnKeyFilter::EnumerableStrings)
                    .unwrap()
            ),
            vec!["2", "10", "b", "01"]
        );
        assert_eq!(
            names(
                obj.ordinary_own_property_keys(OwnKeyFilter::AllStrings)
                    .unwrap()
            ),
            vec!["2", "10", "b", "01", "hidden"]
        );
        let all = obj
            .ordinary_own_property_keys(OwnKeyFilter::AllKeys)
            .unwrap();
        assert_eq!(all.len(), 6);
        assert!(all[5].is_symbol(), "symbols come last");
    }

    fn dense_array_with(elements: Vec<JsValue>, length: usize) -> JsObjectData {
        let mut obj = obj_with_kind(ObjectKind::Array(ArrayData::default()));
        obj.insert_property(
            "length",
            PropertyDescriptor::data(JsValue::number(length as f64), true, false, false),
        );
        obj.set_array_elements(elements);
        obj
    }

    #[test]
    fn plain_dense_array_requires_only_a_synced_writable_length() {
        let two = vec![JsValue::TRUE, JsValue::TRUE];
        let obj = dense_array_with(two.clone(), 2);
        assert!(obj.is_plain_dense_array(2));
        assert!(!obj.is_plain_dense_array(3), "length mismatch");
        assert!(
            !dense_array_with(two.clone(), 3).is_plain_dense_array(2),
            "sparse tail"
        );

        let mut extra = dense_array_with(two.clone(), 2);
        extra.insert_value("tag", JsValue::TRUE);
        assert!(!extra.is_plain_dense_array(2), "extra own property");

        let mut indexed = dense_array_with(two.clone(), 2);
        indexed.insert_value("0", JsValue::TRUE);
        assert!(
            !indexed.is_plain_dense_array(2),
            "descriptor-backed element"
        );

        let mut frozen_length = dense_array_with(two, 2);
        frozen_length
            .properties
            .get_mut("length")
            .unwrap()
            .set_writable(Some(false));
        assert!(!frozen_length.is_plain_dense_array(2), "read-only length");

        assert!(!JsObjectData::new().is_plain_dense_array(0), "not an Array");
    }

    #[test]
    fn dense_array_set_plan_distinguishes_overwrite_create_and_generic() {
        use DenseArraySet::{Create, Overwrite};
        let v = JsValue::number(1.0);
        let obj = dense_array_with(vec![JsValue::TRUE, JsValue::UNDEFINED], 2);
        assert_eq!(obj.dense_array_set_plan(0, &v), Some(Overwrite));
        assert_eq!(obj.dense_array_set_plan(1, &v), Some(Create), "hole");
        assert_eq!(obj.dense_array_set_plan(2, &v), Some(Create), "append");
        assert_eq!(obj.dense_array_set_plan(3, &v), None, "past the next index");
        assert_eq!(
            obj.dense_array_set_plan(0, &JsValue::UNDEFINED),
            None,
            "undefined needs a descriptor"
        );
        assert_eq!(obj.dense_array_set_plan(0xFFFF_FFFF, &v), None);

        let mut named = dense_array_with(vec![JsValue::TRUE], 1);
        named.insert_value("tag", JsValue::TRUE);
        assert_eq!(named.dense_array_set_plan(0, &v), Some(Overwrite));
        named.insert_value("0x", JsValue::TRUE);
        assert_eq!(named.dense_array_set_plan(0, &v), None, "index-like key");

        let mut sealed = dense_array_with(vec![JsValue::TRUE, JsValue::UNDEFINED], 2);
        sealed.extensible = false;
        assert_eq!(sealed.dense_array_set_plan(0, &v), Some(Overwrite));
        assert_eq!(
            sealed.dense_array_set_plan(1, &v),
            None,
            "hole, not extensible"
        );
        assert_eq!(
            sealed.dense_array_set_plan(2, &v),
            None,
            "append, not extensible"
        );

        let mut readonly = dense_array_with(vec![JsValue::TRUE, JsValue::UNDEFINED], 2);
        readonly
            .properties
            .get_mut("length")
            .unwrap()
            .set_writable(Some(false));
        assert_eq!(readonly.dense_array_set_plan(1, &v), Some(Create));
        // The plan is advisory: the append itself re-checks the read-only length.
        assert!(!readonly.apply_dense_array_set(2, &v, Create));

        let short_length = dense_array_with(vec![JsValue::TRUE, JsValue::UNDEFINED], 1);
        assert_eq!(
            short_length.dense_array_set_plan(1, &v),
            None,
            "hole at or past length"
        );

        assert_eq!(JsObjectData::new().dense_array_set_plan(0, &v), None);
    }

    #[test]
    fn apply_dense_array_set_keeps_length_and_holes_consistent() {
        use DenseArraySet::{Create, Overwrite};
        let mut obj = dense_array_with(vec![JsValue::TRUE, JsValue::UNDEFINED], 4);
        assert!(obj.apply_dense_array_set(0, &JsValue::number(5.0), Overwrite));
        assert!(obj.apply_dense_array_set(1, &JsValue::number(6.0), Create));
        assert!(obj.apply_dense_array_set(2, &JsValue::number(7.0), Create));
        let elements = obj.array_elements().unwrap();
        assert_eq!(elements.len(), 3);
        assert_eq!(elements[0].as_number(), Some(5.0));
        assert_eq!(elements[1].as_number(), Some(6.0));
        assert_eq!(elements[2].as_number(), Some(7.0));
        assert!(
            obj.has_writable_array_length(4),
            "writes below length leave it alone"
        );
        assert!(obj.apply_dense_array_set(3, &JsValue::number(8.0), Create));
        assert!(obj.is_plain_dense_array(4));
        assert!(obj.apply_dense_array_set(4, &JsValue::number(9.0), Create));
        assert!(obj.is_plain_dense_array(5), "append past length grows it");
    }

    #[test]
    fn dense_element_append_extends_only_at_the_next_index() {
        let mut obj = dense_array_with(vec![JsValue::TRUE], 1);
        assert!(!obj.try_append_dense_array_element(2, &JsValue::TRUE));
        assert!(!obj.try_append_dense_array_element(1, &JsValue::UNDEFINED));
        assert!(obj.try_append_dense_array_element(1, &JsValue::number(7.0)));
        assert_eq!(obj.array_elements().unwrap().len(), 2);
        assert_eq!(
            obj.properties
                .get("length")
                .and_then(|d| d.value())
                .and_then(JsValue::as_number),
            Some(2.0)
        );
        obj.extensible = false;
        assert!(!obj.try_append_dense_array_element(2, &JsValue::TRUE));
        obj.extensible = true;
        obj.properties
            .get_mut("length")
            .unwrap()
            .set_writable(Some(false));
        assert!(!obj.try_append_dense_array_element(2, &JsValue::TRUE));
    }

    #[test]
    fn ordinary_own_property_keys_declines_exotics() {
        let mut wrapper = JsObjectData::new();
        wrapper.class_name = "String";
        wrapper.primitive_value = Some(JsValue::string(JsString::from_str("ab")));
        assert!(
            wrapper
                .ordinary_own_property_keys(OwnKeyFilter::AllKeys)
                .is_none()
        );
        assert!(wrapper.has_enumerable_string_key_hint().is_none());
    }

    #[test]
    fn ordinary_own_property_keys_merges_dense_elements_with_named_keys() {
        let mut obj = obj_with_kind(ObjectKind::Array(ArrayData::default()));
        obj.insert_property(
            "length",
            PropertyDescriptor::data(JsValue::number(3.0), true, false, false),
        );
        obj.set_array_elements(vec![JsValue::TRUE, JsValue::TRUE, JsValue::TRUE]);
        obj.insert_value("tag", JsValue::TRUE);
        obj.insert_property(
            "1",
            PropertyDescriptor::data(JsValue::TRUE, true, false, true),
        );
        let keys = obj
            .ordinary_own_property_keys(OwnKeyFilter::EnumerableStrings)
            .unwrap()
            .iter()
            .map(JsPropertyKey::to_string)
            .collect::<Vec<_>>();
        assert_eq!(keys, vec!["0", "2", "tag"]);
        let keys = obj
            .ordinary_own_property_keys(OwnKeyFilter::AllStrings)
            .unwrap()
            .iter()
            .map(JsPropertyKey::to_string)
            .collect::<Vec<_>>();
        assert_eq!(keys, vec!["0", "1", "2", "length", "tag"]);
    }

    // --- set_data / set_data_mut ---

    #[test]
    fn set_data_some_for_set_kind() {
        let mut entries = KeyedEntries::default();
        entries.upsert(JsValue::number(1.0), |_, _| {});
        let obj = obj_with_kind(ObjectKind::Set(Box::new(entries)));
        assert!(obj.set_data().is_some());
        assert_eq!(obj.set_data().unwrap().live_len(), 1);
    }

    #[test]
    fn set_data_none_for_non_set_kind() {
        let obj = obj_with_kind(ObjectKind::Array(ArrayData::default()));
        assert!(obj.set_data().is_none());
    }

    #[test]
    fn set_data_mut_allows_push() {
        let mut obj = obj_with_kind(ObjectKind::Set(Box::default()));
        obj.set_data_mut()
            .unwrap()
            .upsert(JsValue::number(1.0), |_, _| {});
        assert_eq!(obj.set_data().unwrap().live_len(), 1);
    }

    // --- map_data / map_data_mut ---

    #[test]
    fn map_data_some_for_map_kind() {
        let mut entries = KeyedEntries::default();
        entries.upsert((JsValue::number(1.0), JsValue::FALSE), |_, _| {});
        let obj = obj_with_kind(ObjectKind::Map(Box::new(entries)));
        assert!(obj.map_data().is_some());
        assert_eq!(obj.map_data().unwrap().live_len(), 1);
    }

    #[test]
    fn map_data_none_for_non_map_kind() {
        let obj = JsObjectData::new();
        assert!(obj.map_data().is_none());
    }

    #[test]
    fn map_data_mut_allows_push() {
        let mut obj = obj_with_kind(ObjectKind::Map(Box::default()));
        obj.map_data_mut()
            .unwrap()
            .upsert((JsValue::number(0.0), JsValue::UNDEFINED), |_, _| {});
        assert_eq!(obj.map_data().unwrap().live_len(), 1);
    }

    // --- bound ---

    #[test]
    fn bound_some_for_bound_function_kind() {
        let data = BoundFunctionData {
            target: JsValue::UNDEFINED,
            this: JsValue::UNDEFINED,
            args: vec![],
        };
        let obj = obj_with_kind(ObjectKind::BoundFunction(Box::new(data)));
        assert!(obj.bound().is_some());
    }

    #[test]
    fn bound_none_for_ordinary() {
        let obj = JsObjectData::new();
        assert!(obj.bound().is_none());
    }

    // --- regexp ---

    #[test]
    fn regexp_some_for_regexp_kind() {
        let data = RegExpData::new(JsString::from_str("foo"), JsString::from_str(""));
        let obj = obj_with_kind(ObjectKind::RegExp(data));
        assert!(obj.regexp().is_some());
    }

    #[test]
    fn regexp_none_for_non_regexp_kind() {
        let obj = JsObjectData::new();
        assert!(obj.regexp().is_none());
    }

    // --- Mutual exclusion: accessor returns None when a *different* macro-generated kind is set ---

    #[test]
    fn set_data_none_when_map_kind_is_set() {
        let obj = obj_with_kind(ObjectKind::Map(Box::default()));
        assert!(obj.set_data().is_none());
        assert!(obj.map_data().is_some());
    }

    #[test]
    fn array_elements_none_when_set_kind_is_set() {
        let obj = obj_with_kind(ObjectKind::Set(Box::default()));
        assert!(obj.array_elements().is_none());
        assert!(obj.set_data().is_some());
    }

    // --- shadow_realm_id (Copy return — hand-written, not macro) ---

    #[test]
    fn shadow_realm_id_some_for_shadow_realm_kind() {
        let obj = obj_with_kind(ObjectKind::ShadowRealm(42));
        assert_eq!(obj.shadow_realm_id(), Some(42));
    }

    #[test]
    fn shadow_realm_id_none_for_ordinary() {
        let obj = JsObjectData::new();
        assert!(obj.shadow_realm_id().is_none());
    }
}

#[cfg(test)]
mod property_bag_tests {
    use super::*;

    fn keys(obj: &JsObjectData) -> Vec<String> {
        obj.properties.keys().map(|k| k.to_string()).collect()
    }

    #[test]
    fn remove_property_clears_the_map_entry() {
        let mut obj = JsObjectData::new();
        obj.insert_property("a", PropertyDescriptor::data_default(JsValue::number(1.0)));
        obj.insert_property("b", PropertyDescriptor::data_default(JsValue::number(2.0)));

        let removed = obj.remove_property("a");

        assert!(removed.is_some(), "returns the removed descriptor");
        assert!(!obj.properties.contains_key("a"), "gone from property map");
        assert_eq!(keys(&obj), vec!["b"], "gone from the key order too");
    }

    #[test]
    fn remove_property_preserves_order_of_survivors() {
        let mut obj = JsObjectData::new();
        for k in ["a", "b", "c", "d"] {
            obj.insert_property(k, PropertyDescriptor::data_default(JsValue::UNDEFINED));
        }

        obj.remove_property("b");

        // Deleting a middle key must not disturb the relative order of the rest.
        assert_eq!(keys(&obj), vec!["a", "c", "d"]);
        assert!(!obj.properties.contains_key("b"));
    }

    #[test]
    fn remove_property_returns_the_stored_descriptor() {
        let mut obj = JsObjectData::new();
        obj.insert_property(
            "x",
            PropertyDescriptor::data(JsValue::number(42.0), false, true, false),
        );

        let removed = obj.remove_property("x").expect("descriptor returned");

        assert_eq!(
            removed.value.and_then(|value| value.as_number()),
            Some(42.0)
        );
        assert_eq!(removed.writable, Some(false));
        assert_eq!(removed.enumerable, Some(true));
        assert_eq!(removed.configurable, Some(false));
    }

    #[test]
    fn remove_absent_key_is_a_noop() {
        let mut obj = JsObjectData::new();
        obj.insert_property("a", PropertyDescriptor::data_default(JsValue::UNDEFINED));

        assert!(obj.remove_property("missing").is_none());
        assert_eq!(keys(&obj), vec!["a"], "order untouched");
        assert!(obj.properties.contains_key("a"), "map untouched");
    }

    #[test]
    fn second_remove_returns_none() {
        let mut obj = JsObjectData::new();
        obj.insert_property("a", PropertyDescriptor::data_default(JsValue::UNDEFINED));

        assert!(obj.remove_property("a").is_some());
        assert!(obj.remove_property("a").is_none());
        assert!(keys(&obj).is_empty());
    }
}

#[cfg(test)]
mod string_exotic_index_tests {
    use super::string_exotic_index;

    // §10.4.3 StringGetOwnProperty own-index predicate. Expected values are the
    // ECMAScript truth table (CanonicalNumericIndexString + IsIntegralNumber +
    // -0 + range), cross-checked against node.
    #[test]
    fn canonical_in_range_indices_resolve() {
        assert_eq!(string_exotic_index("0", 3), Some(0));
        assert_eq!(string_exotic_index("1", 3), Some(1));
        assert_eq!(string_exotic_index("2", 3), Some(2));
    }

    #[test]
    fn out_of_range_is_none() {
        assert_eq!(string_exotic_index("3", 3), None);
        assert_eq!(string_exotic_index("0", 0), None);
        assert_eq!(string_exotic_index("9999999999", 3), None);
    }

    #[test]
    fn non_canonical_spellings_are_none() {
        // Round-trippable ToString(ToNumber(key)) != key -> not a canonical index.
        assert_eq!(string_exotic_index("01", 3), None);
        assert_eq!(string_exotic_index("00", 3), None);
        assert_eq!(string_exotic_index("+1", 3), None);
        assert_eq!(string_exotic_index("1.0", 3), None);
        assert_eq!(string_exotic_index(" 1", 3), None);
        assert_eq!(string_exotic_index("1 ", 3), None);
        assert_eq!(string_exotic_index("1e0", 3), None);
        assert_eq!(string_exotic_index("0x1", 3), None);
        assert_eq!(string_exotic_index("", 3), None);
    }

    #[test]
    fn non_integral_canonical_numbers_are_none() {
        assert_eq!(string_exotic_index("1.5", 3), None);
        assert_eq!(string_exotic_index("0.5", 3), None);
    }

    #[test]
    fn negative_and_negative_zero_are_none() {
        assert_eq!(string_exotic_index("-1", 3), None);
        // CanonicalNumericIndexString("-0") is -0f; StringGetOwnProperty rejects it.
        assert_eq!(string_exotic_index("-0", 3), None);
    }

    #[test]
    fn non_finite_canonical_words_are_none() {
        // "Infinity"/"NaN" round-trip through ToString but are not integral indices.
        assert_eq!(string_exotic_index("Infinity", 3), None);
        assert_eq!(string_exotic_index("-Infinity", 3), None);
        assert_eq!(string_exotic_index("NaN", 3), None);
    }

    #[test]
    fn length_word_is_not_an_index() {
        assert_eq!(string_exotic_index("length", 3), None);
    }
}

#[cfg(test)]
mod typed_array_codec_tests {
    //! Behavioural tests for the shared element codec used by every typed-array
    //! `[[Get]]`/`[[Set]]`. Expected values are drawn from independent sources of
    //! truth: two's-complement / IEEE-754 byte layouts, the spec's ToInt/ToUint
    //! wrapping rules (§7.1.x), and known half-float bit patterns — never
    //! recomputed the way the codec computes them.
    use super::{TypedArrayKind, decode_ta_element, encode_ta_element};
    use crate::types::{JsBigInt, JsValue};

    fn num(v: &JsValue) -> f64 {
        v.as_number().expect("expected a number JsValue")
    }

    // --- decode: integers ---

    #[test]
    fn decode_int8_reads_twos_complement() {
        // 0xFF as a signed byte is -1.
        assert_eq!(
            num(&decode_ta_element(TypedArrayKind::Int8, &[0xFF], 0)),
            -1.0
        );
        assert_eq!(
            num(&decode_ta_element(TypedArrayKind::Int8, &[0x80], 0)),
            -128.0
        );
        assert_eq!(
            num(&decode_ta_element(TypedArrayKind::Int8, &[0x7F], 0)),
            127.0
        );
    }

    #[test]
    fn decode_uint8_variants_read_raw_byte() {
        assert_eq!(
            num(&decode_ta_element(TypedArrayKind::Uint8, &[0xFF], 0)),
            255.0
        );
        assert_eq!(
            num(&decode_ta_element(TypedArrayKind::Uint8Clamped, &[0xFF], 0)),
            255.0
        );
    }

    #[test]
    fn decode_multibyte_ints_round_trip_native_encoding() {
        // Build inputs with std's encoder (independent of our from_ne_bytes decode).
        assert_eq!(
            num(&decode_ta_element(
                TypedArrayKind::Int16,
                &(-2i16).to_ne_bytes(),
                0
            )),
            -2.0
        );
        assert_eq!(
            num(&decode_ta_element(
                TypedArrayKind::Uint16,
                &(0xBEEFu16).to_ne_bytes(),
                0
            )),
            48879.0
        );
        assert_eq!(
            num(&decode_ta_element(
                TypedArrayKind::Int32,
                &(-2i32).to_ne_bytes(),
                0
            )),
            -2.0
        );
        assert_eq!(
            num(&decode_ta_element(
                TypedArrayKind::Uint32,
                &(0xDEADBEEFu32).to_ne_bytes(),
                0
            )),
            3_735_928_559.0
        );
    }

    #[test]
    fn decode_reads_at_nonzero_offset() {
        let mut buf = vec![0u8; 8];
        buf[4..8].copy_from_slice(&(0x0102_0304u32).to_ne_bytes());
        assert_eq!(
            num(&decode_ta_element(TypedArrayKind::Uint32, &buf, 4)),
            16_909_060.0
        );
    }

    // --- decode: floats ---

    #[test]
    fn decode_float32_and_float64_round_trip() {
        assert_eq!(
            num(&decode_ta_element(
                TypedArrayKind::Float32,
                &1.5f32.to_ne_bytes(),
                0
            )),
            1.5
        );
        assert_eq!(
            num(&decode_ta_element(
                TypedArrayKind::Float64,
                &6.5f64.to_ne_bytes(),
                0
            )),
            6.5
        );
    }

    #[test]
    fn decode_float16_known_bit_patterns() {
        // IEEE-754 half: 0x3C00 == 1.0, 0xC000 == -2.0.
        assert_eq!(
            num(&decode_ta_element(
                TypedArrayKind::Float16,
                &0x3C00u16.to_ne_bytes(),
                0
            )),
            1.0
        );
        assert_eq!(
            num(&decode_ta_element(
                TypedArrayKind::Float16,
                &0xC000u16.to_ne_bytes(),
                0
            )),
            -2.0
        );
    }

    // --- decode: bigint ---

    #[test]
    fn decode_bigint64_sign_extends() {
        let v = decode_ta_element(TypedArrayKind::BigInt64, &(-5i64).to_ne_bytes(), 0);
        let got = v.with_bigint(|b| b.clone()).expect("expected a bigint");
        assert_eq!(got, num_bigint::BigInt::from(-5i64));
    }

    #[test]
    fn decode_biguint64_is_unsigned() {
        let v = decode_ta_element(TypedArrayKind::BigUint64, &u64::MAX.to_ne_bytes(), 0);
        let got = v.with_bigint(|b| b.clone()).expect("expected a bigint");
        assert_eq!(got, num_bigint::BigInt::from(u64::MAX));
    }

    // --- decode: bounds ---

    #[test]
    fn decode_past_end_is_undefined() {
        // Int32 needs 4 bytes; a 3-byte buffer cannot supply one at offset 0.
        assert!(decode_ta_element(TypedArrayKind::Int32, &[0, 0, 0], 0).is_undefined());
        // Exactly-fits succeeds; one past the last full element does not.
        let buf = [0u8; 4];
        assert!(!decode_ta_element(TypedArrayKind::Int16, &buf, 2).is_undefined());
        assert!(decode_ta_element(TypedArrayKind::Int16, &buf, 3).is_undefined());
    }

    // --- encode: integer coercion (spec ToInt/ToUint wrapping) ---

    #[test]
    fn encode_int8_wraps_modulo_256() {
        // ToInt8(300) = 300 - 256 = 44.
        let mut buf = [0u8; 1];
        assert!(encode_ta_element(
            TypedArrayKind::Int8,
            &mut buf,
            0,
            &JsValue::number(300.0)
        ));
        assert_eq!(buf[0] as i8, 44);
        // ToInt8(-1) = -1 == 0xFF.
        assert!(encode_ta_element(
            TypedArrayKind::Int8,
            &mut buf,
            0,
            &JsValue::number(-1.0)
        ));
        assert_eq!(buf[0], 0xFF);
    }

    #[test]
    fn encode_uint8_clamped_rounds_half_to_even() {
        let mut buf = [0u8; 1];
        // 2.5 -> 2 (round half to even), 3.5 -> 4, 256 -> 255 (clamp), -1 -> 0.
        for (input, expected) in [(2.5, 2u8), (3.5, 4), (256.0, 255), (-1.0, 0)] {
            assert!(encode_ta_element(
                TypedArrayKind::Uint8Clamped,
                &mut buf,
                0,
                &JsValue::number(input)
            ));
            assert_eq!(buf[0], expected, "clamp({input})");
        }
    }

    #[test]
    fn encode_int32_matches_native_encoding() {
        let mut buf = [0u8; 4];
        assert!(encode_ta_element(
            TypedArrayKind::Int32,
            &mut buf,
            0,
            &JsValue::number(-2.0)
        ));
        assert_eq!(buf, (-2i32).to_ne_bytes());
    }

    // --- encode: floats ---

    #[test]
    fn encode_float64_matches_native_encoding() {
        let mut buf = [0u8; 8];
        assert!(encode_ta_element(
            TypedArrayKind::Float64,
            &mut buf,
            0,
            &JsValue::number(6.5)
        ));
        assert_eq!(buf, 6.5f64.to_ne_bytes());
    }

    #[test]
    fn encode_float32_narrows_then_encodes() {
        let mut buf = [0u8; 4];
        assert!(encode_ta_element(
            TypedArrayKind::Float32,
            &mut buf,
            0,
            &JsValue::number(1.5)
        ));
        assert_eq!(buf, 1.5f32.to_ne_bytes());
    }

    // --- encode: bigint ---

    #[test]
    fn encode_bigint64_matches_native_encoding() {
        let mut buf = [0u8; 8];
        let value = JsValue::bigint(JsBigInt::new(num_bigint::BigInt::from(-5i64)));
        assert!(encode_ta_element(
            TypedArrayKind::BigInt64,
            &mut buf,
            0,
            &value
        ));
        assert_eq!(buf, (-5i64).to_ne_bytes());
    }

    // --- encode: bounds ---

    #[test]
    fn encode_past_end_returns_false_and_writes_nothing() {
        let mut buf = [0u8; 3];
        assert!(!encode_ta_element(
            TypedArrayKind::Int32,
            &mut buf,
            0,
            &JsValue::number(1.0)
        ));
        assert_eq!(
            buf, [0u8; 3],
            "buffer must be untouched on out-of-bounds write"
        );
    }

    // --- round-trip: encode then decode is identity ---

    #[test]
    fn encode_decode_round_trips_per_kind() {
        let cases: &[(TypedArrayKind, f64)] = &[
            (TypedArrayKind::Int8, -3.0),
            (TypedArrayKind::Uint8, 200.0),
            (TypedArrayKind::Int16, -1234.0),
            (TypedArrayKind::Uint16, 40000.0),
            (TypedArrayKind::Int32, -70000.0),
            (TypedArrayKind::Uint32, 3_000_000_000.0),
            (TypedArrayKind::Float32, 1.5),
            (TypedArrayKind::Float64, -9.5),
        ];
        for &(kind, value) in cases {
            let mut buf = vec![0u8; kind.bytes_per_element()];
            assert!(
                encode_ta_element(kind, &mut buf, 0, &JsValue::number(value)),
                "encode {}",
                kind.name()
            );
            assert_eq!(
                num(&decode_ta_element(kind, &buf, 0)),
                value,
                "round-trip {}",
                kind.name()
            );
        }
    }
}

#[cfg(test)]
mod canonical_numeric_index_tests {
    use super::canonical_numeric_index_string;

    #[test]
    fn plain_digit_strings_are_their_own_canonical_form() {
        for (key, expected) in [
            ("0", 0.0),
            ("7", 7.0),
            ("10", 10.0),
            ("4294967295", 4294967295.0),
            ("999999999999999", 999_999_999_999_999.0),
        ] {
            assert_eq!(canonical_numeric_index_string(key), Some(expected), "{key}");
        }
    }

    #[test]
    fn non_canonical_spellings_stay_rejected() {
        for key in [
            "",
            "00",
            "01",
            "+1",
            "1.0",
            "1e3",
            "0x10",
            " 1",
            "1 ",
            "1_0",
            "abc",
            "Infinity1",
        ] {
            assert_eq!(canonical_numeric_index_string(key), None, "{key:?}");
        }
    }

    #[test]
    fn sixteen_digit_strings_take_the_general_path() {
        assert_eq!(
            canonical_numeric_index_string("1000000000000000"),
            Some(1e15)
        );
        assert_eq!(
            canonical_numeric_index_string("9007199254740993"),
            None,
            "not the shortest round-trip spelling of its double"
        );
        assert_eq!(
            canonical_numeric_index_string("-0").map(f64::is_sign_negative),
            Some(true)
        );
        assert_eq!(canonical_numeric_index_string("-5"), Some(-5.0));
        assert_eq!(canonical_numeric_index_string("1.5"), Some(1.5));
        assert_eq!(
            canonical_numeric_index_string("NaN").map(f64::is_nan),
            Some(true)
        );
    }
}

#[cfg(test)]
mod parameter_map_tests {
    use super::*;

    fn map_over(names: &[&str], last: &[bool], argc: usize) -> (EnvRef, Option<ParameterMap>) {
        let env = Environment::new(None);
        for (i, name) in names.iter().enumerate() {
            env.borrow_mut().declare(name, BindingKind::Var);
            env.borrow_mut()
                .initialize_binding(name, JsValue::number(i as f64));
        }
        let atoms: Rc<[Atom]> = names.iter().map(|n| Atom::intern(n)).collect();
        (env.clone(), ParameterMap::new(env, atoms, last, argc))
    }

    #[test]
    fn nothing_mapped_yields_no_map() {
        assert!(map_over(&["a"], &[true], 0).1.is_none());
        assert!(map_over(&["a", "a"], &[false, false], 2).1.is_none());
        assert!(map_over(&[], &[], 3).1.is_none());
    }

    #[test]
    fn only_canonical_indices_below_the_argument_count_are_mapped() {
        let (_, map) = map_over(&["a", "b", "c"], &[true, true, true], 2);
        let map = map.expect("mapped");
        assert!(map.contains_key("0") && map.contains_key("1"));
        for key in ["2", "00", "01", "+1", "1.0", " 1", "", "a", "4294967296"] {
            assert!(!map.contains_key(key), "{key:?}");
        }
    }

    #[test]
    fn duplicate_names_map_only_their_last_occurrence() {
        let (env, map) = map_over(&["a", "b", "a"], &[false, true, true], 3);
        let map = map.expect("mapped");
        assert!(!map.contains_key("0"));
        assert!(map.contains_key("1") && map.contains_key("2"));
        env.borrow_mut()
            .bindings
            .get_mut("a")
            .expect("binding")
            .value = JsValue::number(9.0);
        assert_eq!(map.value("2").and_then(|v| v.as_number()), Some(9.0));
        assert!(map.value("0").is_none());
    }

    #[test]
    fn reads_and_writes_go_through_the_environment() {
        let (env, map) = map_over(&["a", "b"], &[true, true], 2);
        let mut map = map.expect("mapped");
        assert_eq!(map.value("1").and_then(|v| v.as_number()), Some(1.0));
        map.set_value("1", &JsValue::number(5.0));
        assert_eq!(
            env.borrow().bindings.get("b").map(|b| b.value.as_number()),
            Some(Some(5.0))
        );
        map.set_value("7", &JsValue::number(1.0));
        map.remove("1");
        assert!(!map.contains_key("1") && map.contains_key("0"));
        map.set_value("1", &JsValue::number(6.0));
        assert_eq!(
            env.borrow().bindings.get("b").map(|b| b.value.as_number()),
            Some(Some(5.0)),
            "an unmapped index no longer writes through"
        );
        assert!(map.value("1").is_none());
    }
}
