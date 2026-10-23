use crate::types::JsValue;

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
    mut cmp: impl FnMut(&JsValue, &JsValue) -> Result<std::cmp::Ordering, E>,
) -> Result<(), E> {
    let n = items.len();
    if n < 2 {
        return Ok(());
    }
    let mut src = std::mem::take(items);
    let mut bounds = match find_runs(&mut src, &mut cmp) {
        Ok(bounds) => bounds,
        Err(e) => {
            *items = src;
            return Err(e);
        }
    };
    let mut dst: Vec<JsValue> = Vec::with_capacity(n);
    let mut merged_bounds: Vec<usize> = Vec::with_capacity(bounds.len() / 2 + 2);
    while bounds.len() > 2 {
        dst.clear();
        merged_bounds.clear();
        if let Err(e) = merge_run_pairs(&src, &mut dst, &bounds, &mut merged_bounds, &mut cmp) {
            *items = src;
            return Err(e);
        }
        std::mem::swap(&mut src, &mut dst);
        std::mem::swap(&mut bounds, &mut merged_bounds);
    }
    *items = src;
    Ok(())
}

// Splits `src` into maximal runs, reversing each strictly descending one, and
// returns the run boundaries (`0 ..= src.len()`). A run is reversed only after
// it is fully scanned, so an `Err` leaves `src` a permutation of its input.
fn find_runs<E>(
    src: &mut [JsValue],
    cmp: &mut impl FnMut(&JsValue, &JsValue) -> Result<std::cmp::Ordering, E>,
) -> Result<Vec<usize>, E> {
    let n = src.len();
    let mut bounds = Vec::with_capacity(n / 2 + 2);
    bounds.push(0);
    let mut lo = 0;
    while lo < n {
        let mut hi = lo + 1;
        if hi < n {
            let descending = cmp(&src[lo], &src[hi])? == std::cmp::Ordering::Greater;
            hi += 1;
            while hi < n
                && (cmp(&src[hi - 1], &src[hi])? == std::cmp::Ordering::Greater) == descending
            {
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

// One pass of `merge_sort_by`: merges each pair of adjacent runs of `src`
// (delimited by `bounds`) into `dst` and records the merged runs' boundaries
// in `merged_bounds`; a trailing odd run is copied through.
fn merge_run_pairs<E>(
    src: &[JsValue],
    dst: &mut Vec<JsValue>,
    bounds: &[usize],
    merged_bounds: &mut Vec<usize>,
    cmp: &mut impl FnMut(&JsValue, &JsValue) -> Result<std::cmp::Ordering, E>,
) -> Result<(), E> {
    merged_bounds.push(0);
    let mut k = 0;
    while k + 1 < bounds.len() {
        let lo = bounds[k];
        if k + 2 >= bounds.len() {
            let hi = bounds[k + 1];
            dst.extend_from_slice(&src[lo..hi]);
            merged_bounds.push(hi);
            break;
        }
        let (mid, hi) = (bounds[k + 1], bounds[k + 2]);
        merge_runs(src, dst, lo, mid, hi, cmp)?;
        merged_bounds.push(hi);
        k += 2;
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
    cmp: &mut impl FnMut(&JsValue, &JsValue) -> Result<std::cmp::Ordering, E>,
) -> Result<(), E> {
    // Already in order: one comparison instead of a full merge.
    if cmp(&src[mid - 1], &src[mid])? != std::cmp::Ordering::Greater {
        dst.extend_from_slice(&src[lo..hi]);
        return Ok(());
    }
    // With a one-element left run that check was the merge's first
    // comparison, so reuse its result instead of calling cmp again.
    let mut known_first = (mid - lo == 1).then_some(std::cmp::Ordering::Greater);
    let (mut i, mut j) = (lo, mid);
    while i < mid && j < hi {
        let ord = match known_first.take() {
            Some(ord) => ord,
            None => cmp(&src[i], &src[j])?,
        };
        if ord == std::cmp::Ordering::Greater {
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
        let mut out = values(&items);
        out.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut expected: Vec<f64> = (0..200).map(|n| f64::from(n % 17)).collect();
        expected.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(out, expected);
    }

    #[test]
    fn an_error_mid_run_leaves_a_permutation_and_stops_calling() {
        for fail_at in 1..=20 {
            let mut items: Vec<JsValue> = (0..12)
                .rev()
                .map(|n| JsValue::number(f64::from(n)))
                .collect();
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
            assert_eq!(result.is_err(), calls == fail_at);
            if result.is_err() {
                assert_eq!(calls, fail_at);
            }
            let mut out = values(&items);
            out.sort_by(|a, b| a.partial_cmp(b).unwrap());
            assert_eq!(out, (0..12).map(f64::from).collect::<Vec<_>>());
        }
    }
}
