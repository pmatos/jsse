# Phase 5: Runtime Core

**Spec Reference:** §9 (Environment Records), §10 (Execution Contexts), §6.1.7 (Object Internals), §7.3 (Object Operations), §8 (Syntax-Directed Operations)

## Goal
Implement the runtime infrastructure: environments, execution contexts, ordinary/exotic objects, and all abstract operations needed to begin evaluating code.

## Tasks

### 5.1 Environment Records (§9.1)
- [ ] Declarative Environment Record
  - [ ] HasBinding, CreateMutableBinding, CreateImmutableBinding
  - [ ] InitializeBinding, SetMutableBinding, GetBindingValue
  - [ ] DeleteBinding, HasThisBinding, HasSuperBinding, WithBaseObject
- [ ] Object Environment Record
  - [ ] All concrete methods
  - [ ] `withEnvironment` flag
- [ ] Function Environment Record
  - [ ] `[[ThisValue]]`, `[[ThisBindingStatus]]`
  - [ ] BindThisValue, GetThisBinding, GetSuperBase
- [ ] Global Environment Record
  - [ ] HasBinding, CreateMutableBinding, CreateImmutableBinding
  - [ ] InitializeBinding, SetMutableBinding, GetBindingValue
  - [ ] DeleteBinding, HasThisBinding, GetThisBinding
  - [ ] HasVarDeclaration, HasLexicalDeclaration, HasRestrictedGlobalProperty
  - [ ] CanDeclareGlobalVar, CanDeclareGlobalFunction
  - [ ] CreateGlobalVarBinding, CreateGlobalFunctionBinding
- [ ] Module Environment Record
  - [ ] CreateImportBinding
  - [ ] GetBindingValue (indirection for imports)
  - [ ] GetThisBinding (returns undefined)

### 5.2 Environment Record Operations (§9.1.2)
- [ ] GetIdentifierReference
- [ ] NewDeclarativeEnvironment
- [ ] NewObjectEnvironment
- [ ] NewFunctionEnvironment
- [ ] NewGlobalEnvironment
- [ ] NewModuleEnvironment

### 5.3 PrivateEnvironment Records (§9.2)
- [ ] PrivateEnvironment Record
- [ ] NewPrivateEnvironment
- [ ] ResolvePrivateIdentifier

### 5.4 Realms (§9.3)
- [ ] Realm Record
- [ ] CreateRealm
- [ ] CreateIntrinsics
- [ ] SetRealmGlobalObject
- [ ] SetDefaultGlobalBindings

### 5.5 Execution Contexts (§9.4)
- [ ] Execution context stack
- [ ] Running execution context
- [ ] LexicalEnvironment / VariableEnvironment / PrivateEnvironment
- [ ] Function execution context
- [ ] Script/Module execution context
- [ ] GetActiveScriptOrModule
- [ ] ResolveBinding
- [ ] GetThisEnvironment
- [ ] ResolveThisBinding
- [ ] GetNewTarget
- [ ] GetGlobalObject

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
- [ ] `[[GetPrototypeOf]]` / OrdinaryGetPrototypeOf
- [ ] `[[SetPrototypeOf]]` / OrdinarySetPrototypeOf
- [ ] `[[IsExtensible]]` / OrdinaryIsExtensible
- [ ] `[[PreventExtensions]]` / OrdinaryPreventExtensions
- [ ] `[[GetOwnProperty]]` / OrdinaryGetOwnProperty
- [ ] `[[DefineOwnProperty]]` / OrdinaryDefineOwnProperty / ValidateAndApplyPropertyDescriptor
- [ ] `[[HasProperty]]` / OrdinaryHasProperty
- [ ] `[[Get]]` / OrdinaryGet
- [ ] `[[Set]]` / OrdinarySet / OrdinarySetWithOwnDescriptor
- [ ] `[[Delete]]` / OrdinaryDelete
- [ ] `[[OwnPropertyKeys]]` / OrdinaryOwnPropertyKeys
- [ ] OrdinaryObjectCreate
- [ ] OrdinaryCreateFromConstructor
- [ ] GetPrototypeFromConstructor

### 5.9 ECMAScript Function Objects (§10.2)
- [ ] Function internal slots (`[[Environment]]`, `[[FormalParameters]]`, `[[ECMAScriptCode]]`, `[[ConstructorKind]]`, `[[Realm]]`, `[[ScriptOrModule]]`, `[[ThisMode]]`, `[[Strict]]`, `[[HomeObject]]`, `[[SourceText]]`, `[[Fields]]`, `[[PrivateMethods]]`, `[[ClassFieldInitializerName]]`, `[[IsClassConstructor]]`)
- [ ] `[[Call]]` internal method
- [ ] `[[Construct]]` internal method
- [ ] OrdinaryFunctionCreate
- [ ] MakeConstructor
- [ ] MakeClassConstructor
- [ ] MakeMethod
- [ ] SetFunctionName
- [ ] SetFunctionLength
- [ ] FunctionDeclarationInstantiation

### 5.10 Built-in Function Objects (§10.3)
- [ ] Built-in function as abstract closure
- [ ] CreateBuiltinFunction
- [ ] Built-in `[[Call]]` behavior

### 5.11 Exotic Objects (§10.4)
- [ ] Bound Function Exotic Objects (§10.4.1)
  - [ ] `[[Call]]`, `[[Construct]]`
  - [ ] BoundFunctionCreate
- [ ] Array Exotic Objects (§10.4.2)
  - [ ] `[[DefineOwnProperty]]` with array length semantics
  - [ ] ArrayCreate, ArraySpeciesCreate
- [ ] String Exotic Objects (§10.4.3)
  - [ ] `[[GetOwnProperty]]`, `[[DefineOwnProperty]]`, `[[OwnPropertyKeys]]`
  - [ ] StringCreate, StringGetOwnProperty
- [ ] Arguments Exotic Objects (§10.4.4)
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
