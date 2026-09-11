//! Private bounded argv transport. Authority and Task settlement remain caller-owned.
mod files;
mod protocol;
pub use files::{
    FileFailure, FileReceipt, FileRequest, FileTransferBounds, MAX_FILE_BYTES, read_file_request,
    write_file_result,
};
pub use protocol::{ExecutionRequest, LaunchError, MAX_FRAME_BYTES, read_frame};

#[cfg(target_os = "linux")]
mod file_confinement;
#[cfg(target_os = "linux")]
mod file_io;
#[cfg(target_os = "linux")]
mod launcher;
#[cfg(target_os = "linux")]
pub use file_io::transfer_file;
#[cfg(target_os = "linux")]
pub use launcher::launch;

pub const LAUNCHER_PATH: &str = "/usr/local/bin/veoveo-computer-exec";
pub const RETAINED_HOME: &str = "/sandbox/persistent";
pub const PROTOCOL_VERSION: u8 = 1;

#[cfg(test)]
mod tests;
