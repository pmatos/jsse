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
  - [ ] Internal methods table (§6.1.7.2)
  - [ ] Internal slots representation
  - [ ] Well-known intrinsic objects table (§6.1.7.4)

### 2.2 Specification Types (§6.2)
- [x] Enum specification type
- [x] List and Record
- [ ] Set and Relation
- [x] Completion Record (normal, throw, return, break, continue)
  - [x] NormalCompletion
  - [x] ThrowCompletion
  - [x] ReturnCompletion
  - [ ] UpdateEmpty
- [ ] Reference Record
  - [ ] IsPropertyReference
  - [ ] IsUnresolvableReference
  - [ ] IsSuperReference
  - [ ] IsPrivateReference
  - [ ] GetValue
  - [ ] PutValue
  - [ ] GetThisValue
  - [ ] InitializeReferencedBinding
  - [ ] MakePrivateReference
- [x] Property Descriptor
  - [x] IsAccessorDescriptor
  - [x] IsDataDescriptor
  - [ ] IsGenericDescriptor
  - [x] FromPropertyDescriptor
  - [x] ToPropertyDescriptor
  - [ ] CompletePropertyDescriptor
- [x] Environment Record types (placeholder — detailed in Phase 5)
- [x] Abstract Closure
- [ ] Data Blocks
  - [ ] CreateByteDataBlock
  - [ ] CreateSharedByteDataBlock
  - [ ] CopyDataBlockBytes
- [ ] PrivateElement specification type
- [ ] ClassFieldDefinition Record
- [ ] Private Names
- [ ] ClassStaticBlockDefinition Record

### 2.3 Type Conversion Abstract Operations (§7.1)
- [ ] ToPrimitive / OrdinaryToPrimitive
- [ ] ToBoolean
- [ ] ToNumeric
- [ ] ToNumber (including StringToNumber)
  - [ ] StringNumericValue runtime semantics
  - [ ] RoundMVResult
- [ ] ToIntegerOrInfinity
- [ ] ToInt32 / ToUint32
- [ ] ToInt16 / ToUint16
- [ ] ToInt8 / ToUint8 / ToUint8Clamp
- [ ] ToBigInt
- [ ] StringToBigInt
- [ ] ToBigInt64 / ToBigUint64
- [ ] ToString
- [ ] ToObject
- [ ] ToPropertyKey
- [ ] ToLength
- [ ] CanonicalNumericIndexString
- [ ] ToIndex

### 2.4 Testing Abstract Operations (§7.2)
- [ ] RequireObjectCoercible
- [ ] IsArray
- [ ] IsCallable
- [ ] IsConstructor
- [ ] IsExtensible
- [ ] IsIntegralNumber
- [ ] IsPropertyKey
- [ ] IsRegExp
- [ ] SameValue
- [ ] SameValueZero
- [ ] SameValueNonNumber
- [ ] IsLessThan
- [ ] IsLooselyEqual
- [ ] IsStrictlyEqual

### 2.5 Operations on Objects (§7.3)
- [ ] MakeBasicObject
- [ ] Get / Set
- [ ] CreateDataProperty / CreateMethodProperty
- [ ] CreateDataPropertyOrThrow
- [ ] CreateNonEnumerableDataPropertyOrThrow
- [ ] DefinePropertyOrThrow
- [ ] DeletePropertyOrThrow
- [ ] GetMethod
- [ ] HasProperty / HasOwnProperty
- [ ] Call
- [ ] Construct
- [ ] SetIntegrityLevel / TestIntegrityLevel
- [ ] CreateArrayFromList
- [ ] LengthOfArrayLike
- [ ] CreateListFromArrayLike
- [ ] Invoke
- [ ] OrdinaryHasInstance
- [ ] SpeciesConstructor
- [ ] EnumerableOwnProperties
- [ ] GetFunctionRealm
- [ ] CopyDataProperties
- [ ] PrivateElementFind / PrivateFieldAdd / PrivateMethodOrAccessorAdd / PrivateGet / PrivateSet
- [ ] DefineField
- [ ] InitializeInstanceElements
- [ ] AddValueToKeyedGroup
- [ ] GroupBy
- [ ] SetterThatIgnoresPrototypeProperties

## test262 Tests
- `test262/test/language/types/` — ~113 tests
- Many type conversion tests spread across `built-ins/Number`, `built-ins/String`, `built-ins/Boolean`, etc.
