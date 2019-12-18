use crate::loom::{
    cell::CausalCell,
    sync::atomic::{spin_loop_hint, AtomicUsize, Ordering},
};
use slab::Slab;
use std::collections::HashMap;
use std::hash::Hash;

const LOCKED: usize = 1;
const READER_ONE: usize = 1 << 2;

#[derive(Debug)]
pub(crate) struct Shared<K: Hash + Eq, V> {
    shards: CausalCell<Slab<CausalCell<HashMap<K, V>>>>,
    lock: AtomicUsize,
}

pub(crate) struct ReadGuard<'a, K: Hash + Eq, V> {
    lock: &'a AtomicUsize,
    shards: &'a CausalCell<Slab<CausalCell<HashMap<K, V>>>>,
}

#[derive(Debug)]
pub(crate) struct WriteGuard<'a, K: Hash + Eq, V> {
    lock: &'a AtomicUsize,
    shards: &'a CausalCell<Slab<CausalCell<HashMap<K, V>>>>,
}

impl<K: Hash + Eq, V> Shared<K, V> {
    pub(crate) fn new() -> Self {
        Self {
            shards: CausalCell::new(Slab::new()),
            lock: AtomicUsize::new(0),
        }
    }

    pub(crate) fn read(&self) -> ReadGuard<'_, K, V> {
        loop {
            if let Some(read) = self.try_read() {
                return read;
            }

            spin_loop_hint();
        }
    }

    pub(crate) fn write(&self) -> WriteGuard<'_, K, V> {
        loop {
            match self
                .lock
                .compare_exchange_weak(0, LOCKED, Ordering::Acquire, Ordering::Acquire)
            {
                Ok(_) => {
                    println!("ACQUIRE WRITE GUARD");
                    return WriteGuard {
                        lock: &self.lock,
                        shards: &self.shards,
                    };
                }
                Err(actual) => println!("actual = {:?}", actual),
            }
            spin_loop_hint();
        }
    }

    pub(crate) fn try_read(&self) -> Option<ReadGuard<'_, K, V>> {
        let mut val = self.lock.load(Ordering::Relaxed);
        loop {
            if val & LOCKED != 0 {
                return None;
            }

            match self.lock.compare_exchange_weak(
                val,
                val + READER_ONE,
                Ordering::Acquire,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    return Some(ReadGuard {
                        lock: &self.lock,
                        shards: &self.shards,
                    })
                }
                Err(actual) => {
                    val = actual;
                }
            }
        }
    }
}

impl<'a, K: Hash + Eq, V> ReadGuard<'a, K, V> {
    #[inline]
    pub(crate) fn with<T>(&self, f: impl FnOnce(&Slab<CausalCell<HashMap<K, V>>>) -> T) -> T {
        self.shards.with(|c| unsafe { f(&*c) })
    }
}

impl<'a, K: Hash + Eq, V> Drop for ReadGuard<'a, K, V> {
    fn drop(&mut self) {
        self.lock.fetch_sub(READER_ONE, Ordering::Release);
    }
}

impl<'a, K: Hash + Eq, V> WriteGuard<'a, K, V> {
    #[inline]
    pub(crate) fn with_mut<T>(
        &mut self,
        f: impl FnOnce(&mut Slab<CausalCell<HashMap<K, V>>>) -> T,
    ) -> T {
        self.shards.with_mut(|c| unsafe { f(&mut *c) })
    }

    #[inline]
    pub(crate) fn to_ref(&self) -> &Slab<CausalCell<HashMap<K, V>>> {
        self.shards.with_mut(|c| unsafe { &*c })
    }
}

impl<'a, K: Hash + Eq, V> Drop for WriteGuard<'a, K, V> {
    fn drop(&mut self) {
        self.lock.store(0, Ordering::Release);
        println!("DROP WRITE GUARD");
    }
}
