//! Read-only failure diagnostics; event tails never settle lifecycle operations.
use crate::native_support::Provider;
use prost::Message as _;
use serde::Serialize;
use std::{
    fs,
    io::Write as _,
    os::unix::fs::OpenOptionsExt as _,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tonic::transport::{Certificate, ClientTlsConfig, Endpoint, Identity};
use veoveo_computers_runtime::{
    Binding, LifecycleCheckpoint, Observation,
    protocol::{datamodel::v1 as domain, v1 as api},
};

const WATCH_BUDGET: Duration = Duration::from_secs(10);
const EVENT_TAIL: u32 = 32;
const MAX_STREAM_BYTES: usize = 128 * 1024;
const MAX_RECEIPT_BYTES: usize = 64 * 1024;
const MAX_DISPATCH_WINDOW: Duration = Duration::from_secs(180);

#[derive(Clone, Copy, Serialize)]
pub struct DispatchWindow {
    began_unix_millis: u64,
    ended_unix_millis: u64,
}
impl DispatchWindow {
    pub fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
            .try_into()
            .unwrap()
    }
    pub fn finish(began_unix_millis: u64) -> Self {
        Self {
            began_unix_millis,
            ended_unix_millis: Self::now(),
        }
    }
    fn admits(&self, timestamp: Option<&prost_types::Timestamp>) -> bool {
        let Some(time) = timestamp else { return false };
        if !(0..=253_402_300_799).contains(&time.seconds)
            || !(0..1_000_000_000).contains(&time.nanos)
            || self.ended_unix_millis < self.began_unix_millis
            || self.ended_unix_millis - self.began_unix_millis
                > MAX_DISPATCH_WINDOW.as_millis() as u64
        {
            return false;
        }
        let millis = time.seconds as u64 * 1000 + time.nanos as u64 / 1_000_000;
        (self.began_unix_millis..=self.ended_unix_millis).contains(&millis)
    }
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum End {
    Deadline,
    StreamEnded,
    TransportFailure,
    IdentityMismatch,
    MissingSnapshot,
    ByteLimit,
    EventLimit,
}

fn observation_end(code: tonic::Code) -> End {
    if code == tonic::Code::DeadlineExceeded {
        End::Deadline
    } else {
        End::TransportFailure
    }
}

#[derive(Serialize)]
struct DiagnosticEvent {
    unix_millis: u64,
    source: RedactedText,
    severity: RedactedText,
    reason: RedactedText,
    message: RedactedText,
}

#[derive(Serialize)]
struct RedactedText {
    text: String,
    redacted_lines: usize,
    truncated: bool,
}

#[derive(Serialize)]
struct Receipt<'a> {
    format: &'static str,
    correlation: &'static str,
    lifecycle_outcome: &'static str,
    checkpoint: &'a LifecycleCheckpoint,
    provider_endpoint: &'a str,
    provider_process_id: Option<u32>,
    sandbox_id: &'a str,
    previous_process_id: &'a str,
    redaction_policy: &'static str,
    dispatch_window: DispatchWindow,
    snapshot_matched: bool,
    best_effort_tail: bool,
    gap_warnings: usize,
    rejected_events: usize,
    stream_bytes: usize,
    events: Vec<DiagnosticEvent>,
    end: End,
    transport_code: Option<i32>,
}

fn snapshot_matches(sandbox: &api::Sandbox, binding: &Binding, before: &Observation) -> bool {
    let Some(metadata) = sandbox.metadata.as_ref() else {
        return false;
    };
    metadata.id == before.sandbox_id
        && metadata.name == binding.name()
        && metadata.workspace == "default"
        && metadata.labels.get("veoveo-instance") == binding.labels().get("veoveo-instance")
        && binding
            .labels()
            .iter()
            .all(|(key, value)| metadata.labels.get(key) == Some(value))
}

#[allow(dead_code)] // Shared diagnostics are included by fixtures without CLI output.
pub(super) fn sanitized_output(text: &str, private_values: &[&str], maximum: usize) -> String {
    redacted(text, private_values, maximum).text
}

fn redacted(text: &str, private_values: &[&str], maximum: usize) -> RedactedText {
    let mut output = String::new();
    let mut pem = false;
    let mut redacted_lines = 0;
    let mut truncated = false;
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("-----begin") {
            pem = true;
        }
        let sensitive = pem
            || [
                "authorization",
                "bearer",
                "token",
                "password",
                "secret",
                "credential",
                "private key",
                "client-key",
                "jwt-key",
                "api_key",
                "access_key",
                "passwd",
            ]
            .iter()
            .any(|word| lower.contains(word))
            || private_values
                .iter()
                .any(|value| !value.is_empty() && line.contains(value))
            || line
                .split_whitespace()
                .any(|word| word.matches('.').count() == 2 && word.starts_with("eyJ"));
        let selected = if sensitive {
            redacted_lines += 1;
            "[redacted sensitive line]"
        } else {
            line
        };
        if output.len() + selected.len() + 1 > maximum {
            let remaining = maximum.saturating_sub(output.len());
            let mut end = remaining.min(selected.len());
            while !selected.is_char_boundary(end) {
                end -= 1;
            }
            output.push_str(&selected[..end]);
            truncated = true;
            break;
        }
        output.push_str(selected);
        output.push('\n');
        if lower.contains("-----end") {
            pem = false;
        }
    }
    RedactedText {
        text: output,
        redacted_lines,
        truncated,
    }
}

fn receipt_bytes(receipt: &mut Receipt<'_>) -> std::io::Result<Vec<u8>> {
    loop {
        let bytes = serde_json::to_vec_pretty(receipt)?;
        if bytes.len() <= MAX_RECEIPT_BYTES {
            return Ok(bytes);
        }
        if receipt.events.pop().is_none() {
            return Err(std::io::Error::other(
                "diagnostic identity exceeds receipt byte limit",
            ));
        }
        receipt.end = End::ByteLimit;
        receipt.rejected_events += 1;
    }
}

fn write_receipt(root: &std::path::Path, receipt: &mut Receipt<'_>) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    let metadata = fs::symlink_metadata(root)?;
    if !metadata.is_dir() || metadata.permissions().mode() & 0o777 != 0o700 {
        return Err(std::io::Error::other(
            "diagnostic directory must be owned and private",
        ));
    }
    let bytes = receipt_bytes(receipt)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(root.join("start-failure-diagnostics.json"))?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    file.write_all(&bytes)?;
    file.sync_all()
}

/// Capture one authenticated watch before fixture Drop, with no retry or mutation.
pub async fn capture_start_failure(
    provider: &Provider,
    binding: &Binding,
    before: &Observation,
    checkpoint: &LifecycleCheckpoint,
    window: DispatchWindow,
    private_values: &[&str],
) -> std::io::Result<()> {
    let mut receipt = Receipt {
        format: "veoveo.ai/computer-native-start-diagnostics/v1",
        correlation: "same authenticated fixture endpoint and sandbox snapshot; dispatch-window timestamps; events have no operation epoch",
        lifecycle_outcome: "unchanged; diagnostics do not settle lifecycle uncertainty",
        checkpoint,
        provider_endpoint: &provider.endpoint,
        provider_process_id: provider.process_id(),
        sandbox_id: &before.sandbox_id,
        previous_process_id: &before.main_process_instance_id,
        redaction_policy: "known fixture paths and credential-marked lines suppressed; arbitrary event metadata and log payloads omitted; unmarked upstream text is not guaranteed secret-free",
        dispatch_window: window,
        snapshot_matched: false,
        best_effort_tail: true,
        gap_warnings: 0,
        rejected_events: 0,
        stream_bytes: 0,
        events: Vec::new(),
        end: End::Deadline,
        transport_code: None,
    };
    let result = tokio::time::timeout(WATCH_BUDGET, async {
        let tls = ClientTlsConfig::new()
            .domain_name("localhost")
            .ca_certificate(Certificate::from_pem(fs::read(
                provider.dir.join("ca.pem"),
            )?))
            .identity(Identity::from_pem(
                fs::read(provider.dir.join("client.pem"))?,
                fs::read(provider.dir.join("client-key.pem"))?,
            ));
        let channel = Endpoint::from_shared(format!("https://{}", provider.endpoint))
            .map_err(|_| std::io::Error::other("diagnostic endpoint admission failed"))?
            .tls_config(tls)
            .map_err(|_| std::io::Error::other("diagnostic TLS admission failed"))?
            .connect_timeout(WATCH_BUDGET)
            .timeout(WATCH_BUDGET)
            .connect()
            .await
            .map_err(|_| std::io::Error::other("diagnostic transport unavailable"))?;
        let mut client = api::open_shell_client::OpenShellClient::new(channel)
            .max_decoding_message_size(MAX_RECEIPT_BYTES);
        let request = api::WatchSandboxRequest {
            workspace_scope: Some(domain::WorkspaceSelector {
                selection: Some(domain::workspace_selector::Selection::Workspace(
                    "default".into(),
                )),
            }),
            sandbox: binding.name(),
            follow_status: true,
            follow_events: true,
            event_tail: EVENT_TAIL,
            stop_on_terminal: false,
            ..Default::default()
        };
        let mut request = tonic::Request::new(request);
        request.set_timeout(WATCH_BUDGET);
        let mut stream = match client.watch_sandbox(request).await {
            Ok(response) => response.into_inner(),
            Err(status) => {
                receipt.end = observation_end(status.code());
                receipt.transport_code = Some(status.code() as i32);
                return Ok(());
            }
        };
        loop {
            let event = match stream.message().await {
                Ok(Some(event)) => event,
                Ok(None) => {
                    receipt.end = End::StreamEnded;
                    break;
                }
                Err(status) => {
                    receipt.end = observation_end(status.code());
                    receipt.transport_code = Some(status.code() as i32);
                    break;
                }
            };
            receipt.stream_bytes += event.encoded_len();
            if receipt.stream_bytes > MAX_STREAM_BYTES {
                receipt.end = End::ByteLimit;
                break;
            }
            match event.payload {
                Some(api::sandbox_stream_event::Payload::Sandbox(snapshot)) => {
                    if !snapshot_matches(&snapshot, binding, before) {
                        receipt.end = End::IdentityMismatch;
                        break;
                    }
                    receipt.snapshot_matched = true;
                }
                Some(api::sandbox_stream_event::Payload::Warning(_)) | None => {
                    receipt.gap_warnings += 1
                }
                Some(api::sandbox_stream_event::Payload::Event(event)) => {
                    if !receipt.snapshot_matched {
                        receipt.end = End::MissingSnapshot;
                        break;
                    }
                    if !window.admits(event.event_time.as_ref()) {
                        receipt.rejected_events += 1;
                        continue;
                    }
                    if receipt.events.len() >= EVENT_TAIL as usize {
                        receipt.end = End::EventLimit;
                        break;
                    }
                    let time = event.event_time.as_ref().unwrap();
                    receipt.events.push(DiagnosticEvent {
                        unix_millis: time.seconds as u64 * 1000 + time.nanos as u64 / 1_000_000,
                        source: redacted(&event.source, private_values, 128),
                        severity: redacted(&event.r#type, private_values, 128),
                        reason: redacted(&event.reason, private_values, 128),
                        message: redacted(&event.message, private_values, 16 * 1024),
                    });
                }
                _ => receipt.rejected_events += 1,
            }
        }
        Ok::<_, std::io::Error>(())
    })
    .await;
    if matches!(result, Ok(Err(_))) {
        receipt.end = End::TransportFailure;
    }
    if !receipt.snapshot_matched
        && !matches!(
            receipt.end,
            End::TransportFailure | End::IdentityMismatch | End::Deadline
        )
    {
        receipt.end = End::MissingSnapshot;
    }
    write_receipt(&provider.dir, &mut receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;
    use veoveo_computers_runtime::{LifecycleOperationId, Phase};

    fn binding() -> Binding {
        Binding::new(uuid::Uuid::now_v7(), "a".repeat(64)).unwrap()
    }
    fn before() -> Observation {
        Observation {
            sandbox_id: "same-sandbox".into(),
            phase: Phase::Stopped,
            main_process_instance_id: "previous-process".into(),
            exit_code: None,
        }
    }

    #[test]
    fn diagnostic_identity_refuses_foreign_and_incomplete_snapshots_but_allows_new_process() {
        let binding = binding();
        let before = before();
        let mut sandbox = api::Sandbox::default();
        assert!(!snapshot_matches(&sandbox, &binding, &before));
        sandbox.metadata = Some(Default::default());
        let metadata = sandbox.metadata.as_mut().unwrap();
        metadata.id = before.sandbox_id.clone();
        metadata.name = binding.name();
        metadata.workspace = "default".into();
        metadata.labels = binding.labels().into_iter().collect();
        sandbox.status = Some(api::SandboxStatus {
            main_process_instance_id: "new-start-process".into(),
            ..Default::default()
        });
        assert!(snapshot_matches(&sandbox, &binding, &before));
        sandbox.metadata.as_mut().unwrap().id = "foreign-sandbox".into();
        assert!(!snapshot_matches(&sandbox, &binding, &before));
        sandbox.metadata.as_mut().unwrap().id = before.sandbox_id.clone();
        sandbox.metadata.as_mut().unwrap().workspace = "foreign".into();
        assert!(!snapshot_matches(&sandbox, &binding, &before));
        sandbox.metadata.as_mut().unwrap().workspace = "default".into();
        sandbox
            .metadata
            .as_mut()
            .unwrap()
            .labels
            .insert("veoveo-instance".into(), uuid::Uuid::now_v7().to_string());
        assert!(!snapshot_matches(&sandbox, &binding, &before));
    }

    #[test]
    fn diagnostic_event_clock_rejects_missing_malformed_old_future_and_excessive_windows() {
        let window = DispatchWindow {
            began_unix_millis: 10_000,
            ended_unix_millis: 11_000,
        };
        assert!(!window.admits(None));
        for (seconds, nanos) in [(9, 0), (12, 0), (-1, 0), (10, -1), (10, 1_000_000_000)] {
            assert!(!window.admits(Some(&prost_types::Timestamp { seconds, nanos })));
        }
        assert!(window.admits(Some(&prost_types::Timestamp {
            seconds: 10,
            nanos: 123_000_000
        })));
        assert!(
            !DispatchWindow {
                began_unix_millis: 11_000,
                ended_unix_millis: 10_000
            }
            .admits(Some(&prost_types::Timestamp {
                seconds: 10,
                nanos: 0
            }))
        );
        assert!(
            !DispatchWindow {
                began_unix_millis: 0,
                ended_unix_millis: 181_000
            }
            .admits(Some(&prost_types::Timestamp {
                seconds: 10,
                nanos: 0
            }))
        );
        assert!(WATCH_BUDGET <= MAX_DISPATCH_WINDOW);
        assert!(matches!(
            observation_end(tonic::Code::DeadlineExceeded),
            End::Deadline
        ));
        assert!(matches!(
            observation_end(tonic::Code::Unavailable),
            End::TransportFailure
        ));
    }

    #[test]
    fn diagnostic_text_preserves_failure_cause_while_removing_secrets_and_bounds_utf8() {
        let raw = "supervisor ready wait failed\nAuthorization: Bearer sensitive-value\nAPI_KEY=provider-value\nprivate-transfer-雪.tar\n-----BEGIN PRIVATE KEY-----\nbase64-private-material\n-----END PRIVATE KEY-----\nprocess exited 127\n";
        let admitted = redacted(raw, &["private-transfer-雪.tar"], 1024);
        assert!(admitted.text.contains("supervisor ready wait failed"));
        assert!(admitted.text.contains("process exited 127"));
        for secret in [
            "sensitive-value",
            "provider-value",
            "private-transfer-雪.tar",
            "base64-private-material",
        ] {
            assert!(!admitted.text.contains(secret));
        }
        assert!(admitted.redacted_lines > 0);
        assert!(!admitted.truncated);
        let bounded = redacted(&"雪".repeat(100), &[], 20);
        assert!(bounded.truncated);
        assert!(bounded.text.len() <= 20);
        assert!(!bounded.text.is_empty());
    }

    #[test]
    fn diagnostic_receipt_limits_payload_preserves_uncertainty_and_requires_private_destination() {
        let binding = binding();
        let checkpoint = LifecycleCheckpoint::start(
            "00000000-0000-7000-8000-000000000064".parse().unwrap(),
            LifecycleOperationId::new(),
            binding,
            &before(),
        )
        .unwrap();
        let mut receipt = Receipt {
            format: "veoveo.ai/computer-native-start-diagnostics/v1",
            correlation: "request/snapshot/window only",
            lifecycle_outcome: "unchanged",
            checkpoint: &checkpoint,
            provider_endpoint: "127.0.0.1:1",
            provider_process_id: Some(1),
            sandbox_id: "same-sandbox",
            previous_process_id: "previous-process",
            redaction_policy: "sensitive lines removed",
            dispatch_window: DispatchWindow {
                began_unix_millis: 10_000,
                ended_unix_millis: 11_000,
            },
            snapshot_matched: true,
            best_effort_tail: true,
            gap_warnings: 1,
            rejected_events: 0,
            stream_bytes: MAX_STREAM_BYTES,
            events: Vec::new(),
            end: End::Deadline,
            transport_code: None,
        };
        for _ in 0..EVENT_TAIL {
            receipt.events.push(DiagnosticEvent {
                unix_millis: 10_000,
                source: redacted("docker", &[], 128),
                severity: redacted("Warning", &[], 128),
                reason: redacted("Failed", &[], 128),
                message: redacted(&"failure-details ".repeat(3000), &[], 16 * 1024),
            });
        }
        let bytes = receipt_bytes(&mut receipt).unwrap();
        assert!(bytes.len() <= MAX_RECEIPT_BYTES);
        assert!(!receipt.events.is_empty());
        assert!(receipt.rejected_events > 0 && matches!(receipt.end, End::ByteLimit));
        assert!(receipt.best_effort_tail && receipt.gap_warnings == 1);
        assert_eq!(receipt.lifecycle_outcome, "unchanged");
        let root =
            std::env::temp_dir().join(format!("veoveo-diagnostics-{}", uuid::Uuid::now_v7()));
        fs::create_dir(&root).unwrap();
        struct OwnedDirectory(std::path::PathBuf);
        impl Drop for OwnedDirectory {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = OwnedDirectory(root.clone());
        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(write_receipt(&root, &mut receipt).is_err());
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        write_receipt(&root, &mut receipt).unwrap();
        let path = root.join("start-failure-diagnostics.json");
        assert_eq!(
            fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(
            write_receipt(&root, &mut receipt).is_err(),
            "diagnostic writer must not overwrite a prior receipt"
        );
    }
}
