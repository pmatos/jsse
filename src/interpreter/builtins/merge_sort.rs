use crate::types::JsValue;
use std::cmp::Ordering;

// Stable natural merge sort driven by a fallible comparator. A user
// comparefn need not be a consistent comparator (the resulting order is then
// implementation-defined), and `slice::sort_by` panics when it detects such a
// total-order violation, so user-driven sorts must go through this instead.
// Stops at the first `Err`, leaving `items` a permutation of its input.
//
// Adjacent elements are first grouped into maximal runs: non-descending runs
// stay as they are and strictly descending runs (each element `Greater` than
// its successor, so reversing cannot reorder equal elements) are reversed in
// place. Sorted or reverse-sorted input then costs n-1 comparefn calls.
pub(super) fn merge_sort_by<E>(
    items: &mut Vec<JsValue>,
    mut cmp: impl FnMut(&JsValue, &JsValue) -> Result<Ordering, E>,
) -> Result<(), E> {
    if items.len() < 2 {
        return Ok(());
    }
    let mut src = std::mem::take(items);
    let result = sort_runs(&mut src, &mut cmp);
    *items = src;
    result
}

fn sort_runs<E>(
    src: &mut Vec<JsValue>,
    cmp: &mut impl FnMut(&JsValue, &JsValue) -> Result<Ordering, E>,
) -> Result<(), E> {
    let mut bounds = find_runs(src, cmp)?;
    let mut dst: Vec<JsValue> = Vec::new();
    while bounds.len() > 2 {
        dst.clear();
        dst.reserve(src.len());
        merge_run_pairs(src, &mut dst, &bounds, cmp)?;
        std::mem::swap(src, &mut dst);
        // Pairs of runs became one, so every other boundary survives.
        let last = bounds.len() - 1;
        let mut index = 0;
        bounds.retain(|_| {
            index += 1;
            (index - 1) % 2 == 0
        });
        if last % 2 == 1 {
            bounds.push(src.len());
        }
    }
    Ok(())
}

// Splits `src` into maximal runs, reversing each strictly descending one, and
// returns the run boundaries (`0 ..= src.len()`). A run is reversed only after
// it is fully scanned, so an `Err` leaves `src` a permutation of its input.
fn find_runs<E>(
    src: &mut [JsValue],
    cmp: &mut impl FnMut(&JsValue, &JsValue) -> Result<Ordering, E>,
) -> Result<Vec<usize>, E> {
    let n = src.len();
    let mut bounds = Vec::with_capacity(n / 2 + 2);
    bounds.push(0);
    let mut lo = 0;
    while lo < n {
        let mut hi = lo + 1;
        if hi < n {
            let descending = cmp(&src[lo], &src[hi])? == Ordering::Greater;
            hi += 1;
            while hi < n && (cmp(&src[hi - 1], &src[hi])? == Ordering::Greater) == descending {
                hi += 1;
            }
            if descending {
                src[lo..hi].reverse();
            }
        }
        bounds.push(hi);
        lo = hi;
    }
    Ok(bounds)
}

// One pass of `sort_runs`: merges each pair of adjacent runs of `src`
// (delimited by `bounds`) onto `dst`; a trailing odd run is copied through.
fn merge_run_pairs<E>(
    src: &[JsValue],
    dst: &mut Vec<JsValue>,
    bounds: &[usize],
    cmp: &mut impl FnMut(&JsValue, &JsValue) -> Result<Ordering, E>,
) -> Result<(), E> {
    for k in (0..bounds.len() - 1).step_by(2) {
        match bounds.get(k + 2) {
            Some(&hi) => merge_runs(src, dst, bounds[k], bounds[k + 1], hi, cmp)?,
            None => dst.extend_from_slice(&src[bounds[k]..bounds[k + 1]]),
        }
    }
    Ok(())
}

// Merges the sorted runs `src[lo..mid]` and `src[mid..hi]` onto the end of `dst`.
fn merge_runs<E>(
    src: &[JsValue],
    dst: &mut Vec<JsValue>,
    lo: usize,
    mid: usize,
    hi: usize,
    cmp: &mut impl FnMut(&JsValue, &JsValue) -> Result<Ordering, E>,
) -> Result<(), E> {
    // Already in order: one comparison instead of a full merge.
    if cmp(&src[mid - 1], &src[mid])? != Ordering::Greater {
        dst.extend_from_slice(&src[lo..hi]);
        return Ok(());
    }
    let (mut i, mut j) = (lo, mid);
    while i < mid && j < hi {
        if cmp(&src[i], &src[j])? == Ordering::Greater {
            dst.push(src[j].clone());
            j += 1;
        } else {
            dst.push(src[i].clone());
            i += 1;
        }
    }
    dst.extend_from_slice(&src[i..mid]);
    dst.extend_from_slice(&src[j..hi]);
    Ok(())
}

#[cfg(test)]
mod merge_sort_tests {
    use super::merge_sort_by;
    use crate::types::JsValue;
    use std::cmp::Ordering;

    fn sorted(mut v: Vec<f64>) -> Vec<f64> {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v
    }

    fn values(items: &[JsValue]) -> Vec<f64> {
        items.iter().map(|v| v.as_number().unwrap()).collect()
    }

    // Keys are `x.floor()` so the fractional part tags equal keys' input order.
    fn sort_by_key(input: &[f64]) -> (Vec<f64>, usize) {
        let mut items: Vec<JsValue> = input.iter().map(|&n| JsValue::number(n)).collect();
        let mut calls = 0;
        merge_sort_by(&mut items, |a, b| {
            calls += 1;
            let (a, b) = (
                a.as_number().unwrap().floor(),
                b.as_number().unwrap().floor(),
            );
            Ok::<_, ()>(a.partial_cmp(&b).unwrap())
        })
        .unwrap();
        (values(&items), calls)
    }

    #[test]
    fn sorted_input_costs_n_minus_one_calls() {
        let (out, calls) = sort_by_key(&[0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(out, [0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
        assert_eq!(calls, 5);
    }

    #[test]
    fn strictly_descending_input_costs_n_minus_one_calls() {
        let input: Vec<f64> = (0..256).rev().map(f64::from).collect();
        let (out, calls) = sort_by_key(&input);
        assert_eq!(out, (0..256).map(f64::from).collect::<Vec<_>>());
        assert_eq!(calls, 255);
    }

    #[test]
    fn equal_keys_keep_input_order_inside_descending_runs() {
        let (out, _) = sort_by_key(&[3.1, 3.2, 2.1, 2.2, 1.1, 1.2, 1.3, 0.5]);
        assert_eq!(out, [0.5, 1.1, 1.2, 1.3, 2.1, 2.2, 3.1, 3.2]);
    }

    #[test]
    fn mixed_runs_are_sorted_stably() {
        let (out, _) = sort_by_key(&[1.1, 3.1, 5.1, 4.1, 2.1, 2.2, 6.1, 0.1, 6.2, 6.3]);
        assert_eq!(out, [0.1, 1.1, 2.1, 2.2, 3.1, 4.1, 5.1, 6.1, 6.2, 6.3]);
    }

    #[test]
    fn inconsistent_comparator_yields_a_permutation_without_panicking() {
        let mut items: Vec<JsValue> = (0..200)
            .map(|n| JsValue::number(f64::from(n % 17)))
            .collect();
        let mut flip = 0u32;
        merge_sort_by(&mut items, |_, _| {
            flip += 1;
            Ok::<_, ()>(if flip % 2 == 0 {
                Ordering::Less
            } else {
                Ordering::Greater
            })
        })
        .unwrap();
        let expected: Vec<f64> = (0..200).map(|n| f64::from(n % 17)).collect();
        assert_eq!(sorted(values(&items)), sorted(expected));
    }

    #[test]
    fn an_error_mid_sort_leaves_a_permutation_and_stops_calling() {
        let input = [5.0, 3.0, 9.0, 1.0, 7.0, 2.0, 8.0, 4.0, 6.0, 0.0, 11.0, 10.0];
        let total_calls = sort_by_key(&input).1;
        for fail_at in 1..=total_calls {
            let mut items: Vec<JsValue> = input.iter().map(|&n| JsValue::number(n)).collect();
            let mut calls = 0;
            let result = merge_sort_by(&mut items, |a, b| {
                calls += 1;
                if calls == fail_at {
                    return Err(());
                }
                Ok(a.as_number()
                    .unwrap()
                    .partial_cmp(&b.as_number().unwrap())
                    .unwrap())
            });
            assert!(result.is_err(), "fail_at {fail_at}");
            assert_eq!(calls, fail_at, "no calls after the error");
            assert_eq!(sorted(values(&items)), sorted(input.to_vec()));
        }
    }
}
