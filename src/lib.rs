//!
//! ## What
//!
//! `SnapMap` is a _multi-producer, single concurrent consumer_
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

macro_rules! snap_dbg {
    () => {
        snap_println!("")
    };
    ($val:expr) => {
        match $val {
            tmp => {
                snap_println!("{} = {:#?}", stringify!($val), &tmp);
                tmp
            }
        }
    };
    // Trailing comma with single argument is ignored
    ($val:expr,) => { $crate::snap_dbg!($val) };
    ($($val:expr),+ $(,)?) => {
        ($($crate::snap_dbg!($val)),+,)
    };
}

macro_rules! snap_println {
    ($($arg:tt)*) => {
        if cfg!(test) || cfg!(snapmap_debug) {
            eprintln!("[{}:{}] {} {}",
                file!(),
                line!(),
                std::thread::current().name().unwrap_or("???"),
                format_args!($($arg)*)
            );
        }
    };
}

#[derive(Debug, Clone)]
pub struct SnapMap<K: Hash + Eq, V> {
    shared: Arc<RwLock<Shared<K, V>>>,
}

unsafe impl<K: Hash + Eq, V> Send for SnapMap<K, V> {}
unsafe impl<K: Hash + Eq, V> Sync for SnapMap<K, V> {}

#[derive(Debug)]
pub struct Writer<K: Hash + Eq, V> {
    shared: Arc<RwLock<Shared<K, V>>>,
    idx: usize,
    q: HashMap<K, Option<V>>,
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

impl<K: Hash + Eq + std::fmt::Debug, V: std::fmt::Debug> Writer<K, V> {
    #[inline(always)]
    fn with_map<T>(
        &mut self,
        f: impl FnOnce(&mut HashMap<K, V>) -> T,
        or: impl FnOnce(&mut HashMap<K, Option<V>>) -> T,
    ) -> T {
        let q = &mut self.q;
        match self.shared.try_read() {
            Ok(slab) => slab[self.idx].with_mut(|map| {
                let map = unsafe { &mut *map };
                Self::do_sync(q, map);
                f(map)
            }),
            Err(TryLockError::WouldBlock) => or(q),
            Err(_) => panic!("lock poisoned"),
        }
    }

    pub fn sync(&mut self) {
        let q = &mut self.q;
        let slab = self.shared.read().unwrap();
        slab[self.idx].with_mut(|map| unsafe { Self::do_sync(q, &mut *map) });
    }

    fn do_sync(q: &mut HashMap<K, Option<V>>, map: &mut HashMap<K, V>) {
        map.reserve(q.len());
        for (k, v) in q.drain() {
            let k = snap_dbg!(k);
            if let Some(v) = snap_dbg!(v) {
                snap_dbg!(map.insert(k, v));
            } else {
                snap_dbg!(map.remove(&k));
            }
        }
    }

    pub fn insert(&mut self, key: K, val: V) -> Option<V> {
        let q = &mut self.q;
        match self.shared.try_read() {
            Ok(slab) => slab[self.idx].with_mut(|map| {
                let map = unsafe { &mut *map };
                Self::do_sync(q, map);
                snap_dbg!(map.insert(key, val))
            }),
            Err(TryLockError::WouldBlock) => snap_dbg!(q.insert(key, Some(val)))?,
            Err(_) => panic!("lock poisoned"),
        }
    }

    pub fn remove(&mut self, key: K) -> Option<V> {
        let q = &mut self.q;
        match self.shared.try_read() {
            Ok(slab) => slab[self.idx].with_mut(|map| {
                let map = unsafe { &mut *map };
                Self::do_sync(q, map);
                snap_dbg!(map.remove(&key))
            }),
            Err(TryLockError::WouldBlock) => snap_dbg!(q.insert(key, None))?,
            Err(_) => panic!("lock poisoned"),
        }
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
                    return Some(f(q.as_mut()?));
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
