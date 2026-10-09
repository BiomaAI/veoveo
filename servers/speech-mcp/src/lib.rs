//! Governed speech transcription and the private hardware inference boundary.
#[cfg(feature = "contract")]
pub use veoveo_speech_contract as contract;
#[cfg(feature = "runtime")]
pub mod application;
#[cfg(feature = "runtime")]
pub mod dictation;
#[cfg(feature = "runtime")]
mod model;
#[cfg(feature = "runtime")]
pub mod process;
#[cfg(feature = "mcp")]
pub mod server;

#[cfg(feature = "runtime")]
pub mod worker;

#[cfg(feature = "gateway")]
pub mod gateway;

#[cfg(all(test, feature = "runtime"))]
#[path = "../../../testing/fixtures/store.rs"]
pub(crate) mod store_fixture;
#[cfg(all(test, feature = "runtime"))]
#[path = "../tests/support/mod.rs"]
pub(crate) mod test_support;
