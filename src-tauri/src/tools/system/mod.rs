//! System control + awareness tools.
//! Control: volume, lock, shutdown, restart.
//! Awareness: active window, resource usage snapshot, running apps.

pub mod active_window;
pub mod lock;
pub mod resource_usage;
pub mod restart;
pub mod running_apps;
pub mod shutdown;
pub mod volume;

pub use active_window::ActiveWindow;
pub use lock::LockScreen;
pub use resource_usage::ResourceUsage;
pub use restart::RestartSystem;
pub use running_apps::RunningApps;
pub use shutdown::ShutdownSystem;
pub use volume::VolumeControl;
