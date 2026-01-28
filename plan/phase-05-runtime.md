# Phase 5: Runtime Core

**Spec Reference:** §9 (Environment Records), §10 (Execution Contexts), §6.1.7 (Object Internals), §7.3 (Object Operations), §8 (Syntax-Directed Operations)

## Goal
Implement the runtime infrastructure: environments, execution contexts, ordinary/exotic objects, and all abstract operations needed to begin evaluating code.

## Tasks

### 5.1 Environment Records (§9.1)
- [x] Declarative Environment Record
  - [x] HasBinding, CreateMutableBinding, CreateImmutableBinding
  - [x] InitializeBinding, SetMutableBinding, GetBindingValue
  - [x] DeleteBinding, HasThisBinding, HasSuperBinding, WithBaseObject
- [x] Object Environment Record
  - [x] All concrete methods
  - [x] `withEnvironment` flag
- [x] Function Environment Record
  - [x] `[[ThisValue]]`, `[[ThisBindingStatus]]`
  - [x] BindThisValue, GetThisBinding, GetSuperBase
- [x] Global Environment Record
  - [x] HasBinding, CreateMutableBinding, CreateImmutableBinding
  - [x] InitializeBinding, SetMutableBinding, GetBindingValue
  - [x] DeleteBinding, HasThisBinding, GetThisBinding
  - [x] HasVarDeclaration, HasLexicalDeclaration, HasRestrictedGlobalProperty
  - [x] CanDeclareGlobalVar, CanDeclareGlobalFunction
  - [x] CreateGlobalVarBinding, CreateGlobalFunctionBinding
- [ ] Module Environment Record
  - [ ] CreateImportBinding
  - [ ] GetBindingValue (indirection for imports)
  - [ ] GetThisBinding (returns undefined)

### 5.2 Environment Record Operations (§9.1.2)
- [x] GetIdentifierReference
- [x] NewDeclarativeEnvironment
- [x] NewObjectEnvironment
- [x] NewFunctionEnvironment
- [x] NewGlobalEnvironment
- [ ] NewModuleEnvironment

### 5.3 PrivateEnvironment Records (§9.2)
- [ ] PrivateEnvironment Record
- [ ] NewPrivateEnvironment
- [ ] ResolvePrivateIdentifier

### 5.4 Realms (§9.3)
- [x] Realm Record
- [x] CreateRealm
- [x] CreateIntrinsics
- [x] SetRealmGlobalObject
- [x] SetDefaultGlobalBindings

### 5.5 Execution Contexts (§9.4)
- [x] Execution context stack
- [x] Running execution context
- [x] LexicalEnvironment / VariableEnvironment / PrivateEnvironment
- [x] Function execution context
- [x] Script/Module execution context
- [ ] GetActiveScriptOrModule
- [x] ResolveBinding
- [x] GetThisEnvironment
- [x] ResolveThisBinding
- [x] GetNewTarget
- [x] GetGlobalObject

### 5.6 Jobs & Job Queues (§9.5)
- [ ] Job queue infrastructure (for Promises, etc.)
- [ ] HostEnqueuePromiseJob (placeholder)
- [ ] HostEnqueueGenericJob

### 5.7 Agents (§9.7)
- [ ] Agent Record (single-agent initially, multi-agent for SharedArrayBuffer later)
- [ ] AgentSignifier
- [ ] IsCompatiblePropertyDescriptor
- [ ] ValidateAndApplyPropertyDescriptor

### 5.8 Ordinary Object Internal Methods (§10.1)
- [x] `[[GetPrototypeOf]]` / OrdinaryGetPrototypeOf
- [x] `[[SetPrototypeOf]]` / OrdinarySetPrototypeOf
- [x] `[[IsExtensible]]` / OrdinaryIsExtensible
- [x] `[[PreventExtensions]]` / OrdinaryPreventExtensions
- [x] `[[GetOwnProperty]]` / OrdinaryGetOwnProperty
- [x] `[[DefineOwnProperty]]` / OrdinaryDefineOwnProperty / ValidateAndApplyPropertyDescriptor
- [x] `[[HasProperty]]` / OrdinaryHasProperty
- [x] `[[Get]]` / OrdinaryGet
- [x] `[[Set]]` / OrdinarySet / OrdinarySetWithOwnDescriptor
- [x] `[[Delete]]` / OrdinaryDelete
- [x] `[[OwnPropertyKeys]]` / OrdinaryOwnPropertyKeys
- [x] OrdinaryObjectCreate
- [x] OrdinaryCreateFromConstructor
- [x] GetPrototypeFromConstructor

### 5.9 ECMAScript Function Objects (§10.2)
- [x] Function internal slots (`[[Environment]]`, `[[FormalParameters]]`, `[[ECMAScriptCode]]`, `[[ConstructorKind]]`, `[[Realm]]`, `[[ScriptOrModule]]`, `[[ThisMode]]`, `[[Strict]]`, `[[HomeObject]]`, `[[SourceText]]`, `[[Fields]]`, `[[PrivateMethods]]`, `[[ClassFieldInitializerName]]`, `[[IsClassConstructor]]`)
- [x] `[[Call]]` internal method
- [x] `[[Construct]]` internal method
- [x] OrdinaryFunctionCreate
- [x] MakeConstructor
- [x] MakeClassConstructor
- [x] MakeMethod
- [x] SetFunctionName
- [x] SetFunctionLength
- [x] FunctionDeclarationInstantiation

### 5.10 Built-in Function Objects (§10.3)
- [x] Built-in function as abstract closure
- [x] CreateBuiltinFunction
- [x] Built-in `[[Call]]` behavior

### 5.11 Exotic Objects (§10.4)
- [x] Bound Function Exotic Objects (§10.4.1)
  - [x] `[[Call]]`, `[[Construct]]`
  - [x] BoundFunctionCreate
- [x] Array Exotic Objects (§10.4.2)
  - [x] `[[DefineOwnProperty]]` with array length semantics
  - [x] ArrayCreate, ArraySpeciesCreate
- [x] String Exotic Objects (§10.4.3)
  - [x] `[[GetOwnProperty]]`, `[[DefineOwnProperty]]`, `[[OwnPropertyKeys]]`
  - [x] StringCreate, StringGetOwnProperty
- [ ] Arguments Exotic Objects (§10.4.4) — **BLOCKER: ~140 tests**
  - [ ] Mapped arguments
  - [ ] Unmapped arguments
  - [ ] CreateMappedArgumentsObject, CreateUnmappedArgumentsObject
- [ ] Integer-Indexed Exotic Objects (§10.4.5)
  - [ ] For TypedArrays
- [ ] Module Namespace Exotic Objects (§10.4.6)
- [ ] Immutable Prototype Exotic Objects (§10.4.7)
  - [ ] `%Object.prototype%` and `%JSON%` use this

### 5.12 Syntax-Directed Operations (§8)
- [ ] Runtime Semantics: Evaluation (dispatch per AST node type)
- [ ] Runtime Semantics: NamedEvaluation
- [ ] Runtime Semantics: PropertyDefinitionEvaluation
- [ ] Runtime Semantics: ArgumentListEvaluation
- [ ] Runtime Semantics: IteratorBindingInitialization
- [ ] Runtime Semantics: KeyedBindingInitialization
- [ ] Runtime Semantics: BindingInitialization
- [ ] Static Semantics: BoundNames
- [ ] Static Semantics: DeclarationPart
- [ ] Static Semantics: IsConstantDeclaration
- [ ] Static Semantics: LexicallyDeclaredNames
- [ ] Static Semantics: LexicallyScopedDeclarations
- [ ] Static Semantics: VarDeclaredNames
- [ ] Static Semantics: VarScopedDeclarations
- [ ] Static Semantics: TopLevelLexicallyDeclaredNames
- [ ] Static Semantics: TopLevelLexicallyScopedDeclarations
- [ ] Static Semantics: TopLevelVarDeclaredNames
- [ ] Static Semantics: TopLevelVarScopedDeclarations
- [ ] Static Semantics: ContainsDuplicateLabels
- [ ] Static Semantics: ContainsUndefinedBreakTarget
- [ ] Static Semantics: ContainsUndefinedContinueTarget
- [ ] Static Semantics: ContainsArguments
- [ ] Static Semantics: AssignmentTargetType
- [ ] Static Semantics: PropName
- [ ] Static Semantics: ContainsExpression

### 5.13 Iteration (§7.4)
- [ ] Iterator Records
- [ ] GetIteratorFromMethod
- [ ] GetIterator (sync and async)
- [ ] IteratorNext / IteratorStep / IteratorStepValue
- [ ] IteratorClose
- [ ] IfAbruptCloseIterator
- [ ] AsyncIteratorClose
- [ ] CreateIteratorResultObject
- [ ] CreateListIteratorRecord
- [ ] IteratorToList
- [ ] CreateAsyncFromSyncIterator

## test262 Tests
- `test262/test/language/arguments-object/` — 263 tests
- `test262/test/language/function-code/` — 217 tests
- `test262/test/language/global-code/` — 42 tests
- `test262/test/language/identifier-resolution/` — 14 tests
- `test262/test/language/block-scope/` — 145 tests
- Portions of `built-ins/Object/` — 3,411 tests
