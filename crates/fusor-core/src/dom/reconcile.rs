/// The previous position of a row that was just rendered and is not yet in
/// the list. Such a row is never stationary: it always needs an insertion.
pub(super) const NEW: usize = usize::MAX;

/// Mark the rows that can stay where they are while the others move.
///
/// `positions[i]` is the previous index of the row now at `i`, or [`NEW`].
/// Positions are distinct. The result marks one longest strictly increasing
/// run of previous positions: those rows are already in relative order, so
/// each unmarked row needs exactly one move or insertion. When several runs
/// are equally long, which one is marked is unspecified.
pub(super) fn stationary(positions: &[usize]) -> Vec<bool> {
    let existing = |position: &usize| *position != NEW;
    // Rows were only added or removed: every existing row stays, in O(n).
    if positions
        .iter()
        .filter(|position| existing(position))
        .is_sorted_by(|left, right| left < right)
    {
        return positions.iter().map(existing).collect();
    }
    // Patience sorting in O(n log n). `tails[k]` is the row that ends the
    // increasing run of length k + 1 with the smallest last position so far;
    // `predecessor[row]` is the row before it in the best run ending at `row`.
    let mut tails: Vec<usize> = Vec::new();
    let mut predecessor: Vec<Option<usize>> = vec![None; positions.len()];
    for (row, position) in positions.iter().enumerate() {
        if !existing(position) {
            continue;
        }
        let length = tails.partition_point(|&tail| positions[tail] < *position);
        predecessor[row] = length.checked_sub(1).map(|shorter| tails[shorter]);
        if length == tails.len() {
            tails.push(row);
        } else {
            tails[length] = row;
        }
    }
    let mut stationary = vec![false; positions.len()];
    let run = std::iter::successors(tails.last().copied(), |&row| predecessor[row]);
    for row in run {
        stationary[row] = true;
    }
    stationary
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
        if sorted.windows(2).any(|pair| pair[0] == pair[1]) {
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

    /// The length of the longest strictly increasing run of existing rows,
    /// by trying every subsequence.
    fn longest(positions: &[usize]) -> usize {
        (0..1usize << positions.len())
            .filter_map(|mask| {
                let run: Vec<_> = (0..positions.len())
                    .filter(|&row| mask & (1 << row) != 0)
                    .map(|row| positions[row])
                    .collect();
                let valid = run.iter().all(|&position| position != NEW)
                    && run.windows(2).all(|pair| pair[0] < pair[1]);
                valid.then_some(run.len())
            })
            .max()
            .unwrap()
    }

    fn check(positions: &[usize]) {
        let kept: Vec<_> = positions
            .iter()
            .zip(stationary(positions))
            .filter_map(|(&position, keep)| keep.then_some(position))
            .collect();
        assert!(!kept.contains(&NEW), "{positions:?}");
        assert!(
            kept.windows(2).all(|pair| pair[0] < pair[1]),
            "{positions:?}"
        );
        assert_eq!(kept.len(), longest(positions), "{positions:?}");
    }

    fn permutations(values: &mut [usize], offset: usize, visit: &mut impl FnMut(&[usize])) {
        if offset == values.len() {
            return visit(values);
        }
        for index in offset..values.len() {
            values.swap(offset, index);
            permutations(values, offset + 1, visit);
            values.swap(offset, index);
        }
    }

    #[test]
    fn every_small_reorder_moves_the_fewest_rows() {
        for size in 0..=6 {
            permutations(&mut (0..size).collect::<Vec<_>>(), 0, &mut |order| {
                check(order);
                // A new row can arrive anywhere in the list.
                for at in 0..=order.len() {
                    let mut inserted = order.to_vec();
                    inserted.insert(at, NEW);
                    check(&inserted);
                }
            });
        }
    }

    #[test]
    fn insertions_are_never_stationary_and_deletion_gaps_do_not_move_survivors() {
        assert_eq!(stationary(&[0, NEW, 3, 7]), [true, false, true, true]);
        assert_eq!(stationary(&[NEW; 3]), [false; 3]);
    }
}
