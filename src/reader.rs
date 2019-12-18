use crate::{loom::cell::CausalCell, shared::WriteGuard};

use std::{
    borrow::Borrow,
    collections::hash_map::{self, HashMap},
    hash::Hash,
};

#[derive(Debug)]
pub struct Reader<'a, K: Hash + Eq, V> {
    pub(super) shared: WriteGuard<'a, K, V>,
}

pub struct Iter<'a, K, V>(
    std::iter::FlatMap<
        slab::Iter<'a, CausalCell<HashMap<K, V>>>,
        hash_map::Iter<'a, K, V>,
        fn((usize, &'a CausalCell<HashMap<K, V>>)) -> hash_map::Iter<'a, K, V>,
    >,
);

pub struct Keys<'a, K, V>(
    std::iter::FlatMap<
        slab::Iter<'a, CausalCell<HashMap<K, V>>>,
        hash_map::Keys<'a, K, V>,
        fn((usize, &'a CausalCell<HashMap<K, V>>)) -> hash_map::Keys<'a, K, V>,
    >,
);

pub struct Values<'a, K, V>(
    std::iter::FlatMap<
        slab::Iter<'a, CausalCell<HashMap<K, V>>>,
        hash_map::Values<'a, K, V>,
        fn((usize, &'a CausalCell<HashMap<K, V>>)) -> hash_map::Values<'a, K, V>,
    >,
);

impl<'a, K: Hash + Eq, V> Reader<'a, K, V> {
    pub fn get<'b, Q>(&'b self, key: &'b Q) -> impl Iterator<Item = &'b V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
        'b: 'a,
    {
        self.shared
            .to_ref()
            .iter()
            .filter_map(move |(_, map)| map.with(|map| unsafe { (*map).get(&key) }))
    }

    pub fn contains_key<Q>(&self, key: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.shared
            .to_ref()
            .iter()
            .any(|(_, map)| map.with(|map| unsafe { (*map).contains_key(key) }))
    }

    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter(
            self.shared
                .to_ref()
                .iter()
                .flat_map(|(_, map)| map.with(|map| unsafe { (*map).iter() })),
        )
    }

    pub fn keys(&self) -> Keys<'_, K, V> {
        Keys(
            self.shared
                .to_ref()
                .iter()
                .flat_map(|(_, map)| map.with(|map| unsafe { (*map).keys() })),
        )
    }

    pub fn values(&self) -> Values<'_, K, V> {
        Values(
            self.shared
                .to_ref()
                .iter()
                .flat_map(|(_, map)| map.with(|map| unsafe { (*map).values() })),
        )
    }
}

impl<'a, K: Hash + Eq, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl<'a, K: Hash + Eq, V> Iterator for Keys<'a, K, V> {
    type Item = &'a K;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl<'a, K: Hash + Eq, V> Iterator for Values<'a, K, V> {
    type Item = &'a V;
    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next()
    }
}

impl<'a, K: Hash + Eq, V: 'a> IntoIterator for &'a Reader<'a, K, V> {
    type IntoIter = Iter<'a, K, V>;
    type Item = (&'a K, &'a V);
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}
