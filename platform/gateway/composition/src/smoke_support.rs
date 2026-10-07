use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

use anyhow::{Context, Result, anyhow, bail};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::Deserialize;
use serde_json::Value;

use veoveo_testing_support::*;
#[path = "smoke_support/audit.rs"]
pub mod audit;
pub use audit::*;
#[path = "smoke_support/control_plane.rs"]
pub mod control_plane;
pub use control_plane::*;
#[path = "smoke_support/gateway_auth.rs"]
pub mod gateway_auth;
pub use gateway_auth::*;
#[path = "smoke_support/gpu.rs"]
pub mod gpu;
pub use gpu::*;
#[path = "smoke_support/installation.rs"]
pub mod installation;
pub use installation::*;
#[path = "smoke_support/services.rs"]
pub mod services;
pub use services::*;
#[path = "smoke_support/types.rs"]
pub mod types;
pub use types::*;
#[path = "smoke_support/usage.rs"]
pub mod usage;
pub use usage::*;
#[path = "smoke_support/auth/context.rs"]
pub mod auth;
#[path = "smoke/fixture_profile_count.rs"]
mod fixture_profile_count;
pub use fixture_profile_count::*;

#[path = "smoke_support/fixture_client.rs"]
pub mod fixture_client;
pub use fixture_client::*;
