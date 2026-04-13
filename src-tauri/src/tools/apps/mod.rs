//! Application launcher — scans the Start Menu, fuzzy-matches on user
//! input, and opens apps via `std::process::Command` or `ShellExecute`.

pub mod launcher;

pub use launcher::AppLauncher;
