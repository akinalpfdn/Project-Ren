//! Central state machine.
//!
//! `RenStateMachine` is the single authority over Ren's lifecycle state.
//! No other module mutates state directly — every change goes through
//! [`RenStateMachine::transition`] (validated) or [`RenStateMachine::force`]
//! (recovery only).
//!
//! Two side effects fire on every transition:
//! 1. A `ren://state-changed` Tauri event reaches the frontend.
//! 2. A `tokio::sync::broadcast` notifies in-process observers — used by
//!    the conversation idle timer and the model lifecycle task in `lib.rs`.

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast;
use tracing::{info, warn};

/// Capacity of the in-process state broadcast channel. State changes are bursty
/// but rare; a small buffer keeps slow observers from missing edges without
/// pinning memory.
const STATE_BROADCAST_CAPACITY: usize = 32;

/// All possible states in Ren's lifecycle.
/// Mirrors `RenState` in the frontend (`src/types/index.ts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RenState {
    Initializing,
    Sleeping,
    Waking,
    Listening,
    Thinking,
    Speaking,
    Idle,
    Error,
}

impl std::fmt::Display for RenState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            RenState::Initializing => "initializing",
            RenState::Sleeping     => "sleeping",
            RenState::Waking       => "waking",
            RenState::Listening    => "listening",
            RenState::Thinking     => "thinking",
            RenState::Speaking     => "speaking",
            RenState::Idle         => "idle",
            RenState::Error        => "error",
        };
        write!(f, "{}", s)
    }
}

/// Payload sent with `ren://state-changed` events.
#[derive(Clone, Serialize)]
pub struct StateChangedPayload {
    pub state: RenState,
}

/// Payload sent with `ren://error` events.
#[derive(Clone, Serialize)]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
}

/// Central state machine. Single authority over Ren's current state.
pub struct RenStateMachine {
    current: RenState,
    app: AppHandle,
    /// In-process notifier. Subscribers receive every state Ren enters.
    /// Use `subscribe()` to obtain a receiver.
    notifier: broadcast::Sender<RenState>,
}

impl RenStateMachine {
    pub fn new(app: AppHandle) -> Self {
        let (notifier, _) = broadcast::channel(STATE_BROADCAST_CAPACITY);
        Self {
            current: RenState::Initializing,
            app,
            notifier,
        }
    }

    pub fn current(&self) -> RenState {
        self.current
    }

    /// Subscribe to in-process state-change notifications.
    /// Each receiver sees every transition that happens after subscription.
    pub fn subscribe(&self) -> broadcast::Receiver<RenState> {
        self.notifier.subscribe()
    }

    /// Attempt a state transition.
    /// Validates the pair, runs side effects, emits the Tauri event, and
    /// notifies in-process observers.
    pub fn transition(&mut self, to: RenState) -> Result<(), String> {
        if !Self::is_transition_valid(self.current, to) {
            let msg = format!(
                "Invalid state transition: {} → {}",
                self.current, to
            );
            warn!("{}", msg);
            return Err(msg);
        }

        info!("State: {} → {}", self.current, to);
        self.current = to;
        self.broadcast(to);
        Ok(())
    }

    /// Force a transition bypassing validation. Reserved for error recovery
    /// and forced-sleep hotkeys.
    pub fn force(&mut self, to: RenState) {
        info!("State (forced): {} → {}", self.current, to);
        self.current = to;
        self.broadcast(to);
    }

    /// Emit a structured error event and transition to Error state.
    pub fn emit_error(&mut self, code: &str, message: &str) {
        warn!("Error [{}]: {}", code, message);
        let _ = self.app.emit(
            "ren://error",
            ErrorPayload {
                code: code.to_string(),
                message: message.to_string(),
            },
        );
        self.force(RenState::Error);
    }

    fn broadcast(&self, state: RenState) {
        let _ = self.app.emit("ren://state-changed", StateChangedPayload { state });
        // `send` only fails when there are no subscribers — that's expected at
        // startup and not an error.
        let _ = self.notifier.send(state);
    }

    /// Legal transition matrix. Any unlisted pair is rejected.
    fn is_transition_valid(from: RenState, to: RenState) -> bool {
        use RenState::*;
        matches!(
            (from, to),
            (Initializing, Sleeping)
            | (Initializing, Error)
            | (Sleeping,     Waking)
            | (Sleeping,     Listening)   // hotkey push-to-talk override
            | (Sleeping,     Error)
            | (Waking,       Listening)
            | (Waking,       Sleeping)
            | (Waking,       Error)
            | (Listening,    Thinking)
            | (Listening,    Sleeping)
            | (Listening,    Idle)
            | (Listening,    Error)
            | (Thinking,     Speaking)
            | (Thinking,     Idle)
            | (Thinking,     Sleeping)
            | (Thinking,     Error)
            | (Speaking,     Idle)
            | (Speaking,     Sleeping)
            | (Speaking,     Error)
            | (Idle,         Listening)
            | (Idle,         Sleeping)
            | (Idle,         Error)
            | (Error,        Sleeping)
        )
    }
}

/// Thread-safe wrapper, shared across Tauri commands and background tasks.
pub type SharedStateMachine = Arc<Mutex<RenStateMachine>>;

pub fn new_shared(app: AppHandle) -> SharedStateMachine {
    Arc::new(Mutex::new(RenStateMachine::new(app)))
}
