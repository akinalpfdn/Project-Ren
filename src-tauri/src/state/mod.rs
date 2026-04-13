use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};
use tracing::{info, warn};

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
            RenState::Sleeping    => "sleeping",
            RenState::Waking      => "waking",
            RenState::Listening   => "listening",
            RenState::Thinking    => "thinking",
            RenState::Speaking    => "speaking",
            RenState::Idle        => "idle",
            RenState::Error       => "error",
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
/// No other code transitions state directly — all changes go through `transition()`.
pub struct RenStateMachine {
    current: RenState,
    app: AppHandle,
}

impl RenStateMachine {
    pub fn new(app: AppHandle) -> Self {
        Self {
            current: RenState::Initializing,
            app,
        }
    }

    pub fn current(&self) -> RenState {
        self.current
    }

    /// Attempt a state transition.
    /// Validates the transition is legal, runs side effects, emits the Tauri event.
    pub fn transition(&mut self, to: RenState) -> Result<(), String> {
        if !self.is_transition_valid(self.current, to) {
            let msg = format!(
                "Invalid state transition: {} → {}",
                self.current, to
            );
            warn!("{}", msg);
            return Err(msg);
        }

        info!("State: {} → {}", self.current, to);
        self.current = to;
        self.emit_state_changed(to);
        Ok(())
    }

    /// Force a transition bypassing validation (e.g. error recovery).
    pub fn force(&mut self, to: RenState) {
        info!("State (forced): {} → {}", self.current, to);
        self.current = to;
        self.emit_state_changed(to);
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

    fn emit_state_changed(&self, state: RenState) {
        let _ = self.app.emit("ren://state-changed", StateChangedPayload { state });
    }

    /// Legal transitions. Any unlisted pair is rejected.
    fn is_transition_valid(&self, from: RenState, to: RenState) -> bool {
        use RenState::*;
        matches!(
            (from, to),
            (Initializing, Sleeping)
            | (Initializing, Error)
            | (Sleeping,    Waking)
            | (Sleeping,    Error)
            | (Waking,      Listening)
            | (Waking,      Sleeping)
            | (Waking,      Error)
            | (Listening,   Thinking)
            | (Listening,   Sleeping)
            | (Listening,   Error)
            | (Thinking,    Speaking)
            | (Thinking,    Idle)
            | (Thinking,    Sleeping)
            | (Thinking,    Error)
            | (Speaking,    Idle)
            | (Speaking,    Sleeping)
            | (Speaking,    Error)
            | (Idle,        Listening)
            | (Idle,        Sleeping)
            | (Idle,        Error)
            | (Error,       Sleeping)
        )
    }
}

/// Thread-safe wrapper, shared across Tauri commands and background tasks.
pub type SharedStateMachine = Arc<Mutex<RenStateMachine>>;

pub fn new_shared(app: AppHandle) -> SharedStateMachine {
    Arc::new(Mutex::new(RenStateMachine::new(app)))
}
