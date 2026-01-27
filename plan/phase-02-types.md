# Phase 2: Types & Values

**Spec Reference:** §6 (ECMAScript Data Types and Values), §7 (Abstract Operations — Type Conversion)

## Goal
Implement all ECMAScript language types, specification types, and type conversion abstract operations.

## Tasks

### 2.1 Language Types (§6.1)
- [ ] `Undefined` type
- [ ] `Null` type
- [ ] `Boolean` type
- [ ] `String` type (UTF-16 code unit sequences)
  - [ ] `StringIndexOf` abstract operation
  - [ ] `StringLastIndexOf` abstract operation
- [ ] `Symbol` type
  - [ ] Symbol description
  - [ ] Well-known symbols table (§6.1.5.1): `@@iterator`, `@@toPrimitive`, `@@toStringTag`, `@@hasInstance`, `@@species`, `@@match`, `@@replace`, `@@search`, `@@split`, `@@unscopables`, `@@isConcatSpreadable`, `@@asyncIterator`, `@@dispose`, `@@asyncDispose`, `@@matchAll`
- [ ] `Number` type (§6.1.6.1)
  - [ ] IEEE 754-2019 double precision
  - [ ] Special values: NaN, +Infinity, -Infinity, +0, -0
  - [ ] Number::unaryMinus
  - [ ] Number::bitwiseNOT
  - [ ] Number::exponentiate
  - [ ] Number::multiply
  - [ ] Number::divide
  - [ ] Number::remainder
  - [ ] Number::add
  - [ ] Number::subtract
  - [ ] Number::leftShift
  - [ ] Number::signedRightShift
  - [ ] Number::unsignedRightShift
  - [ ] Number::lessThan
  - [ ] Number::equal
  - [ ] Number::sameValue
  - [ ] Number::sameValueZero
  - [ ] Number::bitwiseAND / bitwiseXOR / bitwiseOR
  - [ ] Number::toString
- [ ] `BigInt` type (§6.1.6.2)
  - [ ] Arbitrary precision integers
  - [ ] BigInt::unaryMinus
  - [ ] BigInt::bitwiseNOT
  - [ ] BigInt::exponentiate
  - [ ] BigInt::multiply / divide / remainder
  - [ ] BigInt::add / subtract
  - [ ] BigInt::leftShift / signedRightShift / unsignedRightShift
  - [ ] BigInt::lessThan / equal
  - [ ] BigInt::bitwiseAND / bitwiseXOR / bitwiseOR
  - [ ] BigInt::toString
- [ ] `Object` type (§6.1.7) — initial representation
  - [ ] Property attributes (§6.1.7.1): data vs accessor descriptors
  - [ ] Internal methods table (§6.1.7.2)
  - [ ] Internal slots representation
  - [ ] Well-known intrinsic objects table (§6.1.7.4)

### 2.2 Specification Types (§6.2)
- [ ] Enum specification type
- [ ] List and Record
- [ ] Set and Relation
- [ ] Completion Record (normal, throw, return, break, continue)
  - [ ] NormalCompletion
  - [ ] ThrowCompletion
  - [ ] ReturnCompletion
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
- [ ] Property Descriptor
  - [ ] IsAccessorDescriptor
  - [ ] IsDataDescriptor
  - [ ] IsGenericDescriptor
  - [ ] FromPropertyDescriptor
  - [ ] ToPropertyDescriptor
  - [ ] CompletePropertyDescriptor
- [ ] Environment Record types (placeholder — detailed in Phase 5)
- [ ] Abstract Closure
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
