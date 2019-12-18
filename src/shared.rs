use crate::loom::{
    cell::CausalCell,
    sync::{
        atomic::{spin_loop_hint, AtomicUsize, Ordering},
        Condvar, Mutex, MutexGuard,
    },
};
use slab::Slab;
use std::collections::HashMap;
use std::hash::Hash;

const LOCKED: usize = 1;
const READER_ONE: usize = 1 << 2;

#[derive(Debug)]
pub(crate) struct Shared<K: Hash + Eq, V> {
    shards: CausalCell<Slab<CausalCell<HashMap<K, V>>>>,
    writer: Mutex<usize>,
    cv: Condvar,
}

pub(crate) struct ReadGuard<'a, K: Hash + Eq, V> {
    shards: &'a CausalCell<Slab<CausalCell<HashMap<K, V>>>>,
    writer: &'a Mutex<usize>,
    cv: &'a Condvar,
}

#[derive(Debug)]
pub(crate) struct WriteGuard<'a, K: Hash + Eq, V> {
    cv: &'a Condvar,
    writer: &'a Mutex<usize>,
    shards: &'a CausalCell<Slab<CausalCell<HashMap<K, V>>>>,
}

impl<K: Hash + Eq, V> Shared<K, V> {
    pub(crate) fn new() -> Self {
        Self {
            shards: CausalCell::new(Slab::new()),
            cv: Condvar::new(),
            writer: Mutex::new(0),
        }
    }

    pub(crate) fn read(&self) -> ReadGuard<'_, K, V> {
        let mut locked = self.writer.lock().unwrap();
        loop {
            if let Some(read) = self.try_read2(&mut locked) {
                return read;
            }
            locked = self.cv.wait(locked).unwrap();
        }
    }

    pub(crate) fn write(&self) -> WriteGuard<'_, K, V> {
        let mut locked = self.writer.lock().unwrap();
        while *locked > 0 {
            locked = self.cv.wait(locked).unwrap();
        }
        *locked = LOCKED;
        WriteGuard {
            shards: &self.shards,
            cv: &self.cv,
            writer: &self.writer,
        }
    }

    pub(crate) fn try_read(&self) -> Option<ReadGuard<'_, K, V>> {
        self.try_read2(&mut self.writer.lock().unwrap())
    }

    fn try_read2(&self, guard: &mut MutexGuard<'_, usize>) -> Option<ReadGuard<'_, K, V>> {
        if (**guard) & LOCKED == 0 {
            (**guard) += READER_ONE;
            return Some(ReadGuard {
                writer: &self.writer,
                shards: &self.shards,
                cv: &self.cv,
            });
        }
        None
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
        *self.writer.lock().unwrap() -= READER_ONE;
        self.cv.notify_one();
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
        *self.writer.lock().unwrap() = 0;
        self.cv.notify_all();
    }
}
