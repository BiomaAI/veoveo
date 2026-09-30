//! OTLP/HTTP JSON Logs profile. Send directly so a committed receipt means the
//! collector acknowledged the request, rather than an SDK accepting a queue item.
use super::{
    ExportError,
    config::{OtlpConfig, destination_id, endpoint},
    encode, response_bytes,
};
use serde::{Deserialize, Serialize};
use veoveo_audit_contract::*;

pub(super) struct OtlpDestination {
    pub id: AuditDestinationId,
    config: OtlpConfig,
    authorization: Option<http::HeaderValue>,
    http: reqwest::Client,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    resource_logs: Vec<ResourceLogs>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResourceLogs {
    resource: Resource,
    scope_logs: Vec<ScopeLogs>,
}
#[derive(Serialize)]
struct Resource {
    attributes: Vec<Attribute>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScopeLogs {
    scope: Scope,
    log_records: Vec<LogRecord>,
}
#[derive(Serialize)]
struct Scope {
    name: &'static str,
    version: &'static str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LogRecord {
    time_unix_nano: String,
    observed_time_unix_nano: String,
    severity_number: u8,
    event_name: &'static str,
    body: StringValue,
    attributes: Vec<Attribute>,
    trace_id: AuditTraceId,
    span_id: AuditSpanId,
}
#[derive(Serialize)]
struct Attribute {
    key: &'static str,
    value: StringValue,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StringValue {
    string_value: String,
}
impl Attribute {
    fn text(key: &'static str, value: impl ToString) -> Self {
        Self {
            key,
            value: StringValue {
                string_value: value.to_string(),
            },
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Response {
    partial_success: Option<PartialSuccess>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PartialSuccess {
    #[serde(default)]
    rejected_log_records: RejectedCount,
}
#[derive(Deserialize)]
#[serde(untagged)]
enum RejectedCount {
    Text(String),
    Number(u64),
}
impl Default for RejectedCount {
    fn default() -> Self {
        Self::Number(0)
    }
}
impl RejectedCount {
    fn is_zero(&self) -> bool {
        match self {
            Self::Text(s) => s == "0",
            Self::Number(n) => *n == 0,
        }
    }
}
impl OtlpDestination {
    pub fn new(config: OtlpConfig, http: reqwest::Client) -> Result<Self, ExportError> {
        endpoint(&config.endpoint, config.allow_http)?;
        let authorization = config
            .bearer_token_env
            .as_ref()
            .map(|name| {
                let token = zeroize::Zeroizing::new(std::env::var(name).map_err(|_| {
                    ExportError::Configuration("audit OTLP bearer environment variable is missing")
                })?);
                if token.is_empty() {
                    return Err(ExportError::Configuration(
                        "audit OTLP bearer token is empty",
                    ));
                }
                let value = zeroize::Zeroizing::new(format!("Bearer {}", token.as_str()));
                let mut header = http::HeaderValue::from_str(&value)
                    .map_err(|_| ExportError::Configuration("invalid audit OTLP bearer token"))?;
                header.set_sensitive(true);
                Ok(header)
            })
            .transpose()?;
        Ok(Self {
            id: destination_id("otlp-json", &config)?,
            config,
            authorization,
            http,
        })
    }
    pub fn payload(records: &[AuditRecord]) -> Result<Vec<u8>, ExportError> {
        let logs = records
            .iter()
            .map(|record| {
                let draft = &record.draft;
                let time = draft
                    .occurred_at()
                    .timestamp_nanos_opt()
                    .filter(|n| *n >= 0)
                    .ok_or(ExportError::Protocol)?;
                let observed = record
                    .recorded_at
                    .timestamp_nanos_opt()
                    .filter(|n| *n >= 0)
                    .ok_or(ExportError::Protocol)?;
                let event_name = match draft.detail().class() {
                    AuditClass::ApiActivity => "veoveo.audit.api_activity",
                    AuditClass::Authentication => "veoveo.audit.authentication",
                    AuditClass::AccountChange => "veoveo.audit.account_change",
                    AuditClass::ArtifactActivity => "veoveo.audit.artifact_activity",
                    AuditClass::ComputerActivity => "veoveo.audit.computer_activity",
                    AuditClass::LiveViewAccess => "veoveo.audit.live_view_access",
                };
                let body = String::from_utf8(encode(
                    &super::ocsf::Event::from_record(record),
                    super::MAX_CONTENT_BYTES,
                )?)
                .map_err(|_| ExportError::Protocol)?;
                Ok(LogRecord {
                    time_unix_nano: time.to_string(),
                    observed_time_unix_nano: observed.to_string(),
                    severity_number: 9,
                    event_name,
                    body: StringValue { string_value: body },
                    attributes: vec![
                        Attribute::text("veoveo.audit.id", draft.id()),
                        Attribute::text("veoveo.audit.partition", draft.partition().storage_key()),
                        Attribute::text("veoveo.request.id", draft.request().id),
                    ],
                    trace_id: draft.request().trace_id.clone(),
                    span_id: draft.request().span_id.clone(),
                })
            })
            .collect::<Result<Vec<_>, ExportError>>()?;
        encode(
            &Request {
                resource_logs: vec![ResourceLogs {
                    resource: Resource {
                        attributes: vec![Attribute::text("service.name", "veoveo-audit")],
                    },
                    scope_logs: vec![ScopeLogs {
                        scope: Scope {
                            name: "veoveo.audit",
                            version: "1",
                        },
                        log_records: logs,
                    }],
                }],
            },
            super::MAX_CONTENT_BYTES,
        )
    }
    pub async fn deliver(&self, payload: &[u8]) -> Result<(), ExportError> {
        let mut request = self
            .http
            .post(self.config.endpoint.clone())
            .header("content-type", "application/json")
            .body(payload.to_vec());
        if let Some(value) = &self.authorization {
            request = request.header(http::header::AUTHORIZATION, value);
        }
        let response = request.send().await.map_err(|_| ExportError::Unavailable)?;
        if matches!(response.status().as_u16(), 429 | 502 | 503 | 504) {
            if let Some(value) = response
                .headers()
                .get("retry-after")
                .and_then(|h| h.to_str().ok())
            {
                let delay = value
                    .parse::<u64>()
                    .ok()
                    .map(std::time::Duration::from_secs)
                    .or_else(|| {
                        chrono::DateTime::parse_from_rfc2822(value)
                            .ok()
                            .and_then(|time| {
                                (time.with_timezone(&chrono::Utc) - chrono::Utc::now())
                                    .to_std()
                                    .ok()
                            })
                    });
                if let Some(delay) = delay {
                    tokio::time::Instant::now()
                        .checked_add(delay)
                        .ok_or(ExportError::Protocol)?;
                    return Err(ExportError::RetryAfter(delay));
                }
            }
            return Err(ExportError::Unavailable);
        }
        if response.status() != reqwest::StatusCode::OK {
            return Err(ExportError::Protocol);
        }
        let bytes = response_bytes(response, 65_536).await?;
        let response: Response =
            serde_json::from_slice(&bytes).map_err(|_| ExportError::Protocol)?;
        // OTLP prohibits retrying a partial-success response. Preserve intent and
        // fail operator-visible rather than treating partial acceptance as delivery.
        if response
            .partial_success
            .is_some_and(|partial| !partial.rejected_log_records.is_zero())
        {
            return Err(ExportError::PartialAcceptance);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Router,
        extract::State,
        http::{HeaderMap, StatusCode},
        response::IntoResponse,
        routing::post,
    };
    use std::{
        sync::{
            Arc,
            atomic::{AtomicU8, Ordering},
        },
        time::Duration,
    };
    async fn collector(
        State(case): State<Arc<AtomicU8>>,
        headers: HeaderMap,
        body: axum::body::Bytes,
    ) -> axum::response::Response {
        assert_eq!(headers.get("content-type").unwrap(), "application/json");
        assert!(!body.is_empty());
        match case.load(Ordering::SeqCst) {
            0 => (StatusCode::OK, [("content-type", "application/json")], "{}").into_response(),
            1 => (StatusCode::OK, [("content-type", "application/json")], r#"{"partialSuccess":{"rejectedLogRecords":"1","errorMessage":"provider-private-details"}}"#).into_response(),
            2 => (StatusCode::TOO_MANY_REQUESTS, [("retry-after", "90")], "").into_response(),
            3 => (StatusCode::BAD_REQUEST, "provider-private-details").into_response(),
            _ => (StatusCode::OK, [("content-type", "application/json")], r#"{"partialSuccess":{"rejectedLogRecords":0,"errorMessage":"warning"}}"#).into_response(),
        }
    }
    struct Server(tokio::task::JoinHandle<()>);
    impl Drop for Server {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    #[tokio::test]
    async fn receipt_requires_zero_rejections_and_retry_after_is_preserved() {
        tokio::time::timeout(Duration::from_secs(30), async {
            let case = Arc::new(AtomicU8::new(0));
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let app = Router::new().route("/v1/logs", post(collector)).with_state(case.clone());
            let mut server = Server(tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); }));
            let destination = OtlpDestination::new(OtlpConfig { endpoint: format!("http://{address}/v1/logs").parse().unwrap(), allow_http: true, bearer_token_env: None }, reqwest::Client::new()).unwrap();
            destination.deliver(b"{}").await.unwrap();
            case.store(1, Ordering::SeqCst);
            let error = destination.deliver(b"{}").await.unwrap_err();
            assert!(matches!(error, ExportError::PartialAcceptance));
            assert!(!error.to_string().contains("provider-private-details"));
            case.store(2, Ordering::SeqCst);
            assert!(matches!(destination.deliver(b"{}").await, Err(ExportError::RetryAfter(delay)) if delay == Duration::from_secs(90)));
            case.store(3, Ordering::SeqCst);
            assert!(matches!(destination.deliver(b"{}").await, Err(ExportError::Protocol)));
            case.store(4, Ordering::SeqCst);
            destination.deliver(b"{}").await.unwrap();
            server.0.abort(); let _ = (&mut server.0).await;
        }).await.expect("OTLP export fixture exceeded 30 seconds");
    }
}
