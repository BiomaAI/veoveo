//! Private fixture teardown observations distinguish cleanup from scenario success.
use super::*;
use std::{ffi::OsString, os::unix::ffi::OsStringExt};
use veoveo_types::Sha256Digest;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopFormat {
    #[serde(rename = "veoveo.ai/smoke-stop/v1")]
    V1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TeardownFormat {
    #[serde(rename = "veoveo.ai/smoke-teardown/v1")]
    V1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeardownCause {
    RequestedStop,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeardownOutcome {
    CleanupSettled,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct StopRequest {
    format: StopFormat,
    pub pid: u32,
    pub start_ticks: u64,
    pub invocation_digest: Sha256Digest,
    pub stop_id: uuid::Uuid,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TeardownReceipt {
    format: TeardownFormat,
    pid: u32,
    start_ticks: u64,
    invocation_digest: Sha256Digest,
    stop_id: uuid::Uuid,
    cause: TeardownCause,
    outcome: TeardownOutcome,
    settled_identity_count: u64,
}
fn invocation(pid: u32) -> Result<Sha256Digest> {
    let raw = fs::read(format!("/proc/{pid}/cmdline"))?;
    ensure!(
        !raw.is_empty() && raw.len() <= 65536,
        "invalid owned invocation metadata"
    );
    Ok(veoveo_mcp_conformance::client::failure::arguments_digest(
        raw.split(|b| *b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| OsString::from_vec(s.to_vec())),
    ))
}
fn request_path(root: &Path, pid: u32) -> PathBuf {
    root.join(format!(".stop-{pid}.json"))
}
fn receipt_path(root: &Path, pid: u32) -> PathBuf {
    root.join(format!(".teardown-{pid}.json"))
}
fn publish<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let bytes = serde_json::to_vec(value)?;
    ensure!(
        bytes.len() <= 8192,
        "private teardown observation exceeds limit"
    );
    let staging = path.with_extension(format!("{}.staging", uuid::Uuid::now_v7()));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&staging)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    fs::hard_link(&staging, path)?;
    fs::remove_file(staging)?;
    Ok(())
}
fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let bytes = fs::read(path)?;
    ensure!(
        bytes.len() <= 8192,
        "private teardown observation exceeds limit"
    );
    Ok(serde_json::from_slice(&bytes)?)
}
pub(crate) fn request(root: &Path, pid: u32) -> Result<StopRequest> {
    let (leader, _, start_ticks) = identity(pid)?;
    ensure!(
        leader == pid,
        "fixture is not its admitted process-group leader"
    );
    let value = StopRequest {
        format: StopFormat::V1,
        pid,
        start_ticks,
        invocation_digest: invocation(pid)?,
        stop_id: uuid::Uuid::now_v7(),
    };
    publish(&request_path(root, pid), &value)?;
    Ok(value)
}
pub(crate) fn current_request(root: &Path) -> Result<StopRequest> {
    let pid = std::process::id();
    let value: StopRequest = read(&request_path(root, pid))?;
    let (leader, _, start) = identity(pid)?;
    ensure!(
        leader == pid
            && value.pid == pid
            && value.start_ticks == start
            && value.invocation_digest == invocation(pid)?,
        "stop request does not match the actual owning invocation"
    );
    Ok(value)
}
pub(crate) fn settled(root: &Path, request: &StopRequest, count: usize) -> Result<()> {
    ensure!(
        current_request(root)? == *request,
        "fixture stop identity changed before settlement"
    );
    publish(
        &receipt_path(root, request.pid),
        &TeardownReceipt {
            format: TeardownFormat::V1,
            pid: request.pid,
            start_ticks: request.start_ticks,
            invocation_digest: request.invocation_digest.clone(),
            stop_id: request.stop_id,
            cause: TeardownCause::RequestedStop,
            outcome: TeardownOutcome::CleanupSettled,
            settled_identity_count: u64::try_from(count)?,
        },
    )
}
pub(crate) fn verify(root: &Path, request: &StopRequest) -> Result<()> {
    let receipt: TeardownReceipt = read(&receipt_path(root, request.pid))
        .context("explicit fixture teardown has no settled ownership receipt")?;
    ensure!(
        receipt.pid == request.pid
            && receipt.start_ticks == request.start_ticks
            && receipt.invocation_digest == request.invocation_digest
            && receipt.stop_id == request.stop_id,
        "fixture teardown receipt does not match PID/start/invocation/stop identity"
    );
    // Local group drain is checked by ChildGuard before the leader is reaped.
    fs::remove_file(receipt_path(root, request.pid))?;
    fs::remove_file(request_path(root, request.pid))?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_and_wrong_stop_receipts_never_prove_cleanup() {
        let root = tempfile::tempdir().unwrap();
        let request = StopRequest {
            format: StopFormat::V1,
            pid: 42,
            start_ticks: 10,
            invocation_digest: Sha256Digest::from_bytes([1; 32]),
            stop_id: uuid::Uuid::now_v7(),
        };
        assert!(verify(root.path(), &request).is_err());
        for (pid, start, stop, invocation_digest) in [
            (43, 10, request.stop_id, request.invocation_digest.clone()),
            (42, 11, request.stop_id, request.invocation_digest.clone()),
            (
                42,
                10,
                uuid::Uuid::now_v7(),
                request.invocation_digest.clone(),
            ),
            (42, 10, request.stop_id, Sha256Digest::from_bytes([2; 32])),
        ] {
            publish(
                &receipt_path(root.path(), 42),
                &TeardownReceipt {
                    format: TeardownFormat::V1,
                    pid,
                    start_ticks: start,
                    invocation_digest,
                    stop_id: stop,
                    cause: TeardownCause::RequestedStop,
                    outcome: TeardownOutcome::CleanupSettled,
                    settled_identity_count: 0,
                },
            )
            .unwrap();
            assert!(verify(root.path(), &request).is_err());
            fs::remove_file(receipt_path(root.path(), 42)).unwrap();
        }
    }
}
