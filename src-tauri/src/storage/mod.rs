//! Persistent, encrypted storage primitives that live outside of the plain
//! `config.json` surface.
//!
//! Right now this only exposes a DPAPI-backed credential store; future
//! additions (cached tool results, conversation archive) will share the same
//! module root.

pub mod credentials;

pub use credentials::CredentialStore;
