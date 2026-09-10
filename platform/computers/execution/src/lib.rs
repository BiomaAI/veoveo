//! Private bounded argv transport. Authority and Task settlement remain caller-owned.
mod protocol;
pub use protocol::{ExecutionRequest, LaunchError, MAX_FRAME_BYTES, read_frame};

#[cfg(target_os = "linux")]
mod launcher;
#[cfg(target_os = "linux")]
pub use launcher::launch;

pub const LAUNCHER_PATH: &str = "/usr/local/bin/veoveo-computer-exec";
pub const RETAINED_HOME: &str = "/sandbox/persistent";
pub const PROTOCOL_VERSION: u8 = 1;

#[cfg(test)]
mod tests;
