//! Bounded Computer terminal transport shared by the gateway and Console BFF.
mod deadline;
mod relay;
mod upstream;
pub use relay::relay;
pub use upstream::{Client, Upstream, UpstreamRequest};

pub const MAX_MESSAGE_BYTES: usize = 64 * 1024;
pub const MAX_CONTROL_BYTES: usize = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum TransportError {
    #[error("Computer access expired; connect again with current access")]
    Expired,
    #[error("Computer terminal protocol is invalid")]
    Protocol,
    #[error("Computer connection was interrupted")]
    Interrupted,
}
type Result<T> = std::result::Result<T, TransportError>;
