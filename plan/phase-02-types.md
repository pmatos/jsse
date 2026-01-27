# Phase 2: Types & Values

**Spec Reference:** §6 (ECMAScript Data Types and Values), §7 (Abstract Operations — Type Conversion)

## Goal
Implement all ECMAScript language types, specification types, and type conversion abstract operations.

## Tasks

### 2.1 Language Types (§6.1)
- [x] `Undefined` type
- [x] `Null` type
- [x] `Boolean` type
- [x] `String` type (UTF-16 code unit sequences)
  - [x] `StringIndexOf` abstract operation
  - [x] `StringLastIndexOf` abstract operation
- [x] `Symbol` type
  - [x] Symbol description
  - [x] Well-known symbols table (§6.1.5.1): `@@iterator`, `@@toPrimitive`, `@@toStringTag`, `@@hasInstance`, `@@species`, `@@match`, `@@replace`, `@@search`, `@@split`, `@@unscopables`, `@@isConcatSpreadable`, `@@asyncIterator`, `@@dispose`, `@@asyncDispose`, `@@matchAll`
- [x] `Number` type (§6.1.6.1)
  - [x] IEEE 754-2019 double precision
  - [x] Special values: NaN, +Infinity, -Infinity, +0, -0
  - [x] Number::unaryMinus
  - [x] Number::bitwiseNOT
  - [x] Number::exponentiate
  - [x] Number::multiply
  - [x] Number::divide
  - [x] Number::remainder
  - [x] Number::add
  - [x] Number::subtract
  - [x] Number::leftShift
  - [x] Number::signedRightShift
  - [x] Number::unsignedRightShift
  - [x] Number::lessThan
  - [x] Number::equal
  - [x] Number::sameValue
  - [x] Number::sameValueZero
  - [x] Number::bitwiseAND / bitwiseXOR / bitwiseOR
  - [x] Number::toString
- [x] `BigInt` type (§6.1.6.2)
  - [x] Arbitrary precision integers
  - [x] BigInt::unaryMinus
  - [x] BigInt::bitwiseNOT
  - [x] BigInt::exponentiate
  - [x] BigInt::multiply / divide / remainder
  - [x] BigInt::add / subtract
  - [x] BigInt::leftShift / signedRightShift / unsignedRightShift
  - [x] BigInt::lessThan / equal
  - [x] BigInt::bitwiseAND / bitwiseXOR / bitwiseOR
  - [x] BigInt::toString
- [x] `Object` type (§6.1.7) — initial representation
  - [x] Property attributes (§6.1.7.1): data vs accessor descriptors
  - [x] Internal methods table (§6.1.7.2)
  - [x] Internal slots representation
  - [x] Well-known intrinsic objects table (§6.1.7.4)

### 2.2 Specification Types (§6.2)
- [x] Enum specification type
- [x] List and Record
- [x] Set and Relation
- [x] Completion Record (normal, throw, return, break, continue)
  - [x] NormalCompletion
  - [x] ThrowCompletion
  - [x] ReturnCompletion
  - [x] UpdateEmpty
- [x] Reference Record
  - [x] IsPropertyReference
  - [x] IsUnresolvableReference
  - [x] IsSuperReference
  - [ ] IsPrivateReference (deferred: needs private class fields)
  - [x] GetValue
  - [x] PutValue
  - [x] GetThisValue
  - [x] InitializeReferencedBinding
  - [ ] MakePrivateReference (deferred: needs private class fields)
- [x] Property Descriptor
  - [x] IsAccessorDescriptor
  - [x] IsDataDescriptor
  - [x] IsGenericDescriptor
  - [x] FromPropertyDescriptor
  - [x] ToPropertyDescriptor
  - [x] CompletePropertyDescriptor
- [x] Environment Record types (placeholder — detailed in Phase 5)
- [x] Abstract Closure
- [ ] Data Blocks (deferred: needs TypedArrays)
  - [ ] CreateByteDataBlock
  - [ ] CreateSharedByteDataBlock
  - [ ] CopyDataBlockBytes
- [ ] PrivateElement specification type (deferred: needs private class fields)
- [ ] ClassFieldDefinition Record (deferred: needs class fields)
- [ ] Private Names (deferred: needs private class fields)
- [ ] ClassStaticBlockDefinition Record (deferred: needs static blocks)

### 2.3 Type Conversion Abstract Operations (§7.1)
- [x] ToPrimitive / OrdinaryToPrimitive
- [x] ToBoolean
- [x] ToNumeric
- [x] ToNumber (including StringToNumber)
  - [x] StringNumericValue runtime semantics
  - [ ] RoundMVResult
- [x] ToIntegerOrInfinity
- [x] ToInt32 / ToUint32
- [ ] ToInt16 / ToUint16 (deferred: needs TypedArrays)
- [ ] ToInt8 / ToUint8 / ToUint8Clamp (deferred: needs TypedArrays)
- [ ] ToBigInt
- [ ] StringToBigInt
- [ ] ToBigInt64 / ToBigUint64 (deferred: needs TypedArrays)
- [x] ToString
- [x] ToObject
- [x] ToPropertyKey
- [x] ToLength
- [x] CanonicalNumericIndexString
- [x] ToIndex

### 2.4 Testing Abstract Operations (§7.2)
- [x] RequireObjectCoercible
- [x] IsArray
- [x] IsCallable
- [x] IsConstructor (inline: callable.is_some() + !is_arrow check)
- [x] IsExtensible
- [x] IsIntegralNumber (inline: Number.isInteger)
- [x] IsPropertyKey
- [ ] IsRegExp
- [x] SameValue
- [x] SameValueZero
- [x] SameValueNonNumber (inline in strict equality)
- [x] IsLessThan
- [x] IsLooselyEqual
- [x] IsStrictlyEqual

### 2.5 Operations on Objects (§7.3)
- [x] MakeBasicObject (JsObjectData::new)
- [x] Get / Set (get_property / set_property_value)
- [x] CreateDataProperty / CreateMethodProperty (insert_value / insert_builtin)
- [x] CreateDataPropertyOrThrow (insert_value)
- [x] CreateNonEnumerableDataPropertyOrThrow (insert_builtin)
- [x] DefinePropertyOrThrow (define_own_property)
- [x] DeletePropertyOrThrow (delete via Object.defineProperty path)
- [x] GetMethod (inline in ToPrimitive, etc.)
- [x] HasProperty / HasOwnProperty (has_property / has_own_property)
- [x] Call (call_function)
- [x] Construct (eval_new)
- [x] SetIntegrityLevel / TestIntegrityLevel (Object.freeze / Object.isFrozen / Object.seal / Object.isSealed)
- [x] CreateArrayFromList (create_array)
- [x] LengthOfArrayLike (inline in Array methods)
- [ ] CreateListFromArrayLike
- [ ] Invoke
- [x] OrdinaryHasInstance (instanceof operator)
- [ ] SpeciesConstructor
- [x] EnumerableOwnProperties (Object.keys/values/entries)
- [ ] GetFunctionRealm
- [x] CopyDataProperties (Object.assign)
- [ ] PrivateElementFind / PrivateFieldAdd / PrivateMethodOrAccessorAdd / PrivateGet / PrivateSet
- [ ] DefineField
- [ ] InitializeInstanceElements
- [ ] AddValueToKeyedGroup
- [ ] GroupBy
- [ ] SetterThatIgnoresPrototypeProperties

## test262 Tests
- `test262/test/language/types/` — ~113 tests
- Many type conversion tests spread across `built-ins/Number`, `built-ins/String`, `built-ins/Boolean`, etc.
