pub(crate) mod loom;
pub mod reader;
#[doc(inline)]
pub use self::reader::Reader;
use crate::loom::{
    cell::CausalCell,
    sync::{Arc, RwLock, TryLockError},
};
use slab::Slab;
use std::{borrow::Borrow, collections::HashMap, hash::Hash};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub struct SnapMap<K: Hash + Eq, V> {
    shared: Arc<RwLock<Shared<K, V>>>,
}

#[derive(Debug)]
pub struct Writer<K: Hash + Eq, V> {
    shared: Arc<RwLock<Shared<K, V>>>,
    idx: usize,
    q: HashMap<K, V>,
}

pub(crate) type Shared<K, V> = Slab<CausalCell<HashMap<K, V>>>;

impl<K: Hash + Eq, V> SnapMap<K, V> {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(RwLock::new(Shared::new())),
        }
    }

    pub fn snapshot(&self) -> Reader<'_, K, V> {
        let shared = self.shared.write().unwrap();
        Reader { shared }
    }

    pub fn writer(&self) -> Writer<K, V> {
        let idx = {
            let mut slab = self.shared.write().unwrap();
            slab.insert(CausalCell::new(HashMap::new()))
        };
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
        let q = &mut self.q;
        match self.shared.try_read() {
            Ok(slab) => slab[self.idx].with_mut(|map| {
                let map = unsafe { &mut *map };
                Self::do_sync(q, map);
                f(map)
            }),
            Err(TryLockError::WouldBlock) => f(q),
            Err(_) => panic!("lock poisoned"),
        }
    }

    pub fn sync(&mut self) {
        let q = &mut self.q;
        let slab = self.shared.read().unwrap();
        slab[self.idx].with_mut(|map| unsafe { Self::do_sync(q, &mut *map) });
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
        let q = &mut self.q;
        let slab = match self.shared.try_read() {
            Ok(lock) => lock,
            Err(TryLockError::WouldBlock) => {
                if let Some(q) = q.get_mut(key) {
                    return Some(f(q));
                }
                self.shared.read().unwrap()
            }
            Err(_) => panic!("lock poisoned!"),
        };
        slab[self.idx].with_mut(|map| {
            let map = unsafe { &mut *map };
            Self::do_sync(q, map);
            map.get_mut(key).map(f)
        })
    }
}

impl<K: Hash + Eq, V> Drop for Writer<K, V> {
    fn drop(&mut self) {
        if let Ok(mut slab) = self.shared.write() {
            slab.remove(self.idx);
        }
    }
}
