//! Persist request identity and redacted outcomes before evaluating assertions.
use super::*;
use std::{
    fs::File,
    future::Future,
    io::{Seek, SeekFrom, Write},
    os::unix::fs::OpenOptionsExt,
};
use veoveo_mcp_conformance::client::failure::ObservedFailure;
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub(super) enum Phase {
    #[vocabulary(rename = "admission")]
    Admission,
    #[vocabulary(rename = "discovery")]
    Discovery,
    #[vocabulary(rename = "administration")]
    Administration,
    #[vocabulary(rename = "transport")]
    Transport,
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
pub(super) enum Method {
    #[vocabulary(rename = "fixture_admission")]
    FixtureAdmission,
    #[vocabulary(rename = "tools_list")]
    ToolsList,
    #[vocabulary(rename = "resources_list")]
    ResourcesList,
    #[vocabulary(rename = "resource_templates_list")]
    ResourceTemplatesList,
    #[vocabulary(rename = "prompts_list")]
    PromptsList,
    #[vocabulary(rename = "resources_read")]
    ResourcesRead,
    #[vocabulary(rename = "completion_complete")]
    CompletionComplete,
    #[vocabulary(rename = "http_get")]
    HttpGet,
    #[vocabulary(rename = "http_post")]
    HttpPost,
}
#[derive(Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub(super) enum Target {
    Gateway,
    Resource(ResourceUri),
    ResourceTemplate(veoveo_types::ResourceTemplateUri),
    Http(url::Url),
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Observation {
    Pending,
    Success,
    Failure {
        failure: ObservedFailure,
    },
    Unresolved,
    RequestFailed,
    Transport,
    Http {
        status: u16,
        message_digest: Sha256Digest,
    },
    Timeout,
}
#[derive(Serialize)]
struct Entry {
    phase: Phase,
    method: Method,
    target: Target,
    expected: Option<ObservedFailure>,
    response_status: Option<u16>,
    observed: Observation,
}
pub(super) struct Evidence {
    file: File,
    receipt: ProtocolReceipt,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProtocolReceipt {
    schema: &'static str,
    fixture: Option<ProtocolFixture>,
    entries: Vec<Entry>,
    administrator: Option<admin::AdminReceipt>,
    prompt_isolation: Option<discovery::PromptIsolationObservation>,
    prompt_degradations: Vec<veoveo_gateway_contract::GatewayDiscoveryDegradation>,
    operation_passed: bool,
    cleanup_closed: bool,
    qualified: bool,
}
impl Evidence {
    pub(super) fn create(path: &Path) -> Result<Self> {
        ensure!(
            path.is_absolute(),
            "protocol receipt requires an absolute path"
        );
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)?;
        let mut value = Self {
            file,
            receipt: ProtocolReceipt {
                schema: "veoveo.ai/installed-protocol/v1",
                fixture: None,
                entries: Vec::new(),
                administrator: None,
                prompt_isolation: None,
                prompt_degradations: Vec::new(),
                operation_passed: false,
                cleanup_closed: false,
                qualified: false,
            },
        };
        value.persist()?;
        Ok(value)
    }
    fn persist(&mut self) -> Result<()> {
        self.file.seek(SeekFrom::Start(0))?;
        self.file.set_len(0)?;
        serde_json::to_writer(&mut self.file, &self.receipt)?;
        self.file.flush()?;
        self.file.sync_data()?;
        Ok(())
    }
    fn begin(
        &mut self,
        phase: Phase,
        method: Method,
        target: Target,
        expected: Option<ObservedFailure>,
    ) -> Result<usize> {
        let index = self.receipt.entries.len();
        self.receipt.entries.push(Entry {
            phase,
            method,
            target,
            expected,
            response_status: None,
            observed: Observation::Pending,
        });
        self.persist()?;
        Ok(index)
    }
    fn finish(&mut self, index: usize, observed: Observation) -> Result<()> {
        self.receipt.entries[index].observed = observed;
        self.persist()
    }
    pub(super) async fn success<T>(
        &mut self,
        phase: Phase,
        method: Method,
        target: Target,
        request: impl Future<Output = Result<T>>,
    ) -> Result<T> {
        let index = self.begin(phase, method, target, None)?;
        let seconds = if matches!(
            method,
            Method::ToolsList
                | Method::ResourcesList
                | Method::ResourceTemplatesList
                | Method::PromptsList
        ) {
            30
        } else {
            15
        };
        match tokio::time::timeout(Duration::from_secs(seconds), request).await {
            Ok(Ok(value)) => {
                self.finish(index, Observation::Success)?;
                Ok(value)
            }
            Ok(Err(error)) => {
                let observed = match error.downcast_ref::<rmcp::ServiceError>() {
                    Some(rmcp::ServiceError::McpError(error)) => Observation::Failure {
                        failure: ObservedFailure::mcp(
                            i64::from(error.code.0),
                            error.message.as_ref(),
                        ),
                    },
                    _ => Observation::RequestFailed,
                };
                self.finish(index, observed)?;
                anyhow::bail!("protocol request failed; see redacted receipt")
            }
            Err(_) => {
                self.finish(index, Observation::Timeout)?;
                anyhow::bail!("protocol request deadline; see redacted receipt")
            }
        }
    }
    pub(super) async fn deny<T>(
        &mut self,
        phase: Phase,
        method: Method,
        target: Target,
        expected: ObservedFailure,
        request: impl Future<Output = std::result::Result<T, rmcp::ServiceError>>,
    ) -> Result<()> {
        let index = self.begin(phase, method, target, Some(expected.clone()))?;
        let observed = match tokio::time::timeout(Duration::from_secs(15), request).await {
            Ok(Err(rmcp::ServiceError::McpError(error))) => Observation::Failure {
                failure: ObservedFailure::mcp(error.code.0 as i64, error.message.as_ref()),
            },
            Ok(Err(_)) => Observation::Transport,
            Ok(Ok(_)) => Observation::Success,
            Err(_) => Observation::Timeout,
        };
        let matched = matches!(&observed, Observation::Failure { failure } if failure == &expected);
        self.finish(index, observed)?;
        ensure!(
            matched,
            "protocol denial differed from expected response; see redacted receipt"
        );
        Ok(())
    }
    pub(super) async fn http_get(
        &mut self,
        client: &reqwest::Client,
        url: url::Url,
        bearer: Option<&str>,
        host: Option<&str>,
        expected: u16,
    ) -> Result<Vec<u8>> {
        use futures::StreamExt;
        use sha2::{Digest, Sha256};
        let index = self.begin(
            Phase::Administration,
            Method::HttpGet,
            Target::Http(url.clone()),
            Some(ObservedFailure::Http { status: expected }),
        )?;
        let request = async {
            let mut request = client.get(url);
            if let Some(bearer) = bearer {
                request = request.bearer_auth(bearer);
            }
            if let Some(host) = host {
                request = request.header(reqwest::header::HOST, host);
            }
            let response = request.send().await?;
            let status = response.status().as_u16();
            self.receipt.entries[index].response_status = Some(status);
            self.persist()?;
            let mut stream = response.bytes_stream();
            let mut body = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                ensure!(
                    body.len().saturating_add(chunk.len()) <= 65536,
                    "HTTP response exceeds 64KiB"
                );
                body.extend_from_slice(&chunk);
            }
            Ok::<_, anyhow::Error>((status, body))
        };
        match tokio::time::timeout(Duration::from_secs(15), request).await {
            Ok(Ok((status, body))) => {
                self.finish(
                    index,
                    Observation::Http {
                        status,
                        message_digest: Sha256Digest::from_bytes(Sha256::digest(&body).into()),
                    },
                )?;
                ensure!(
                    status == expected,
                    "HTTP status differed; see redacted receipt"
                );
                Ok(body)
            }
            Ok(Err(_)) => {
                self.finish(index, Observation::Transport)?;
                anyhow::bail!("HTTP transport failed; see redacted receipt")
            }
            Err(_) => {
                self.finish(index, Observation::Timeout)?;
                anyhow::bail!("HTTP deadline; see redacted receipt")
            }
        }
    }

    pub(super) fn begin_http(
        &mut self,
        phase: Phase,
        method: Method,
        url: url::Url,
        expected: u16,
    ) -> Result<usize> {
        self.begin(
            phase,
            method,
            Target::Http(url),
            Some(ObservedFailure::Http { status: expected }),
        )
    }
    pub(super) fn http_status(&mut self, index: usize, status: u16) -> Result<()> {
        self.receipt.entries[index].response_status = Some(status);
        self.persist()
    }
    pub(super) fn http_observed(
        &mut self,
        index: usize,
        status: u16,
        message_digest: Sha256Digest,
    ) -> Result<()> {
        self.receipt.entries[index].response_status = Some(status);
        self.finish(
            index,
            Observation::Http {
                status,
                message_digest,
            },
        )
    }
    pub(super) fn transport_failed(&mut self, index: usize, timeout: bool) -> Result<()> {
        self.finish(
            index,
            if timeout {
                Observation::Timeout
            } else {
                Observation::Transport
            },
        )
    }

    pub(super) fn admitted(&mut self, fixture: ProtocolFixture) -> Result<()> {
        self.receipt.fixture = Some(fixture);
        let index = self.begin(
            Phase::Admission,
            Method::FixtureAdmission,
            Target::Gateway,
            None,
        )?;
        self.finish(index, Observation::Success)
    }
    pub(super) fn administrator(&mut self, receipt: &admin::AdminReceipt) -> Result<()> {
        self.receipt.administrator = Some(receipt.clone());
        self.persist()
    }
    pub(super) fn isolation(
        &mut self,
        receipt: discovery::PromptIsolationObservation,
    ) -> Result<()> {
        self.receipt.prompt_isolation = Some(receipt);
        self.persist()
    }
    pub(super) fn prompt_degradation(
        &mut self,
        observed: &veoveo_gateway_contract::GatewayDiscoveryDegradation,
    ) -> Result<()> {
        self.receipt.prompt_degradations.push(observed.clone());
        self.persist()
    }
    pub(super) fn settled(
        &mut self,
        passed: bool,
        cleanup: bool,
        qualified: bool,
        timed_out: bool,
    ) -> Result<()> {
        for entry in &mut self.receipt.entries {
            if matches!(entry.observed, Observation::Pending) {
                entry.observed = if timed_out {
                    Observation::Timeout
                } else {
                    Observation::Unresolved
                };
            }
        }
        self.receipt.operation_passed = passed;
        self.receipt.cleanup_closed = cleanup;
        self.receipt.qualified = passed && cleanup && qualified;
        self.persist()
    }
}

#[cfg(test)]
mod protocol_evidence_tests {
    use super::*;
    #[tokio::test]
    async fn direct_mcp_denial_is_persisted_but_nested_transport_is_refused() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("receipt.json");
        let mut evidence = Evidence::create(&path)?;
        let expected = ObservedFailure::mcp(-32602, "unknown resource address");
        evidence
            .deny::<()>(
                Phase::Discovery,
                Method::ResourcesRead,
                Target::Gateway,
                expected.clone(),
                async {
                    Err(rmcp::ServiceError::McpError(
                        rmcp::ErrorData::invalid_params("unknown resource address", None),
                    ))
                },
            )
            .await?;
        let nested =
            rmcp::ServiceError::TransportSend(rmcp::transport::DynamicTransportError::from_parts(
                "isolated",
                std::any::TypeId::of::<()>(),
                Box::new(rmcp::ErrorData::invalid_params(
                    "unknown resource address",
                    None,
                )),
            ));
        assert!(
            evidence
                .deny::<()>(
                    Phase::Discovery,
                    Method::ResourcesRead,
                    Target::Gateway,
                    expected,
                    async { Err(nested) }
                )
                .await
                .is_err()
        );
        let report: serde_json::Value = serde_json::from_slice(&fs::read(&path)?)?;
        assert_eq!(report["entries"][0]["observed"]["kind"], "failure");
        assert_eq!(report["entries"][1]["observed"]["kind"], "transport");
        assert!(!fs::read_to_string(path)?.contains("unknown resource address"));
        Ok(())
    }
    #[tokio::test]
    async fn persistence_failure_prevents_request_dispatch() -> Result<()> {
        use std::cell::Cell;
        let ran = Cell::new(false);
        let directory = tempfile::tempdir()?;
        let mut evidence = Evidence::create(&directory.path().join("receipt.json"))?;
        evidence.file = File::open(directory.path().join("receipt.json"))?;
        let result = evidence
            .success(
                Phase::Discovery,
                Method::ToolsList,
                Target::Gateway,
                async {
                    ran.set(true);
                    Ok(())
                },
            )
            .await;
        assert!(result.is_err());
        assert!(!ran.get());
        Ok(())
    }
}
