//! Proactive scheduling — timers and reminders.
//!
//! Two flavours share the same "fire" channel:
//!
//! - **Timer** — ephemeral, in-memory. Dies with the process. Powered by
//!   `tokio::time::sleep` per entry so precision is sub-second.
//! - **Reminder** — persisted to `%APPDATA%\Ren\memory\reminders.json`.
//!   Survives restart. A once-per-minute poll loop checks the store and
//!   fires anything whose deadline has passed.
//!
//! Both push a `Firing` onto `FireSender` when they go off. `lib.rs` bridges
//! that into a spoken notification (via the sentence channel) and a
//! `ren://reminder` Tauri event for the UI.

pub mod reminder;
pub mod timer;

use serde::Serialize;
use tokio::sync::mpsc;

pub use reminder::{
    ReminderCancel, ReminderList, ReminderSet, ReminderStore, SharedReminderStore,
};
pub use timer::{SharedTimerRegistry, TimerCancel, TimerList, TimerRegistry, TimerStart};

/// A pending alert that just went off. Consumed once by `lib.rs` which
/// narrates it to the user.
#[derive(Debug, Clone, Serialize)]
pub struct Firing {
    /// "timer" or "reminder" — kept as a string so the frontend payload
    /// stays stable even if we rename the internal enum later.
    pub kind: &'static str,
    /// Short user-visible label ("tea", "call mom"). Mirrored into the
    /// spoken line and the `ren://reminder` event.
    pub label: String,
}

pub type FireSender = mpsc::Sender<Firing>;
pub type FireReceiver = mpsc::Receiver<Firing>;

/// Builds the fire channel used by both timer and reminder systems. Bounded
/// to 32 — if more than 32 alerts stack up without the consumer draining
/// we have bigger problems than back-pressure.
pub fn fire_channel() -> (FireSender, FireReceiver) {
    mpsc::channel(32)
}
