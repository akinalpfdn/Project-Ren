//! Steam integration — locate installed games and launch them via
//! `steam://rungameid/<appid>` URIs.

pub mod launcher;
pub mod library;
pub mod vdf;

pub use launcher::SteamLauncher;
