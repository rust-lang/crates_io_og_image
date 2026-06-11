//! Management of the process-global Typst memoization cache.

/// Evicts the process-global Typst memoization cache when dropped.
pub struct EvictGuard {
    pub max_age: usize,
}

impl Drop for EvictGuard {
    fn drop(&mut self) {
        ::typst::comemo::evict(self.max_age);
    }
}
