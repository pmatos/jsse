use super::super::*;

/// Convert f64 to IEEE 754 binary16 (half-precision) and back to f64.
/// Uses round-to-nearest-even (banker's rounding).
fn f64_to_f16_to_f64(val: f64) -> f64 {
    if val.is_nan() {
        return f64::NAN;
    }
    if !val.is_finite() || val == 0.0 {
        return val;
    }
    let bits = val.to_bits();
    let sign = (bits >> 63) as u16;
    let exp = ((bits >> 52) & 0x7FF) as i32;
    let frac = bits & 0x000F_FFFF_FFFF_FFFF;

    let f16_sign = sign << 15;
    let unbiased = exp - 1023;

    if unbiased > 15 {
        return f64::from_bits((sign as u64) << 63 | 0x7FF0_0000_0000_0000);
    }

    if unbiased >= -14 {
        // Normal f16: shift 52-bit mantissa to 10-bit, round lower 42 bits
        let f16_exp = ((unbiased + 15) as u16) << 10;
        let mantissa_10 = (frac >> 42) as u16;
        let round_bits = frac & 0x3FF_FFFF_FFFF;
        let halfway = 0x200_0000_0000_u64;

        let rounded = if round_bits > halfway {
            mantissa_10 + 1
        } else if round_bits == halfway {
            if mantissa_10 & 1 != 0 {
                mantissa_10 + 1
            } else {
                mantissa_10
            }
        } else {
            mantissa_10
        };

        let f16_bits = f16_sign | f16_exp | (rounded & 0x3FF);
        let f16_bits = if rounded > 0x3FF {
            f16_bits + (1 << 10)
        } else {
            f16_bits
        };
        return f16_to_f64(f16_bits);
    }

    // Subnormal f16: unbiased exponent < -14
    let shift = (-14 - unbiased) as u64;
    let full = (1_u64 << 52) | frac; // 53-bit mantissa with implicit 1
    let total_shift = 42 + shift;

    if total_shift >= 53 {
        if total_shift == 53 {
            // Round bit is the implicit 1 bit
            if frac > 0 {
                return f16_to_f64(f16_sign | 1);
            }
            return f16_to_f64(f16_sign);
        }
        return f16_to_f64(f16_sign);
    }

    let mantissa = ((full >> total_shift) & 0x3FF) as u16;
    let round_bit_pos = total_shift - 1;
    let round_bit = (full >> round_bit_pos) & 1;
    let sticky = if round_bit_pos > 0 {
        full & ((1_u64 << round_bit_pos) - 1)
    } else {
        0
    };
    let rounded = if round_bit == 1 {
        if sticky > 0 || (mantissa & 1 != 0) {
            mantissa + 1
        } else {
            mantissa
        }
    } else {
        mantissa
    };

    if rounded >= 0x400 {
        return f16_to_f64(f16_sign | (1 << 10));
    }
    f16_to_f64(f16_sign | rounded)
}

fn f16_to_f64(bits: u16) -> f64 {
    let sign = ((bits >> 15) & 1) as u64;
    let exp = ((bits >> 10) & 0x1F) as u64;
    let frac = (bits & 0x3FF) as u64;

    if exp == 0 {
        if frac == 0 {
            return f64::from_bits(sign << 63);
        }
        // Subnormal: value = frac * 2^(-24)
        // Normalize by finding leading 1 position and shifting
        let mut shifts = 0_i32;
        let mut f = frac;
        while f & 0x400 == 0 {
            f <<= 1;
            shifts += 1;
        }
        // After shifting, f has bit 10 set (the implicit 1)
        // Unbiased exponent = -14 - shifts, biased = 1023 + (-14 - shifts)
        let f64_exp = (1023 - 14 - shifts) as u64;
        let f64_frac = (f & 0x3FF) << 42;
        return f64::from_bits((sign << 63) | (f64_exp << 52) | f64_frac);
    }

    if exp == 31 {
        if frac == 0 {
            return f64::from_bits((sign << 63) | 0x7FF0_0000_0000_0000);
        }
        return f64::from_bits((sign << 63) | 0x7FF8_0000_0000_0000 | (frac << 42));
    }

    let f64_exp = (exp as i32 - 15 + 1023) as u64;
    let f64_frac = frac << 42;
    f64::from_bits((sign << 63) | (f64_exp << 52) | f64_frac)
}

/// Exact summation using Shewchuk's algorithm with overflow-safe interleaved ordering.
/// Exact summation using BigInt arithmetic with correct round-to-nearest-even.
fn sum_precise_shewchuk(vals: &[f64]) -> f64 {
    use num_bigint::BigInt;
    use num_bigint::Sign;

    if vals.is_empty() {
        return 0.0;
    }

    // Find the minimum exponent to use as a common scale
    let mut min_exp = i32::MAX;
    for &v in vals {
        if v == 0.0 {
            continue;
        }
        let bits = v.to_bits();
        let biased_exp = ((bits >> 52) & 0x7FF) as i32;
        let exp = if biased_exp == 0 {
            -1074
        } else {
            biased_exp - 1023 - 52
        };
        if exp < min_exp {
            min_exp = exp;
        }
    }
    if min_exp == i32::MAX {
        return 0.0;
    }

    // Sum as big integers scaled to 2^min_exp
    let mut total = BigInt::from(0);
    for &v in vals {
        if v == 0.0 {
            continue;
        }
        let bits = v.to_bits();
        let sign_bit = bits >> 63;
        let biased_exp = ((bits >> 52) & 0x7FF) as i32;
        let mantissa_bits = bits & 0x000F_FFFF_FFFF_FFFF;
        let (mantissa, exp) = if biased_exp == 0 {
            (mantissa_bits, -1074_i32)
        } else {
            (mantissa_bits | (1_u64 << 52), biased_exp - 1023 - 52)
        };
        let mut big_m = BigInt::from(mantissa);
        let shift = exp - min_exp;
        if shift > 0 {
            big_m <<= shift as u64;
        }
        if sign_bit != 0 {
            total -= big_m;
        } else {
            total += big_m;
        }
    }

    if total == BigInt::from(0) {
        return 0.0;
    }

    let (sign, mag) = total.into_parts();
    let is_negative = sign == Sign::Minus;
    let total_abs = BigInt::from_biguint(Sign::Plus, mag);
    let bit_len = total_abs.bits() as i32;
    let unbiased_exp = min_exp + bit_len - 1;

    // Check overflow
    if unbiased_exp > 1023 {
        return if is_negative {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }

    // Check if it fits in the subnormal/normal range
    // For normal f64: need 53 mantissa bits, exponent >= -1022
    // For subnormal: exponent = -1022, mantissa has (unbiased_exp + 1074) bits

    let available_mantissa_bits = if unbiased_exp >= -1022 {
        53 // normal
    } else {
        // subnormal: mantissa bits = unbiased_exp + 1074
        let b = unbiased_exp + 1075;
        if b <= 0 {
            // Below smallest subnormal — need to check rounding to smallest subnormal
            // The value = total_abs * 2^min_exp; smallest subnormal = 2^(-1074)
            // Halfway = 2^(-1075)
            // Our value in units of 2^(-1075) = total_abs * 2^(min_exp + 1075)
            let shift_to_half = min_exp + 1075;
            if shift_to_half < 0 {
                return if is_negative { -0.0 } else { 0.0 };
            }
            let scaled = &total_abs << shift_to_half as u64;
            let one = BigInt::from(1);
            return if scaled > one {
                if is_negative { -5e-324_f64 } else { 5e-324_f64 }
            } else if scaled == one {
                // Tie: round to even. 0 is even, smallest subnormal (1) is odd -> round to 0
                if is_negative { -0.0 } else { 0.0 }
            } else if is_negative {
                -0.0
            } else {
                0.0
            };
        }
        b
    };

    if bit_len <= available_mantissa_bits {
        // Exact representation
        let n: u64 = total_abs.iter_u64_digits().next().unwrap_or(0);
        let val = (n as f64) * (2.0_f64).powi(min_exp);
        return if is_negative { -val } else { val };
    }

    // Extract top (available_mantissa_bits + 1) bits: mantissa + round bit
    // Then sticky = whether any bits below round bit are set
    let round_shift = bit_len - available_mantissa_bits - 1; // bits to discard below round bit
    if round_shift < 0 {
        // Shouldn't happen since bit_len > available_mantissa_bits
        let n: u64 = total_abs.iter_u64_digits().next().unwrap_or(0);
        let val = (n as f64) * (2.0_f64).powi(min_exp);
        return if is_negative { -val } else { val };
    }
    let shifted = &total_abs >> (round_shift as u64);
    let top_val: u64 = shifted.iter_u64_digits().next().unwrap_or(0);
    let mantissa = top_val >> 1; // available_mantissa_bits bits (including implicit 1)
    let round_bit = top_val & 1;
    let sticky = if round_shift > 0 {
        let mask = (BigInt::from(1) << (round_shift as u64)) - 1;
        (&total_abs & mask) != BigInt::from(0)
    } else {
        false
    };

    let rounded = if round_bit == 1 {
        if sticky || (mantissa & 1 != 0) {
            mantissa + 1
        } else {
            mantissa
        }
    } else {
        mantissa
    };

    // Handle mantissa overflow from rounding
    let (final_mantissa, final_exp) = if rounded >= (1_u64 << available_mantissa_bits as u64) {
        (rounded >> 1, unbiased_exp + 1)
    } else {
        (rounded, unbiased_exp)
    };

    if final_exp > 1023 {
        return if is_negative {
            f64::NEG_INFINITY
        } else {
            f64::INFINITY
        };
    }

    let sign_bit = if is_negative { 1_u64 } else { 0 };
    if final_exp >= -1022 {
        // Normal
        let biased_exp = (final_exp + 1023) as u64;
        let mantissa_bits = final_mantissa & 0x000F_FFFF_FFFF_FFFF;
        f64::from_bits((sign_bit << 63) | (biased_exp << 52) | mantissa_bits)
    } else {
        // Subnormal
        let mantissa_bits = final_mantissa & 0x000F_FFFF_FFFF_FFFF;
        f64::from_bits((sign_bit << 63) | mantissa_bits)
    }
}

impl Interpreter {
    pub(super) fn setup_math(&mut self) {
        // Math object
        let math_obj_id = self.create_object_id();
        let math_id = math_obj_id;
        {
            let mut m = self.get_object_cell_expect(math_obj_id).borrow_mut();
            m.class_name = "Math".to_string();
            let math_consts: &[(&str, f64)] = &[
                ("PI", std::f64::consts::PI),
                ("E", std::f64::consts::E),
                ("LN2", std::f64::consts::LN_2),
                ("LN10", std::f64::consts::LN_10),
                ("LOG2E", std::f64::consts::LOG2_E),
                ("LOG10E", std::f64::consts::LOG10_E),
                ("SQRT2", std::f64::consts::SQRT_2),
                ("SQRT1_2", std::f64::consts::FRAC_1_SQRT_2),
            ];
            for (name, val) in math_consts {
                m.insert_property(
                    name.to_string(),
                    PropertyDescriptor::data(JsValue::number(*val), false, false, false),
                );
            }
        }
        self.add_math_unary_methods(math_obj_id);
        self.add_math_rounding_and_extrema_methods(math_obj_id);
        self.add_math_misc_methods(math_obj_id);

        // @@toStringTag
        self.define_to_string_tag(math_obj_id, "Math");

        let math_val = JsValue::object(math_id);
        self.realm()
            .global_env
            .borrow_mut()
            .declare("Math", BindingKind::Const);
        self.realm()
            .global_env
            .borrow_mut()
            .initialize_binding("Math", math_val);
    }

    fn add_math_unary_methods(&mut self, math_obj_id: u64) {
        // Add Math methods
        #[allow(clippy::type_complexity)]
        let math_fns: Vec<(&str, fn(f64) -> f64)> = vec![
            ("abs", f64::abs),
            ("ceil", f64::ceil),
            ("floor", f64::floor),
            ("sqrt", f64::sqrt),
            ("sin", f64::sin),
            ("cos", f64::cos),
            ("tan", f64::tan),
            ("log", f64::ln),
            ("exp", f64::exp),
            ("asin", f64::asin),
            ("acos", f64::acos),
            ("atan", f64::atan),
            ("trunc", f64::trunc),
            (
                "sign",
                (|x: f64| {
                    if x.is_nan() || x == 0.0 {
                        x
                    } else if x > 0.0 {
                        1.0
                    } else {
                        -1.0
                    }
                }) as fn(f64) -> f64,
            ),
            ("cbrt", f64::cbrt),
        ];
        for (name, op) in math_fns {
            let fn_val = self.create_function(JsFunction::native(
                name.to_string(),
                1,
                move |interp, _this, args| {
                    let x =
                        interp.to_number_coerce(args.first().unwrap_or(JsValue::undefined_ref()));
                    Completion::Normal(JsValue::number(op(x)))
                },
            ));
            self.get_object_cell_expect(math_obj_id)
                .borrow_mut()
                .insert_builtin(name.to_string(), fn_val);
        }
    }

    fn add_math_rounding_and_extrema_methods(&mut self, math_obj_id: u64) {
        // Math.round — spec-correct: -0 for x in [-0.5,0), +0 for x in [0,0.5), integer shortcut
        let round_fn = self.create_function(JsFunction::native(
            "round".to_string(),
            1,
            |interp, _this, args| {
                let x = interp.to_number_coerce(args.first().unwrap_or(JsValue::undefined_ref()));
                let result = if x.is_nan() || x.is_infinite() || x == 0.0 {
                    x
                } else if (-0.5..0.0).contains(&x) {
                    -0.0_f64
                } else if (0.0..0.5).contains(&x) {
                    0.0_f64
                } else if x.fract() == 0.0 {
                    x
                } else {
                    (x + 0.5).floor()
                };
                Completion::Normal(JsValue::number(result))
            },
        ));
        self.get_object_cell_expect(math_obj_id)
            .borrow_mut()
            .insert_builtin("round".to_string(), round_fn);
        // Math.max, Math.min, Math.pow, Math.random, Math.atan2
        let max_fn = self.create_function(JsFunction::native(
            "max".to_string(),
            2,
            |interp, _this, args| {
                if args.is_empty() {
                    return Completion::Normal(JsValue::number(f64::NEG_INFINITY));
                }
                // Coerce all args first (spec step 2), propagating errors
                let mut coerced = Vec::with_capacity(args.len());
                for a in args {
                    match interp.to_number_value(a) {
                        Ok(n) => coerced.push(n),
                        Err(e) => return Completion::Throw(e),
                    }
                }
                let mut result = f64::NEG_INFINITY;
                for n in coerced {
                    if n.is_nan() {
                        return Completion::Normal(JsValue::number(f64::NAN));
                    }
                    // +0 is considered greater than -0
                    if n > result
                        || (n == 0.0
                            && result == 0.0
                            && !n.is_sign_negative()
                            && result.is_sign_negative())
                    {
                        result = n;
                    }
                }
                Completion::Normal(JsValue::number(result))
            },
        ));
        self.get_object_cell_expect(math_obj_id)
            .borrow_mut()
            .insert_builtin("max".to_string(), max_fn);
        let min_fn = self.create_function(JsFunction::native(
            "min".to_string(),
            2,
            |interp, _this, args| {
                if args.is_empty() {
                    return Completion::Normal(JsValue::number(f64::INFINITY));
                }
                // Coerce all args first (spec step 2), propagating errors
                let mut coerced = Vec::with_capacity(args.len());
                for a in args {
                    match interp.to_number_value(a) {
                        Ok(n) => coerced.push(n),
                        Err(e) => return Completion::Throw(e),
                    }
                }
                let mut result = f64::INFINITY;
                for n in coerced {
                    if n.is_nan() {
                        return Completion::Normal(JsValue::number(f64::NAN));
                    }
                    // -0 is considered less than +0
                    if n < result
                        || (n == 0.0
                            && result == 0.0
                            && n.is_sign_negative()
                            && !result.is_sign_negative())
                    {
                        result = n;
                    }
                }
                Completion::Normal(JsValue::number(result))
            },
        ));
        self.get_object_cell_expect(math_obj_id)
            .borrow_mut()
            .insert_builtin("min".to_string(), min_fn);
        let pow_fn = self.create_function(JsFunction::native(
            "pow".to_string(),
            2,
            |interp, _this, args| {
                let base =
                    interp.to_number_coerce(args.first().unwrap_or(JsValue::undefined_ref()));
                let exp = interp.to_number_coerce(args.get(1).unwrap_or(JsValue::undefined_ref()));
                Completion::Normal(JsValue::number(number_ops::exponentiate(base, exp)))
            },
        ));
        self.get_object_cell_expect(math_obj_id)
            .borrow_mut()
            .insert_builtin("pow".to_string(), pow_fn);
        let random_fn = self.create_function(JsFunction::native(
            "random".to_string(),
            0,
            |interp, _this, _args| {
                Completion::Normal(JsValue::number(interp.realm_mut().math_random()))
            },
        ));
        self.get_object_cell_expect(math_obj_id)
            .borrow_mut()
            .insert_builtin("random".to_string(), random_fn);
    }

    fn add_math_misc_methods(&mut self, math_obj_id: u64) {
        // Math.atan2
        let atan2_fn = self.create_function(JsFunction::native(
            "atan2".to_string(),
            2,
            |interp, _this, args| {
                let y = interp.to_number_coerce(args.first().unwrap_or(JsValue::undefined_ref()));
                let x = interp.to_number_coerce(args.get(1).unwrap_or(JsValue::undefined_ref()));
                Completion::Normal(JsValue::number(y.atan2(x)))
            },
        ));
        self.get_object_cell_expect(math_obj_id)
            .borrow_mut()
            .insert_builtin("atan2".to_string(), atan2_fn);

        // Math.hypot — coerce all args first, then check infinity before NaN (spec §21.3.2.18)
        let hypot_fn = self.create_function(JsFunction::native(
            "hypot".to_string(),
            2,
            |interp, _this, args| {
                if args.is_empty() {
                    return Completion::Normal(JsValue::number(0.0));
                }
                // Step 1-2: coerce all args to numbers first, propagating errors
                let mut coerced = Vec::with_capacity(args.len());
                for a in args {
                    match interp.to_number_value(a) {
                        Ok(n) => coerced.push(n),
                        Err(e) => return Completion::Throw(e),
                    }
                }
                // Step 3: check for infinity before NaN
                let mut has_nan = false;
                let mut sum = 0.0f64;
                for n in &coerced {
                    if n.is_infinite() {
                        return Completion::Normal(JsValue::number(f64::INFINITY));
                    }
                    if n.is_nan() {
                        has_nan = true;
                    } else {
                        sum += n * n;
                    }
                }
                if has_nan {
                    return Completion::Normal(JsValue::number(f64::NAN));
                }
                Completion::Normal(JsValue::number(sum.sqrt()))
            },
        ));
        self.get_object_cell_expect(math_obj_id)
            .borrow_mut()
            .insert_builtin("hypot".to_string(), hypot_fn);

        // Math.log2, Math.log10
        let log2_fn = self.create_function(JsFunction::native(
            "log2".to_string(),
            1,
            |_interp, _this, args| {
                let x = args.first().map(to_number).unwrap_or(f64::NAN);
                Completion::Normal(JsValue::number(x.log2()))
            },
        ));
        self.get_object_cell_expect(math_obj_id)
            .borrow_mut()
            .insert_builtin("log2".to_string(), log2_fn);
        let log10_fn = self.create_function(JsFunction::native(
            "log10".to_string(),
            1,
            |_interp, _this, args| {
                let x = args.first().map(to_number).unwrap_or(f64::NAN);
                Completion::Normal(JsValue::number(x.log10()))
            },
        ));
        self.get_object_cell_expect(math_obj_id)
            .borrow_mut()
            .insert_builtin("log10".to_string(), log10_fn);

        // Math.fround
        let fround_fn = self.create_function(JsFunction::native(
            "fround".to_string(),
            1,
            |interp, _this, args| {
                let x = interp.to_number_coerce(args.first().unwrap_or(JsValue::undefined_ref()));
                Completion::Normal(JsValue::number((x as f32) as f64))
            },
        ));
        self.get_object_cell_expect(math_obj_id)
            .borrow_mut()
            .insert_builtin("fround".to_string(), fround_fn);

        // Math.clz32
        let clz32_fn = self.create_function(JsFunction::native(
            "clz32".to_string(),
            1,
            |interp, _this, args| {
                let x = interp.to_number_coerce(args.first().unwrap_or(JsValue::undefined_ref()));
                let n = number_ops::to_uint32(x);
                Completion::Normal(JsValue::number(n.leading_zeros() as f64))
            },
        ));
        self.get_object_cell_expect(math_obj_id)
            .borrow_mut()
            .insert_builtin("clz32".to_string(), clz32_fn);

        // Math.imul
        let imul_fn = self.create_function(JsFunction::native(
            "imul".to_string(),
            2,
            |_interp, _this, args| {
                let a = args.first().map(to_number).unwrap_or(0.0);
                let b = args.get(1).map(to_number).unwrap_or(0.0);
                let ia = number_ops::to_int32(a);
                let ib = number_ops::to_int32(b);
                Completion::Normal(JsValue::number(ia.wrapping_mul(ib) as f64))
            },
        ));
        self.get_object_cell_expect(math_obj_id)
            .borrow_mut()
            .insert_builtin("imul".to_string(), imul_fn);

        // Math.expm1, Math.log1p, Math.cosh, Math.sinh, Math.tanh, Math.acosh, Math.asinh, Math.atanh
        #[allow(clippy::type_complexity)]
        let extra_math_fns: Vec<(&str, fn(f64) -> f64)> = vec![
            ("expm1", f64::exp_m1),
            ("log1p", f64::ln_1p),
            ("cosh", f64::cosh),
            ("sinh", f64::sinh),
            ("tanh", f64::tanh),
            ("acosh", f64::acosh),
            ("asinh", f64::asinh),
            ("atanh", f64::atanh),
        ];
        for (name, op) in extra_math_fns {
            let fn_val = self.create_function(JsFunction::native(
                name.to_string(),
                1,
                move |interp, _this, args| {
                    let x =
                        interp.to_number_coerce(args.first().unwrap_or(JsValue::undefined_ref()));
                    Completion::Normal(JsValue::number(op(x)))
                },
            ));
            self.get_object_cell_expect(math_obj_id)
                .borrow_mut()
                .insert_builtin(name.to_string(), fn_val);
        }

        // Math.f16round
        let f16round_fn = self.create_function(JsFunction::native(
            "f16round".to_string(),
            1,
            |interp, _this, args| {
                let x = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let n = match interp.to_number_value(&x) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                Completion::Normal(JsValue::number(f64_to_f16_to_f64(n)))
            },
        ));
        self.get_object_cell_expect(math_obj_id)
            .borrow_mut()
            .insert_builtin("f16round".to_string(), f16round_fn);

        // Math.sumPrecise
        let sum_precise_fn = self.create_function(JsFunction::native(
            "sumPrecise".to_string(),
            1,
            |interp, _this, args| {
                let items = args.first().cloned().unwrap_or(JsValue::UNDEFINED);
                let iterator = match interp.get_iterator(&items) {
                    Ok(v) => v,
                    Err(e) => return Completion::Throw(e),
                };
                let mut has_nan = false;
                let mut pos_inf: u32 = 0;
                let mut neg_inf: u32 = 0;
                let mut has_non_neg_zero = false;
                let mut finite_vals: Vec<f64> = Vec::new();
                loop {
                    let next = match interp.iterator_step(&iterator) {
                        Ok(Some(v)) => v,
                        Ok(None) => break,
                        Err(e) => return Completion::Throw(e),
                    };
                    let value = match interp.iterator_value(&next) {
                        Ok(v) => v,
                        Err(e) => {
                            return Completion::Throw(interp.iterator_close(&iterator, e));
                        }
                    };
                    let Some(n) = value.as_number() else {
                        let err = interp
                            .create_type_error("Math.sumPrecise requires all values to be Numbers");
                        return Completion::Throw(interp.iterator_close(&iterator, err));
                    };
                    if n.is_nan() {
                        has_nan = true;
                    } else if n.is_infinite() {
                        if n > 0.0 {
                            pos_inf += 1;
                        } else {
                            neg_inf += 1;
                        }
                    } else if n == 0.0 {
                        if !n.is_sign_negative() {
                            has_non_neg_zero = true;
                        }
                    } else {
                        has_non_neg_zero = true;
                        finite_vals.push(n);
                    }
                }
                if has_nan || (pos_inf > 0 && neg_inf > 0) {
                    return Completion::Normal(JsValue::number(f64::NAN));
                }
                if pos_inf > 0 {
                    return Completion::Normal(JsValue::number(f64::INFINITY));
                }
                if neg_inf > 0 {
                    return Completion::Normal(JsValue::number(f64::NEG_INFINITY));
                }
                let result = sum_precise_shewchuk(&finite_vals);
                if result == 0.0 && !has_non_neg_zero {
                    Completion::Normal(JsValue::number(-0.0))
                } else {
                    Completion::Normal(JsValue::number(result))
                }
            },
        ));
        self.get_object_cell_expect(math_obj_id)
            .borrow_mut()
            .insert_builtin("sumPrecise".to_string(), sum_precise_fn);
    }
}
