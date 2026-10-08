//! The render watchdog: the panel must not outlive its own failure to draw.
//!
//! `keyboard_mode=NONE` means Esc does nothing, and automatic dismissal is off
//! by default, so ✕ is the panel's only way out (spec §12). That makes "the
//! panel never reached the screen" a state nobody can escape — and it has
//! happened: a probe surface stayed up for 23 minutes because
//! `gtk_main_quit()` does not stop a `GApplication`. So the panel gives itself a
//! few seconds to paint and exits if it never does. The normal path never fires
//! it.

use std::time::{Duration, Instant};

/// How long the panel gives itself to reach the screen.
pub const GRACE: Duration = Duration::from_secs(5);

/// A deadline the first frame cancels.
pub struct RenderWatchdog {
    grace: Duration,
    armed_at: Instant,
    drawn: bool,
}

impl RenderWatchdog {
    /// A watchdog armed now.
    pub fn new(grace: Duration) -> Self {
        Self::armed_at(grace, Instant::now())
    }

    /// The same, armed at a caller's instant: the decision is worth a test that
    /// does not wait five seconds.
    fn armed_at(grace: Duration, now: Instant) -> Self {
        Self {
            grace,
            armed_at: now,
            drawn: false,
        }
    }

    /// The first frame reached the screen. From here the watchdog can never
    /// fire, however long the panel stays up.
    pub fn note_drawn(&mut self) {
        self.drawn = true;
    }

    /// Whether the grace has run out without a frame reaching the screen.
    pub fn expired(&self) -> bool {
        self.expired_at(Instant::now())
    }

    /// The same, against a caller's clock.
    pub fn expired_at(&self, now: Instant) -> bool {
        !self.drawn && now >= self.armed_at + self.grace
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_watchdog_that_never_drew_expires_when_the_grace_runs_out() {
        let start = Instant::now();
        let watchdog = RenderWatchdog::armed_at(GRACE, start);

        assert!(!watchdog.expired_at(start));
        assert!(!watchdog.expired_at(start + GRACE - Duration::from_millis(1)));
        assert!(watchdog.expired_at(start + GRACE));
        assert!(watchdog.expired_at(start + Duration::from_secs(600)));
    }

    /// Normal path: the frame arrives, and the panel may stay up as long as it
    /// likes.
    #[test]
    fn a_watchdog_whose_frame_arrived_never_expires() {
        let start = Instant::now();
        let mut watchdog = RenderWatchdog::armed_at(GRACE, start);

        watchdog.note_drawn();

        assert!(!watchdog.expired_at(start + Duration::from_secs(600)));
    }
}
