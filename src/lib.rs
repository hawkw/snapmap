pub(crate) mod loom;
pub mod reader;
pub(crate) mod shared;
#[doc(inline)]
pub use self::reader::Reader;
use self::shared::Shared;
use crate::loom::{cell::CausalCell, sync::Arc};
use std::{borrow::Borrow, collections::HashMap, hash::Hash};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub struct SnapMap<K: Hash + Eq, V> {
    shared: Arc<Shared<K, V>>,
}

#[derive(Debug)]
pub struct Writer<K: Hash + Eq, V> {
    shared: Arc<Shared<K, V>>,
    idx: usize,
    q: HashMap<K, V>,
}

impl<K: Hash + Eq, V> SnapMap<K, V> {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared::new()),
        }
    }

    pub fn snapshot(&self) -> Reader<'_, K, V> {
        let shared = self.shared.write();
        Reader { shared }
    }

    pub fn writer(&self) -> Writer<K, V> {
        let idx = self
            .shared
            .write()
            .with_mut(|slab| slab.insert(CausalCell::new(HashMap::new())));
        Writer {
            shared: self.shared.clone(),
            idx,
            q: HashMap::new(),
        }
    }
}

impl<K: Hash + Eq, V> Writer<K, V> {
    #[inline(always)]
    fn with_map<T>(&mut self, f: impl FnOnce(&mut HashMap<K, V>) -> T) -> T {
        let idx = self.idx;
        let q = &mut self.q;
        match self.shared.try_read() {
            Some(lock) => lock.with(|slab| {
                slab[idx].with_mut(|map| {
                    let map = unsafe { &mut *map };
                    Self::do_sync(q, map);
                    f(map)
                })
            }),
            None => f(q),
        }
    }

    pub fn sync(&mut self) {
        let idx = self.idx;
        let q = &mut self.q;
        self.shared
            .read()
            .with(|slab| slab[idx].with_mut(|map| unsafe { Self::do_sync(q, &mut *map) }))
    }

    fn do_sync(q: &mut HashMap<K, V>, map: &mut HashMap<K, V>) {
        map.reserve(q.len());
        map.extend(q.drain());
    }

    pub fn insert(&mut self, key: K, val: V) -> Option<V> {
        self.with_map(|map| map.insert(key, val))
    }

    pub fn remove<Q>(&mut self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        self.with_map(|map| map.remove(key))
    }

    pub fn with_mut<Q, T>(&mut self, key: &Q, f: impl FnOnce(&mut V) -> T) -> Option<T>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        let idx = self.idx;
        let shared = &self.shared;
        let q = &mut self.q;
        let lock = shared.try_read();
        let lock = if let Some(lock) = lock {
            lock
        } else {
            if let Some(q) = q.get_mut(key) {
                return Some(f(q));
            }
            shared.read()
        };
        lock.with(|slab| {
            slab[idx].with_mut(|map| {
                let map = unsafe { &mut *map };
                Self::do_sync(q, map);
                map.get_mut(key).map(f)
            })
        })
    }
}

impl<K: Hash + Eq, V> Drop for Writer<K, V> {
    fn drop(&mut self) {
        self.shared.write().with_mut(|slab| {
            slab.remove(self.idx);
        })
    }
}
