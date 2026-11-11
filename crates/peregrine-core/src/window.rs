//! Hysteresis-filtered sliding-sync windowing (catalog-1 D-track, D2#13).
//!
//! Fast scrolling must not translate into a storm of overlapping range
//! subscriptions. This kernel owns the decision: a range shift hits the
//! network only when the viewport has *settled* (smoothed velocity below
//! threshold) or when the anchor has crossed the window's outer margin
//! (velocity-independent forced shift). The target position is smoothed
//! with a second-order-style EMA pair (velocity + position), and shifts
//! smaller than a deadband are suppressed.
//!
//! Pure function of its inputs: the DST harness replays scroll traces and
//! requires byte-identical action vectors. This module is the executable
//! oracle for the future Lean proof of the windowing policy.

/// Tuning for one windowed list (rooms, or a room's event timeline).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowCfg {
    /// Number of items the homeserver holds open for us.
    pub size: u32,
    /// How far the anchor may travel past the window edge before a
    /// velocity-independent forced shift.
    pub margin: u32,
    /// Smoothed velocity (items/s) below which a settle-shift is allowed.
    pub settle_velocity: f32,
    /// EMA factor for velocity smoothing, in (0, 1]. Higher = twitchier.
    pub ema_alpha: f32,
    /// Shift deadband: ignore recomputed starts closer than this.
    pub min_shift: u32,
}

impl WindowCfg {
    /// Default tuning for a 20-item room list: settle below 3 items/s,
    /// force past a 5-item margin, never shift for less than 4 items.
    #[must_use]
    pub const fn default_room_list() -> Self {
        Self {
            size: 20,
            margin: 5,
            settle_velocity: 3.0,
            ema_alpha: 0.35,
            min_shift: 4,
        }
    }
}

/// The network decision for the current scroll sample.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowAction {
    /// Keep the current range subscribed; no network traffic.
    Stay,
    /// Issue a range update starting at `start`.
    Shift {
        /// The new range start to subscribe.
        start: u32,
    },
}

/// State machine for one sliding-sync window.
#[derive(Debug, Clone)]
pub struct SyncWindow {
    start: u32,
    size: u32,
    pos_ema: f32,
    vel_ema: f32,
    last_pos: f32,
    last_ts_ms: u64,
    primed: bool,
}

impl SyncWindow {
    /// A window open at `start`, anchored at `anchor_pos`.
    #[must_use]
    pub fn at(start: u32, size: u32, anchor_pos: f32) -> Self {
        Self {
            start,
            size,
            pos_ema: anchor_pos,
            vel_ema: 0.0,
            last_pos: anchor_pos,
            last_ts_ms: 0,
            primed: false,
        }
    }

    /// Current range start (last issued subscription).
    #[must_use]
    pub const fn start(&self) -> u32 {
        self.start
    }

    /// Ingest one scroll sample; returns the network decision.
    ///
    /// # Panics-free design note
    /// Same-timestamp samples clamp `dt` to 5 ms so burst inputs can never
    /// divide by zero or produce infinite velocity.
    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss
    )]
    // INVARIANT: the truncating cast is the quantization step itself
    // (floor to a whole range start); lossless u32->f32 holds for any
    // realistic list size, and the clamp keeps the virtual clock total.
    pub fn on_scroll(&mut self, pos: f32, ts_ms: u64, cfg: &WindowCfg) -> WindowAction {
        let dt_ms = if self.primed {
            ts_ms.saturating_sub(self.last_ts_ms)
        } else {
            0
        };
        let dt = (dt_ms as f32 / 1000.0).max(0.005);
        let raw_vel = if self.primed {
            (pos - self.last_pos).abs() / dt
        } else {
            0.0
        };
        self.last_pos = pos;
        self.last_ts_ms = ts_ms;
        self.primed = true;

        let a = cfg.ema_alpha;
        self.vel_ema = a * raw_vel + (1.0 - a) * self.vel_ema;
        self.pos_ema = a * pos + (1.0 - a) * self.pos_ema;

        let center = self.start as f32 + self.size as f32 / 2.0;
        let dist = (self.pos_ema - center).abs();
        let half = self.size as f32 / 2.0;

        let forced = dist > half + cfg.margin as f32;
        let settled = self.vel_ema <= cfg.settle_velocity && dist > cfg.min_shift as f32;
        if !(forced || settled) {
            return WindowAction::Stay;
        }

        let target = (self.pos_ema - half).max(0.0);
        let new_start = target as u32;
        if u32::abs_diff(new_start, self.start) < cfg.min_shift {
            return WindowAction::Stay;
        }
        self.start = new_start;
        WindowAction::Shift { start: new_start }
    }
}

#[cfg(test)]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)] // tests: synthetic scroll positions are exact small integers
mod tests {
    use super::*;

    fn cfg() -> WindowCfg {
        WindowCfg::default_room_list()
    }

    #[test]
    fn short_fling_within_reach_shifts_only_at_settle() {
        // A fling that stays inside the window's reach (0 -> 25 with the
        // window at 0: max smoothed distance < half + margin) must never
        // shift during motion; the single shift is the settle correction.
        let mut w = SyncWindow::at(0, 20, 0.0);
        let c = cfg();
        let mut fling_shifts = 0;
        for i in 0..60_u64 {
            if let WindowAction::Shift { .. } = w.on_scroll((i as f32) * (25.0 / 60.0), i * 5, &c) {
                fling_shifts += 1;
            }
        }
        assert_eq!(fling_shifts, 0, "in-reach fling: network-silent");
        let mut settle_shifts = 0;
        for i in 0..60_u64 {
            if let WindowAction::Shift { .. } = w.on_scroll(25.0, 300 + i * 10, &c) {
                settle_shifts += 1;
            }
        }
        assert_eq!(settle_shifts, 1, "settle: exactly one corrective shift");
        // First shift lands at floor(24.2 - 10) = 14; pos_ema then converges
        // toward 25 but the residual correction (1 item) is below the
        // deadband, so the window correctly refuses to shift again.
        assert_eq!(
            w.start(),
            14,
            "deadband suppresses the residual 1-item move"
        );
    }

    #[test]
    fn margin_break_is_velocity_independent() {
        let mut w = SyncWindow::at(0, 20, 0.0);
        let c = cfg();
        // Teleport 10_000 items ahead inside one sample. The EMA smooths
        // the anchor to 0.35 * 10_000 = 3_500; the forced shift re-centers
        // on the smoothed anchor (3_500 - half(10) = 3_490), fired despite
        // enormous raw velocity.
        let action = w.on_scroll(10_000.0, 1_000, &c);
        assert!(matches!(action, WindowAction::Shift { start: 3_490 }));
    }

    #[test]
    fn deadband_suppresses_micro_shifts() {
        let mut w = SyncWindow::at(100, 20, 110.0);
        let c = WindowCfg {
            min_shift: 4,
            ..cfg()
        };
        // Drift of 2 items at zero velocity: below deadband, Stay.
        let action = w.on_scroll(112.0, 500, &c);
        assert_eq!(action, WindowAction::Stay);
    }

    #[test]
    fn never_shifts_below_zero() {
        let mut w = SyncWindow::at(0, 20, 0.0);
        let c = cfg();
        let action = w.on_scroll(-50.0, 500, &c);
        match action {
            WindowAction::Shift { start } => assert_eq!(start, 0),
            WindowAction::Stay => {}
        }
    }
}
