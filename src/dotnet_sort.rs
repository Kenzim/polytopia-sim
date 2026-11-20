//! `List<T>.Sort(Comparison<T>)` as .NET's `ArraySortHelper<T>` runs it (introspective sort:
//! insertion sort for partitions of 16 or fewer, median-of-three quicksort, heapsort fallback).
//! Unstable, so tie order matches the game only when this exact algorithm is used.

use std::cmp::Ordering;

pub fn sort_by<T, F: FnMut(&T, &T) -> Ordering>(keys: &mut [T], mut cmp: F) {
    let n = keys.len();
    if n < 2 {
        return;
    }
    let depth = 2 * (usize::BITS - 1 - n.leading_zeros() + 1) as i32;
    intro_sort(keys, 0, n - 1, depth, &mut cmp);
}

fn gt<T, F: FnMut(&T, &T) -> Ordering>(k: &[T], a: usize, b: usize, c: &mut F) -> bool {
    c(&k[a], &k[b]) == Ordering::Greater
}

fn swap_if_greater<T, F: FnMut(&T, &T) -> Ordering>(k: &mut [T], a: usize, b: usize, c: &mut F) {
    if a != b && gt(k, a, b, c) {
        k.swap(a, b);
    }
}

fn intro_sort<T, F: FnMut(&T, &T) -> Ordering>(k: &mut [T], lo: usize, mut hi: usize, mut depth: i32, c: &mut F) {
    while hi > lo {
        let size = hi - lo + 1;
        if size <= 16 {
            if size == 2 {
                swap_if_greater(k, lo, hi, c);
            } else if size == 3 {
                swap_if_greater(k, lo, hi - 1, c);
                swap_if_greater(k, lo, hi, c);
                swap_if_greater(k, hi - 1, hi, c);
            } else {
                insertion_sort(k, lo, hi, c);
            }
            return;
        }
        if depth == 0 {
            heap_sort(k, lo, hi, c);
            return;
        }
        depth -= 1;
        let p = pick_pivot_and_partition(k, lo, hi, c);
        intro_sort(k, p + 1, hi, depth, c);
        if p == 0 {
            return;
        }
        hi = p - 1;
    }
}

fn pick_pivot_and_partition<T, F: FnMut(&T, &T) -> Ordering>(k: &mut [T], lo: usize, hi: usize, c: &mut F) -> usize {
    let mid = lo + ((hi - lo) >> 1);
    swap_if_greater(k, lo, mid, c);
    swap_if_greater(k, lo, hi, c);
    swap_if_greater(k, mid, hi, c);
    k.swap(mid, hi - 1);
    let pivot = hi - 1; // pivot value now lives at hi-1 and never moves during the scan
    let (mut left, mut right) = (lo, hi - 1);
    while left < right {
        loop {
            left += 1;
            if c(&k[left], &k[pivot]) != Ordering::Less {
                break;
            }
        }
        loop {
            right -= 1;
            if c(&k[pivot], &k[right]) != Ordering::Less {
                break;
            }
        }
        if left >= right {
            break;
        }
        k.swap(left, right);
    }
    if left != hi - 1 {
        k.swap(left, hi - 1);
    }
    left
}

fn insertion_sort<T, F: FnMut(&T, &T) -> Ordering>(k: &mut [T], lo: usize, hi: usize, c: &mut F) {
    for i in lo..hi {
        let mut j = i;
        while c(&k[j + 1], &k[j]) == Ordering::Less {
            k.swap(j + 1, j);
            if j == lo {
                break;
            }
            j -= 1;
        }
    }
}

fn heap_sort<T, F: FnMut(&T, &T) -> Ordering>(k: &mut [T], lo: usize, hi: usize, c: &mut F) {
    let n = hi - lo + 1;
    let mut i = n / 2;
    while i >= 1 {
        down_heap(k, i, n, lo, c);
        i -= 1;
    }
    let mut i = n;
    while i > 1 {
        k.swap(lo, lo + i - 1);
        down_heap(k, 1, i - 1, lo, c);
        i -= 1;
    }
}

fn down_heap<T, F: FnMut(&T, &T) -> Ordering>(k: &mut [T], mut i: usize, n: usize, lo: usize, c: &mut F) {
    // Moves the held element down by swaps; equivalent to the hole-based original.
    while i <= n / 2 {
        let mut child = 2 * i;
        if child < n && c(&k[lo + child - 1], &k[lo + child]) == Ordering::Less {
            child += 1;
        }
        if c(&k[lo + i - 1], &k[lo + child - 1]) != Ordering::Less {
            break;
        }
        k.swap(lo + i - 1, lo + child - 1);
        i = child;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_all_sizes_including_heap_fallback() {
        let mut seed = 12345u32;
        for n in 0..200usize {
            let mut v: Vec<u32> = (0..n)
                .map(|_| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    seed >> 20
                })
                .collect();
            let mut want = v.clone();
            want.sort();
            sort_by(&mut v, |a, b| a.cmp(b));
            assert_eq!(v, want, "n={n}");
        }
        let mut v: Vec<i32> = (0..500).rev().collect();
        sort_by(&mut v, |a, b| a.cmp(b));
        assert!(v.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn small_ties_keep_insertion_order() {
        let mut v = vec![(1, 'a'), (0, 'b'), (1, 'c'), (0, 'd')];
        sort_by(&mut v, |a, b| a.0.cmp(&b.0));
        assert_eq!(v, vec![(0, 'b'), (0, 'd'), (1, 'a'), (1, 'c')]);
    }
}
