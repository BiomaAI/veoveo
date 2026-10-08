//! Browser hardware contracts and optional headed CDP transport.
#[cfg(feature = "contract")]
mod hardware;
#[cfg(feature = "contract")]
pub use hardware::{BrowserGpuApi, HardwareIdentity, software_renderer};

#[cfg(feature = "runtime")]
mod transport;
#[cfg(feature = "runtime")]
pub use transport::*;
