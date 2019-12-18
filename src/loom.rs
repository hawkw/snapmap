pub(crate) use self::inner::*;

#[cfg(test)]
mod inner {
    pub(crate) use loom::{cell, model, thread};
    pub(crate) mod sync {
        pub(crate) use loom::sync::*;
        pub(crate) use std::sync::TryLockError;
    }
}

#[cfg(not(test))]
mod inner {

    pub(crate) mod sync {
        // pub(crate) use std::sync::atomic::*;
        pub(crate) use std::sync::*;
    }

    pub(crate) mod cell {
        use std::cell::UnsafeCell;
        #[derive(Debug)]
        pub(crate) struct CausalCell<T>(UnsafeCell<T>);

        impl<T> CausalCell<T> {
            pub(crate) fn new(data: T) -> CausalCell<T> {
                CausalCell(UnsafeCell::new(data))
            }

            pub(crate) fn with<F, R>(&self, f: F) -> R
            where
                F: FnOnce(*const T) -> R,
            {
                f(self.0.get())
            }

            pub(crate) fn with_mut<F, R>(&self, f: F) -> R
            where
                F: FnOnce(*mut T) -> R,
            {
                f(self.0.get())
            }
        }
    }
}
