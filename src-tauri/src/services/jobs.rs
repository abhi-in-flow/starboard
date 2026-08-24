use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::error::{AppError, AppResult};

/// Cooperative cancellation flag shared by long-running jobs.
#[derive(Debug, Clone)]
pub struct CancelFlag {
    cancelled: Arc<AtomicBool>,
}

impl Default for CancelFlag {
    fn default() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl CancelFlag {
    pub fn request(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn reset(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
    }

    pub fn check(&self) -> AppResult<()> {
        if self.is_cancelled() {
            Err(AppError::cancelled("operation cancelled"))
        } else {
            Ok(())
        }
    }
}

/// Clears an in-memory running flag on drop, including panic/unwind.
pub struct RunningFlagGuard<'a> {
    flag: &'a AtomicBool,
}

impl<'a> RunningFlagGuard<'a> {
    /// Caller must have already set `flag` to true (e.g. via compare_exchange).
    pub fn holding(flag: &'a AtomicBool) -> Self {
        Self { flag }
    }
}

impl Drop for RunningFlagGuard<'_> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancel_flag_is_deterministic() {
        let flag = CancelFlag::default();
        assert!(!flag.is_cancelled());
        flag.check().expect("not cancelled");
        flag.request();
        assert!(flag.is_cancelled());
        let err = flag.check().expect_err("cancelled");
        assert_eq!(err.code, "cancelled");
        flag.reset();
        assert!(!flag.is_cancelled());
        flag.check().expect("reset");
    }

    #[test]
    fn running_guard_clears_flag_on_drop() {
        let flag = AtomicBool::new(true);
        {
            let _guard = RunningFlagGuard::holding(&flag);
            assert!(flag.load(Ordering::SeqCst));
        }
        assert!(!flag.load(Ordering::SeqCst));
    }

    #[test]
    fn running_guard_clears_flag_on_unwind() {
        let flag = AtomicBool::new(true);
        let panicked = std::panic::catch_unwind(|| {
            let _guard = RunningFlagGuard::holding(&flag);
            panic!("boom");
        });
        assert!(panicked.is_err());
        assert!(
            !flag.load(Ordering::SeqCst),
            "running flag must reset after unwind"
        );
    }
}
