//! A small ordered map kept as one sorted vector.

/// An ordered map for a handful of entries, kept as a vector sorted by key. It offers the part of `BTreeMap` the
/// engine uses, iterates and prints the same way, and copies with one allocation.
#[derive(Clone, PartialEq)]
pub struct FlatMap<K, V>(Vec<(K, V)>);

impl<K, V> Default for FlatMap<K, V> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

/// A place in a [`FlatMap`] for one key, filled or not (see [`FlatMap::entry`]).
pub struct FlatEntry<'a, K, V> {
    map: &'a mut FlatMap<K, V>,
    key: K,
    at: Result<usize, usize>,
}

impl<'a, K, V> FlatEntry<'a, K, V> {
    /// The value under the key, first set to `make()` when there is none.
    pub fn or_insert_with(self, make: impl FnOnce() -> V) -> &'a mut V {
        let i = match self.at {
            Ok(i) => i,
            Err(i) => {
                self.map.0.insert(i, (self.key, make()));
                i
            }
        };
        &mut self.map.0[i].1
    }

    pub fn or_insert(self, value: V) -> &'a mut V {
        self.or_insert_with(|| value)
    }

    pub fn or_default(self) -> &'a mut V
    where
        V: Default,
    {
        self.or_insert_with(V::default)
    }
}

impl<K: Ord, V> FlatMap<K, V> {
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    fn find(&self, key: &K) -> Result<usize, usize> {
        self.0.binary_search_by(|e| e.0.cmp(key))
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.find(key).ok().map(|i| &self.0[i].1)
    }

    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        self.find(key).ok().map(|i| &mut self.0[i].1)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.find(key).is_ok()
    }

    pub fn entry(&mut self, key: K) -> FlatEntry<'_, K, V> {
        let at = self.find(&key);
        FlatEntry { map: self, key, at }
    }

    /// Set the value under `key`; the value it replaces, if any.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        match self.find(&key) {
            Ok(i) => Some(std::mem::replace(&mut self.0[i].1, value)),
            Err(i) => {
                self.0.insert(i, (key, value));
                None
            }
        }
    }

    pub fn remove(&mut self, key: &K) -> Option<V> {
        self.find(key).ok().map(|i| self.0.remove(i).1)
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// The entries in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.0.iter().map(|e| (&e.0, &e.1))
    }

    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.0.iter().map(|e| &e.0)
    }

    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.0.iter().map(|e| &e.1)
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.0.iter_mut().map(|e| &mut e.1)
    }
}

impl<K: std::fmt::Debug, V: std::fmt::Debug> std::fmt::Debug for FlatMap<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map().entries(self.0.iter().map(|e| (&e.0, &e.1))).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::FlatMap;
    use std::collections::BTreeMap;

    #[test]
    fn it_behaves_and_prints_like_a_btreemap() {
        let mut flat: FlatMap<(i32, i32), f32> = FlatMap::new();
        let mut tree: BTreeMap<(i32, i32), f32> = BTreeMap::new();
        let mut x = 12345u32;
        for step in 0..2000 {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let key = ((x >> 8) as i32 % 7, (x >> 16) as i32 % 5);
            let value = (x >> 4) as f32;
            match step % 5 {
                0 => assert_eq!(flat.insert(key, value), tree.insert(key, value)),
                1 => {
                    *flat.entry(key).or_insert(0.0) += value;
                    *tree.entry(key).or_insert(0.0) += value;
                }
                2 => assert_eq!(flat.remove(&key), tree.remove(&key)),
                3 => assert_eq!(flat.get(&key), tree.get(&key)),
                _ => {
                    *flat.entry(key).or_default() -= 1.0;
                    *tree.entry(key).or_default() -= 1.0;
                }
            }
            assert_eq!(flat.len(), tree.len());
            assert_eq!(flat.contains_key(&key), tree.contains_key(&key));
        }
        assert!(flat.iter().eq(tree.iter()));
        assert_eq!(format!("{flat:?}"), format!("{tree:?}"));
        assert_eq!(format!("{flat:#?}"), format!("{tree:#?}"));
        flat.clear();
        assert!(flat.is_empty());
        assert_eq!(format!("{flat:?}"), "{}");
    }
}
