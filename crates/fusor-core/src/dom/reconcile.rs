//! The rows in a longest increasing subsequence are already in relative order.
//! New rows use usize::MAX and must be inserted; every other row outside the
//! subsequence needs exactly one move. No DOM inspection is needed per survivor.
pub(super) fn stationary(positions: &[usize]) -> Vec<bool> {
    let mut last = None;
    let ordered = positions
        .iter()
        .copied()
        .filter(|&p| p != usize::MAX)
        .all(|p| {
            let increasing = last.is_none_or(|previous| previous < p);
            last = Some(p);
            increasing
        });
    if ordered {
        return positions.iter().map(|&p| p != usize::MAX).collect();
    }
    let mut keep = vec![false; positions.len()];
    let mut previous = vec![usize::MAX; positions.len()];
    let mut tails: Vec<usize> = Vec::new();
    for (index, &position) in positions.iter().enumerate() {
        if position == usize::MAX {
            continue;
        }
        let slot = tails.partition_point(|&tail| positions[tail] < position);
        if slot > 0 {
            previous[index] = tails[slot - 1];
        }
        if slot == tails.len() {
            tails.push(index);
        } else {
            tails[slot] = index;
        }
    }
    if let Some(&last) = tails.last() {
        let mut index = last;
        while index != usize::MAX {
            keep[index] = true;
            index = previous[index];
        }
    }
    keep
}

/// Borrowed uniqueness validation and a merge cursor for ascending map keys.
/// Input order remains in the caller's original key vector.
pub(super) struct SortedKeys<'a, K> {
    keys: Vec<&'a K>,
    next: usize,
}

impl<'a, K: Ord> SortedKeys<'a, K> {
    pub(super) fn new(keys: &'a [K]) -> Option<Self> {
        let mut sorted: Vec<_> = keys.iter().collect();
        sorted.sort_unstable();
        if sorted.windows(2).any(|pair| pair[0].cmp(pair[1]).is_eq()) {
            return None;
        }
        Some(Self {
            keys: sorted,
            next: 0,
        })
    }

    /// Queries must follow the same ascending order as BTreeMap::retain.
    pub(super) fn contains_next(&mut self, key: &K) -> bool {
        while let Some(candidate) = self.keys.get(self.next) {
            match (*candidate).cmp(key) {
                std::cmp::Ordering::Less => self.next += 1,
                std::cmp::Ordering::Equal => return true,
                std::cmp::Ordering::Greater => return false,
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorted_membership_matches_ordered_set_for_arbitrary_non_hash_keys() {
        use std::collections::{BTreeMap, BTreeSet};
        #[derive(Eq, PartialEq, Debug)]
        struct Key(i16);
        impl Ord for Key {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                other.0.cmp(&self.0)
            }
        }
        impl PartialOrd for Key {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }
        for existing in 0_u32..128 {
            for wanted in 0_u32..128 {
                let keys: Vec<_> = [3, 0, 6, 2, 5, 1, 4]
                    .into_iter()
                    .filter(|i| wanted & (1 << i) != 0)
                    .map(Key)
                    .collect();
                let oracle: BTreeSet<_> = keys.iter().collect();
                let mut membership = SortedKeys::new(&keys).unwrap();
                let mut rows: BTreeMap<_, _> = (0..7)
                    .filter(|i| existing & (1 << i) != 0)
                    .map(|i| (Key(i), ()))
                    .collect();
                let mut removed = Vec::new();
                rows.retain(|key, _| {
                    let keep = membership.contains_next(key);
                    assert_eq!(keep, oracle.contains(key));
                    if !keep {
                        removed.push(key.0);
                    }
                    keep
                });
                assert!(removed.windows(2).all(|pair| pair[0] > pair[1]));
            }
        }
        assert!(SortedKeys::new(&[Key(5), Key(1), Key(5)]).is_none());
    }

    fn check(values: &mut [usize], offset: usize) {
        if offset < values.len() {
            for index in offset..values.len() {
                values.swap(offset, index);
                check(values, offset + 1);
                values.swap(offset, index);
            }
            return;
        }
        let keep = stationary(values);
        let retained: Vec<_> = values
            .iter()
            .zip(&keep)
            .filter_map(|(v, k)| k.then_some(*v))
            .collect();
        assert!(retained.windows(2).all(|pair| pair[0] < pair[1]));
        // Independent exhaustive oracle, including every possible subsequence.
        let best = (0..1usize << values.len())
            .filter_map(|mask| {
                let sequence: Vec<_> = values
                    .iter()
                    .enumerate()
                    .filter_map(|(i, v)| (mask & (1 << i) != 0).then_some(*v))
                    .collect();
                sequence
                    .windows(2)
                    .all(|pair| pair[0] < pair[1])
                    .then_some(sequence.len())
            })
            .max()
            .unwrap();
        assert_eq!(retained.len(), best, "{values:?}");
    }

    #[test]
    fn all_small_permutations_have_the_minimum_move_count() {
        for size in 0..=7 {
            check(&mut (0..size).collect::<Vec<_>>(), 0);
        }
    }

    #[test]
    fn insertions_are_never_stationary_and_deletion_gaps_do_not_move_survivors() {
        assert_eq!(
            stationary(&[0, usize::MAX, 3, 7]),
            [true, false, true, true]
        );
        assert_eq!(stationary(&[usize::MAX; 3]), [false; 3]);
    }
}
