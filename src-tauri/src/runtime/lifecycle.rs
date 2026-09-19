use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CloseDecision {
    Hide,
    Allow,
}

#[derive(Debug)]
pub(crate) struct Lifecycle {
    quit_requested: AtomicBool,
    tray_available: AtomicBool,
}

impl Default for Lifecycle {
    fn default() -> Self {
        Self {
            quit_requested: AtomicBool::new(false),
            tray_available: AtomicBool::new(true),
        }
    }
}

impl Lifecycle {
    pub(crate) fn request_quit(&self) {
        self.quit_requested.store(true, Ordering::Release);
    }

    pub(crate) fn on_close_requested(&self) -> CloseDecision {
        if self.quit_requested.load(Ordering::Acquire)
            || !self.tray_available.load(Ordering::Acquire)
        {
            CloseDecision::Allow
        } else {
            CloseDecision::Hide
        }
    }

    pub(crate) fn set_tray_available(&self, available: bool) {
        self.tray_available.store(available, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::{CloseDecision, Lifecycle};

    #[test]
    fn close_hides_until_quit_is_requested() {
        let lifecycle = Lifecycle::default();
        assert_eq!(lifecycle.on_close_requested(), CloseDecision::Hide);

        lifecycle.request_quit();

        assert_eq!(lifecycle.on_close_requested(), CloseDecision::Allow);
    }

    #[test]
    fn quit_request_is_idempotent() {
        let lifecycle = Lifecycle::default();

        lifecycle.request_quit();
        lifecycle.request_quit();

        assert_eq!(lifecycle.on_close_requested(), CloseDecision::Allow);
    }

    #[test]
    fn close_is_allowed_when_tray_is_unavailable() {
        let lifecycle = Lifecycle::default();

        lifecycle.set_tray_available(false);

        assert_eq!(lifecycle.on_close_requested(), CloseDecision::Allow);
    }
}
