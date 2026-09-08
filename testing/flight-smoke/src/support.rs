use anyhow::{Context, Result, bail, ensure};
use std::{
    env,
    ffi::OsString,
    fs::{self, File},
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
};

#[path = "../../smoke/src/bin/smoke/support/gateway_auth/context.rs"]
mod auth;
#[path = "../../smoke/src/bin/smoke/support/gpu.rs"]
mod gpu;
#[allow(dead_code)]
#[path = "../../smoke/src/bin/smoke/support/process.rs"]
mod process;
pub(crate) use auth::gateway_token_for_context;
pub(crate) use gpu::{NvidiaGpuUuid, parse_single_nvidia_smi_gpu};
pub(crate) use process::{assert_executable, run_checked};

pub(crate) fn contains(haystack: &str, needle: &str) -> Result<()> {
    ensure!(
        haystack.contains(needle),
        "expected output to contain `{needle}`\noutput:\n{haystack}"
    );
    Ok(())
}
