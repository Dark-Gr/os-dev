/// Since Rust doesn't allow `impl` in structs that doesn't belong to the current crate
/// we create a "shadow" of the [`spin::Mutex`] so we can use `impl` freely
pub struct Mutex<T> {
    inner: spin::Mutex<T>
}

impl<T> Mutex<T> {
    pub const fn new(data: T) -> Self {
        Mutex {
            inner: spin::Mutex::new(data)
        }
    }

    pub fn lock(&self) -> spin::MutexGuard<T> {
        self.inner.lock()
    }
}