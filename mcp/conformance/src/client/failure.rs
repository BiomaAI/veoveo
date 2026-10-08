//! Private observations from maintained SDK failures, never an authorization verdict.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{ffi::OsString, path::Path};
use veoveo_types::Sha256Digest;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum FailureFormat {
    #[serde(rename = "veoveo.ai/smoke-client-failure/v1")]
    V1,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ObservedFailure {
    Http {
        status: u16,
    },
    Mcp {
        code: i64,
        message_digest: Sha256Digest,
    },
}
impl ObservedFailure {
    pub fn mcp(code: i64, message: impl AsRef<str>) -> Self {
        Self::Mcp {
            code,
            message_digest: Sha256Digest::from_bytes(
                Sha256::digest(message.as_ref().as_bytes()).into(),
            ),
        }
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FailureReport {
    pub format: FailureFormat,
    pub endpoint: String,
    pub arguments_digest: Sha256Digest,
    pub bearer_digest: Option<Sha256Digest>,
    pub observed: ObservedFailure,
}
pub fn arguments_digest(args: impl IntoIterator<Item = OsString>) -> Sha256Digest {
    use std::os::unix::ffi::OsStrExt;
    let mut hash = Sha256::new();
    for argument in args {
        let bytes = argument.as_os_str().as_bytes();
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    Sha256Digest::from_bytes(hash.finalize().into())
}
pub fn bearer_digest(bearer: Option<&str>) -> Option<Sha256Digest> {
    bearer.map(|token| Sha256Digest::from_bytes(Sha256::digest(token.as_bytes()).into()))
}
pub fn observe(error: &anyhow::Error) -> Option<ObservedFailure> {
    use rmcp::transport::streamable_http_client::{
        AuthRequiredError, InsufficientScopeError, StreamableHttpError,
    };
    for cause in error.chain() {
        if let Some(rmcp::service::ClientInitializeError::TransportError { error, .. }) =
            cause.downcast_ref::<rmcp::service::ClientInitializeError>()
            && let Some(observed) = transport_observation(error.error.as_ref())
        {
            return Some(observed);
        }
        if let Some(rmcp::service::ClientInitializeError::JsonRpcError(error)) =
            cause.downcast_ref::<rmcp::service::ClientInitializeError>()
        {
            return Some(ObservedFailure::mcp(
                i64::from(error.code.0),
                error.message.as_ref(),
            ));
        }
        if let Some(rmcp::service::ServiceError::TransportSend(error)) =
            cause.downcast_ref::<rmcp::service::ServiceError>()
            && let Some(observed) = transport_observation(error.error.as_ref())
        {
            return Some(observed);
        }

        if cause.downcast_ref::<AuthRequiredError>().is_some() {
            return Some(ObservedFailure::Http { status: 401 });
        }
        if cause.downcast_ref::<InsufficientScopeError>().is_some() {
            return Some(ObservedFailure::Http { status: 403 });
        }
        if let Some(StreamableHttpError::<reqwest::Error>::HttpResponse { status, .. }) =
            cause.downcast_ref::<StreamableHttpError<reqwest::Error>>()
        {
            return Some(ObservedFailure::Http {
                status: status.as_u16(),
            });
        }
        if let Some(error) = cause.downcast_ref::<reqwest::Error>()
            && let Some(status) = error.status()
        {
            return Some(ObservedFailure::Http {
                status: status.as_u16(),
            });
        }
        if let Some(rmcp::service::ServiceError::McpError(error)) =
            cause.downcast_ref::<rmcp::service::ServiceError>()
        {
            return Some(ObservedFailure::mcp(
                i64::from(error.code.0),
                error.message.as_ref(),
            ));
        }
        if let Some(error) = cause.downcast_ref::<rmcp::ErrorData>() {
            return Some(ObservedFailure::mcp(
                i64::from(error.code.0),
                error.message.as_ref(),
            ));
        }
    }
    None
}
fn transport_observation(
    error: &(dyn std::error::Error + Send + Sync + 'static),
) -> Option<ObservedFailure> {
    use rmcp::transport::streamable_http_client::{
        AuthRequiredError, InsufficientScopeError, StreamableHttpError,
    };
    if error.downcast_ref::<AuthRequiredError>().is_some() {
        return Some(ObservedFailure::Http { status: 401 });
    }
    if error.downcast_ref::<InsufficientScopeError>().is_some() {
        return Some(ObservedFailure::Http { status: 403 });
    }
    if let Some(StreamableHttpError::<reqwest::Error>::HttpResponse { status, .. }) =
        error.downcast_ref::<StreamableHttpError<reqwest::Error>>()
    {
        return Some(ObservedFailure::Http {
            status: status.as_u16(),
        });
    }
    if let Some(error) = error.downcast_ref::<reqwest::Error>() {
        return error.status().map(|status| ObservedFailure::Http {
            status: status.as_u16(),
        });
    }
    None
}
/// Parser/transport/timeout failures have no observed-response receipt. Bodies,
/// provider details and bearer values are deliberately excluded.
pub fn record(endpoint: &str, bearer: Option<&str>, error: &anyhow::Error) -> Result<()> {
    let Some(path) = std::env::var_os("VEOVEO_SMOKE_FAILURE_REPORT") else {
        return Ok(());
    };
    let Some(observed) = observe(error) else {
        return Ok(());
    };
    let report = FailureReport {
        format: FailureFormat::V1,
        endpoint: endpoint.to_owned(),
        arguments_digest: arguments_digest(std::env::args_os().skip(1)),
        bearer_digest: bearer_digest(bearer),
        observed,
    };
    use std::{io::Write, os::unix::fs::OpenOptionsExt};
    let bytes = serde_json::to_vec(&report)?;
    ensure!(bytes.len() <= 8192, "client failure receipt exceeds limit");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}
pub fn read(path: &Path) -> Result<FailureReport> {
    let bytes = std::fs::read(path)
        .context("no typed peer failure observed; parser/transport failures cannot prove denial")?;
    ensure!(bytes.len() <= 8192, "client failure receipt exceeds limit");
    Ok(serde_json::from_slice(&bytes)?)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parser_and_unrelated_transport_errors_are_not_peer_denials() {
        assert!(observe(&anyhow::anyhow!("unknown argument --internal-server")).is_none());
        assert!(observe(&anyhow::anyhow!("connection refused")).is_none());
        let failure = anyhow::Error::new(rmcp::service::ServiceError::McpError(
            rmcp::ErrorData::invalid_params("not authority", None),
        ));
        assert_eq!(
            observe(&failure),
            Some(ObservedFailure::mcp(-32602, "not authority"))
        );
    }
    #[test]
    fn maintained_initialization_http_denial_is_observed_without_header_exposure() {
        let transport = rmcp::transport::DynamicTransportError::from_parts(
            "fixture",
            std::any::TypeId::of::<()>(),
            Box::new(
                rmcp::transport::streamable_http_client::AuthRequiredError::new(
                    "Bearer private-challenge".into(),
                ),
            ),
        );
        let error = anyhow::Error::new(rmcp::service::ClientInitializeError::TransportError {
            error: transport,
            context: "discover".into(),
        });
        let observed = observe(&error).unwrap();
        assert_eq!(observed, ObservedFailure::Http { status: 401 });
        assert!(
            !serde_json::to_string(&observed)
                .unwrap()
                .contains("private-challenge")
        );
    }
    #[test]
    fn unrelated_mcp_message_cannot_satisfy_same_code_denial() {
        assert_ne!(
            ObservedFailure::mcp(-32602, "unknown artifact"),
            ObservedFailure::mcp(-32602, "malformed request")
        );
    }
}
