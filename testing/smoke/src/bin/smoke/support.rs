#[path = "support/gateway_auth/context.rs"]
mod auth;
use super::*;

#[path = "support/assertions.rs"]
mod assertions;
#[path = "support/audit.rs"]
mod audit;
#[path = "support/control_plane.rs"]
mod control_plane;
#[path = "support/duckdb.rs"]
mod duckdb;
#[path = "support/final_tasks.rs"]
mod final_tasks;
#[path = "support/gateway_auth.rs"]
mod gateway_auth;
#[path = "support/gpu.rs"]
mod gpu;
#[path = "support/http.rs"]
mod http;
#[path = "support/installation.rs"]
#[allow(
    dead_code,
    reason = "This module is also compiled by the focused flight and browser clients"
)]
mod installation;
#[path = "support/mcp.rs"]
mod mcp;
#[path = "support/process.rs"]
mod process;
#[path = "support/services.rs"]
mod services;
#[path = "support/types.rs"]
mod types;
#[path = "support/usage.rs"]
mod usage;

pub(crate) use assertions::*;
pub(crate) use audit::*;
pub(crate) use control_plane::*;
pub(crate) use duckdb::*;
pub(crate) use final_tasks::*;
pub(crate) use gateway_auth::*;
pub(crate) use gpu::*;
pub(crate) use http::*;
pub(crate) use installation::InstalledTarget;
pub(crate) use mcp::*;
pub(crate) use process::*;
pub(crate) use services::*;
pub(crate) use types::*;
pub(crate) use usage::*;
