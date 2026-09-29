#[cfg(feature = "runtime")]
pub mod cache;
#[cfg(feature = "runtime")]
pub mod composition;
#[cfg(feature = "contract")]
pub mod contract;
#[cfg(feature = "runtime")]
pub mod decode;
#[cfg(feature = "runtime")]
pub mod geodesy;
#[cfg(feature = "runtime")]
pub mod renderer;
#[cfg(feature = "mcp")]
pub mod server;
#[cfg(feature = "runtime")]
pub mod source;
#[cfg(feature = "runtime")]
pub mod state;
#[cfg(feature = "runtime")]
pub mod tiles;
#[cfg(feature = "contract")]
pub mod uris;

#[cfg(feature = "mcp")]
mod app;
#[cfg(feature = "mcp")]
mod mcp;

#[cfg(feature = "mcp")]
pub use server::run;

#[cfg(feature = "contract")]
pub use contract::*;
#[cfg(feature = "runtime")]
pub use state::ViewService;
