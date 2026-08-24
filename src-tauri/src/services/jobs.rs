use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::{AppError, AppResult};

/// Cooperative cancellation flag shared by long-running jobs.
#[derive(Debug, Default)]
pub struct CancelFlag {
    cancelled: AtomicBool,
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
}
