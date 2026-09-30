//! Gateway-owned export, independent of request latency and lease renewal.
mod config;
pub use config::{AuditExportConfig, BucketName, ObjectLock, ObjectPrefix, OtlpConfig, S3Config};
pub mod ocsf;
mod otlp;
mod s3;
use crate::integrity::{IntegrityError, canonical_bytes, record_root};
use futures::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{io::Write, time::Duration};
use veoveo_audit_contract::*;
use veoveo_platform_store::{PlatformStore, StoreError, audit::AuditSealLease};
use veoveo_types::Sha256Digest;
const MAX_CONTENT_BYTES: usize = 32 * 1024 * 1024;
const MAX_SEAL_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("invalid audit export configuration: {0}")]
    Configuration(&'static str),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Integrity(#[from] IntegrityError),
    #[error("audit destination unavailable; delivery remains unresolved")]
    Unavailable,
    #[error("audit destination requested delayed retry")]
    RetryAfter(Duration),
    #[error("audit destination refused the configured export protocol")]
    Protocol,
    #[error("audit destination content differs from committed delivery intent")]
    Conflict,
    #[error("audit S3 object does not prove versioned compliance-mode retention")]
    ObjectLock,
    #[error("audit collector rejected part of the block; automatic retry is prohibited")]
    PartialAcceptance,
    #[error("audit export LIVE stream disconnected")]
    Disconnected,
    #[error("audit export exceeds the configured wire profile size")]
    Size,
}
impl ExportError {
    fn retry_delay(&self) -> Option<Duration> {
        match self {
            Self::Unavailable => Some(Duration::from_secs(5)),
            Self::RetryAfter(delay) => Some(*delay),
            _ => None,
        }
    }
}
fn digest(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::from_bytes(Sha256::digest(bytes).into())
}
fn successful(status: reqwest::StatusCode) -> Result<(), ExportError> {
    if status.is_success() {
        Ok(())
    } else if status.is_server_error() || matches!(status.as_u16(), 408 | 409 | 429) {
        Err(ExportError::Unavailable)
    } else {
        Err(ExportError::Protocol)
    }
}
async fn response_bytes(response: reqwest::Response, limit: usize) -> Result<Vec<u8>, ExportError> {
    let mut stream = response.bytes_stream();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| ExportError::Unavailable)?;
        if bytes.len().saturating_add(chunk.len()) > limit {
            return Err(ExportError::Size);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
struct Limited {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for Limited {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.bytes.len().saturating_add(bytes.len()) > self.limit {
            return Err(std::io::Error::other("audit export size limit"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn encode(value: &impl Serialize, limit: usize) -> Result<Vec<u8>, ExportError> {
    let mut output = Limited {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut output, value).map_err(|_| ExportError::Size)?;
    Ok(output.bytes)
}
struct Bundle {
    content: Vec<u8>,
    seal: Vec<u8>,
}
impl Bundle {
    fn ocsf(block: &AuditBlock, records: &[AuditRecord]) -> Result<Self, ExportError> {
        let mut writer = Limited {
            bytes: Vec::new(),
            limit: MAX_CONTENT_BYTES,
        };
        for record in records {
            serde_json::to_writer(&mut writer, &ocsf::Event::from_record(record))
                .map_err(|_| ExportError::Size)?;
            writer.write_all(b"\n").map_err(|_| ExportError::Size)?;
        }
        let seal = canonical_bytes(block)?;
        if seal.len() > MAX_SEAL_BYTES {
            return Err(ExportError::Size);
        }
        Ok(Self {
            content: writer.bytes,
            seal,
        })
    }
    fn hashes(&self) -> AuditExportPayload {
        AuditExportPayload {
            content: digest(&self.content),
            seal: digest(&self.seal),
        }
    }
}
enum Destination {
    S3(s3::S3Destination),
    Otlp(otlp::OtlpDestination),
}
impl Destination {
    fn id(&self) -> &AuditDestinationId {
        match self {
            Self::S3(s) => &s.id,
            Self::Otlp(s) => &s.id,
        }
    }
}
pub struct AuditExporter {
    destinations: Vec<Destination>,
}
impl AuditExporter {
    pub fn new(config: AuditExportConfig) -> Result<Self, ExportError> {
        if config.s3.is_none() && config.otlp.is_none() {
            return Ok(Self {
                destinations: Vec::new(),
            });
        }
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| ExportError::Configuration("cannot create audit export HTTP client"))?;
        let mut destinations = Vec::new();
        if let Some(config) = config.s3 {
            destinations.push(Destination::S3(s3::S3Destination::new(
                config,
                http.clone(),
            )?));
        }
        if let Some(config) = config.otlp {
            destinations.push(Destination::Otlp(otlp::OtlpDestination::new(config, http)?));
        }
        Ok(Self { destinations })
    }
    pub fn destination_ids(&self) -> Vec<AuditDestinationId> {
        self.destinations.iter().map(|d| d.id().clone()).collect()
    }
    pub(crate) async fn run(
        &self,
        store: &PlatformStore,
        lease: &AuditSealLease,
    ) -> Result<(), ExportError> {
        if self.destinations.is_empty() {
            return std::future::pending().await;
        }
        // Subscribe before the initial replay. An idle exporter performs no scans.
        let mut wake = store.audit_export_wakes().await?;
        loop {
            let mut pending = false;
            for destination in &self.destinations {
                let Some(block) = store.audit_export_candidate(destination.id()).await? else {
                    continue;
                };
                pending = true;
                match self.deliver(store, lease, destination, &block).await {
                    Ok(()) => {}
                    Err(error) if error.retry_delay().is_some() => {
                        tracing::warn!(
                            destination = destination.id().as_str(),
                            "audit export unresolved; retaining block for retry"
                        );
                        tokio::time::sleep(error.retry_delay().expect("retryable error")).await;
                    }
                    Err(error) => {
                        tracing::error!(destination = destination.id().as_str(), error = %error, "audit export stopped; block retained");
                        return Err(error);
                    }
                }
            }
            if !pending {
                match wake.next().await {
                    Some(Ok(())) => {}
                    Some(Err(error)) => return Err(error.into()),
                    None => return Err(ExportError::Disconnected),
                }
            }
        }
    }
    async fn deliver(
        &self,
        store: &PlatformStore,
        lease: &AuditSealLease,
        destination: &Destination,
        block: &AuditBlock,
    ) -> Result<(), ExportError> {
        let scope = match &block.head.partition {
            AuditPartition::Installation => AuditReadScope::new(None, true),
            AuditPartition::Tenant(tenant) => AuditReadScope::new(Some(tenant.clone()), false),
        };
        let records = store.audit_block_records(&scope, block).await?;
        if record_root(&records)? != block.head.root {
            return Err(ExportError::Conflict);
        }
        let bundle = match destination {
            Destination::S3(_) => Bundle::ocsf(block, &records)?,
            Destination::Otlp(_) => Bundle {
                content: otlp::OtlpDestination::payload(&records)?,
                seal: canonical_bytes(block)?,
            },
        };
        let hashes = bundle.hashes();
        store
            .prepare_audit_export(lease, destination.id(), block, &hashes)
            .await?;
        let result = tokio::time::timeout(Duration::from_secs(60), async {
            match destination {
                Destination::S3(s3) => s3.deliver(block, &bundle).await,
                Destination::Otlp(otlp) => otlp.deliver(&bundle.content).await,
            }
        })
        .await
        .unwrap_or(Err(ExportError::Unavailable));
        if let Err(error) = result {
            let rejection = match &error {
                ExportError::PartialAcceptance => Some(AuditExportRejection::PartialAcceptance),
                ExportError::Protocol => Some(AuditExportRejection::Protocol),
                ExportError::Conflict => Some(AuditExportRejection::ContentConflict),
                ExportError::ObjectLock => Some(AuditExportRejection::ObjectLock),
                ExportError::Size => Some(AuditExportRejection::PayloadTooLarge),
                _ => None,
            };
            if let Some(rejection) = rejection {
                // Keep a known negative provider response while Store reconnects.
                // A network-attempt timeout must never turn it into a resend.
                loop {
                    match store
                        .reject_audit_export(lease, destination.id(), block, &hashes, rejection)
                        .await
                    {
                        Ok(()) | Err(StoreError::AuditExportRejected) => break,
                        Err(StoreError::Database(_)) => {
                            tracing::warn!(
                                destination = destination.id().as_str(),
                                "retrying audit export rejection persistence"
                            );
                            tokio::time::sleep(Duration::from_secs(5)).await;
                        }
                        Err(error) => return Err(error.into()),
                    }
                }
            }
            return Err(error);
        }
        store
            .complete_audit_export(lease, destination.id(), block, &hashes)
            .await?;
        tracing::info!(
            destination = destination.id().as_str(),
            records = records.len(),
            "audit block export acknowledged"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests;
