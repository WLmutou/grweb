use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

pub struct ConnectionPool {
    active: AtomicUsize,
    total: AtomicU64,
    rejected: AtomicU64,
    max_connections: usize,
}

#[derive(Debug, Clone)]
pub struct PoolStats {
    pub active_connections: usize,
    pub total_connections: u64,
    pub rejected_connections: u64,
    pub max_connections: usize,
}

impl ConnectionPool {
    pub fn new(max_connections: usize) -> Self {
        Self {
            active: AtomicUsize::new(0),
            total: AtomicU64::new(0),
            rejected: AtomicU64::new(0),
            max_connections,
        }
    }

    pub fn try_acquire(&self) -> bool {
        if self.max_connections == 0 {
            self.active.fetch_add(1, Ordering::SeqCst);
            self.total.fetch_add(1, Ordering::SeqCst);
            return true;
        }

        loop {
            let current = self.active.load(Ordering::SeqCst);
            if current >= self.max_connections {
                self.rejected.fetch_add(1, Ordering::SeqCst);
                return false;
            }
            if self
                .active
                .compare_exchange(current, current + 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                self.total.fetch_add(1, Ordering::SeqCst);
                return true;
            }
        }
    }

    pub fn release(&self) {
        self.active.fetch_sub(1, Ordering::SeqCst);
    }

    pub fn stats(&self) -> PoolStats {
        PoolStats {
            active_connections: self.active.load(Ordering::SeqCst),
            total_connections: self.total.load(Ordering::SeqCst),
            rejected_connections: self.rejected.load(Ordering::SeqCst),
            max_connections: self.max_connections,
        }
    }
}

pub type SharedPool = Arc<ConnectionPool>;