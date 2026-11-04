//! Share the sorting engine for metadata/UI records with different Rust types.
//! Only comparisons and the final permutation depend on the record type.
use std::cmp::Ordering;

pub(crate) fn sort_by<T>(values: &mut [T], mut compare: impl FnMut(&T, &T) -> Ordering) {
    if values.len() < 2 {
        return;
    }
    let order = sorted_indices(values.len(), &mut |a, b| compare(&values[a], &values[b]));
    permute(values, order);
}

pub(crate) fn sort_by_cached_key<T, K: Ord>(values: &mut [T], key: impl FnMut(&T) -> K) {
    if values.len() < 2 {
        return;
    }
    let keys: Vec<_> = values.iter().map(key).collect();
    let order = sorted_indices(values.len(), &mut |a, b| keys[a].cmp(&keys[b]));
    permute(values, order);
}

// One instantiation of the full sort, even under FatLTO. Original indices break
// ties, preserving stable ordering while sorting only machine-word indices.
#[inline(never)]
fn sorted_indices(len: usize, compare: &mut dyn FnMut(usize, usize) -> Ordering) -> Vec<usize> {
    let mut order: Vec<_> = (0..len).collect();
    order.sort_unstable_by(|&a, &b| compare(a, b).then_with(|| a.cmp(&b)));
    order
}

fn permute<T>(values: &mut [T], mut order: Vec<usize>) {
    for start in 0..order.len() {
        let mut current = start;
        loop {
            let next = order[current];
            order[current] = current;
            if next == start {
                break;
            }
            values.swap(current, next);
            current = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    #[test]
    fn matches_stable_sort_for_duplicate_keys_and_permutation_cycles() {
        let mut seed = 0x12345678_u32;
        for len in 0..256 {
            let original: Vec<_> = (0..len)
                .map(|id| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    (seed % 17, id)
                })
                .collect();
            let mut expected = original.clone();
            expected.sort_by_key(|item| item.0);
            let mut actual = original.clone();
            sort_by(&mut actual, |a, b| a.0.cmp(&b.0));
            assert_eq!(actual, expected, "len={len}");
            let mut cached = original;
            let mut calls = 0;
            sort_by_cached_key(&mut cached, |item| {
                calls += 1;
                item.0
            });
            assert_eq!(cached, expected);
            assert!(calls <= len);
        }
    }

    #[test]
    fn non_clone_values_are_dropped_exactly_once_even_if_comparison_panics() {
        struct Item(i32, Rc<Cell<usize>>);
        impl Drop for Item {
            fn drop(&mut self) {
                self.1.set(self.1.get() + 1);
            }
        }
        let drops = Rc::new(Cell::new(0));
        let mut values: Vec<_> = [2, 0, 1, 0]
            .into_iter()
            .map(|n| Item(n, drops.clone()))
            .collect();
        sort_by(&mut values, |a, b| a.0.cmp(&b.0));
        assert_eq!(
            values.iter().map(|x| x.0).collect::<Vec<_>>(),
            vec![0, 0, 1, 2]
        );
        assert_eq!(drops.get(), 0);
        let before: Vec<_> = values.iter().map(|x| x.0).collect();
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sort_by(&mut values, |_, _| panic!("comparison failed"));
        }));
        assert!(panic.is_err());
        assert_eq!(values.iter().map(|x| x.0).collect::<Vec<_>>(), before);
        assert_eq!(drops.get(), 0);
        drop(values);
        assert_eq!(drops.get(), 4);
    }

    #[test]
    fn cached_keys_keep_unicode_tuple_order_and_zero_sized_values_work() {
        let original = vec![
            (false, "猫咪"),
            (true, "B"),
            (false, "Ä"),
            (true, "a"),
            (false, "ä"),
        ];
        let mut expected = original.clone();
        expected.sort_by_cached_key(|(directory, name)| (!directory, name.to_lowercase()));
        let mut actual = original;
        sort_by_cached_key(&mut actual, |(directory, name)| {
            (!directory, name.to_lowercase())
        });
        assert_eq!(actual, expected);
        sort_by(&mut [(); 32], |_, _| Ordering::Equal);
    }
}
