pub mod mpris;
pub mod paths;
pub mod playback;
pub mod queue;
pub mod shutdown;
#[cfg(all(feature = "tray", target_os = "linux"))]
pub mod tray;
