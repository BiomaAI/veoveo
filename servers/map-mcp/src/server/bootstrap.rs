use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;
use veoveo_mcp_contract::{
    SERVER_BOOTSTRAP_ISSUER, ServerBootstrapDocument, ServerSlug, server_bootstrap_principal,
};
use veoveo_platform_store::PrincipalKind;

use crate::{
    catalog::{MapCatalog, MapScope},
    contract::{MobilityProfile, RegisteredSource},
};

/// Map's server-owned bootstrap payload — the `payload` of a
/// [`ServerBootstrapDocument`] targeting the `map` slug. Applied
/// **create-only**: sources and profile versions that already exist are
/// skipped, never reconciled.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct MapBootstrapPayload {
    #[serde(default)]
    sources: Vec<RegisteredSource>,
    #[serde(default)]
    mobility_profiles: Vec<MobilityProfile>,
}

fn map_slug() -> ServerSlug {
    ServerSlug::new(super::SERVER_SLUG).expect("map is a valid server slug")
}

fn decode(bytes: &[u8]) -> Result<(ServerBootstrapDocument, MapBootstrapPayload)> {
    let document = ServerBootstrapDocument::decode_for(&map_slug(), bytes)?;
    let payload: MapBootstrapPayload = document.payload()?;
    for profile in &payload.mobility_profiles {
        profile.validate()?;
    }
    Ok((document, payload))
}

/// The canonical `bootstrap-validate` verb: decodes and validates a document
/// without touching storage, so deployments can be checked before install.
pub(super) async fn run_validate(path: &Path) -> Result<()> {
    let bytes = tokio::fs::read(path)
        .await
        .with_context(|| format!("reading Map bootstrap document {}", path.display()))?;
    let (document, payload) = decode(&bytes)
        .with_context(|| format!("validating Map bootstrap document {}", path.display()))?;
    println!(
        "ok: tenant `{}`, {} source(s), {} mobility profile(s)",
        document.tenant_key,
        payload.sources.len(),
        payload.mobility_profiles.len(),
    );
    Ok(())
}

pub(super) async fn apply(path: &Path, catalog: &MapCatalog) -> Result<()> {
    let bytes = tokio::fs::read(path)
        .await
        .with_context(|| format!("reading Map bootstrap document {}", path.display()))?;
    let (document, payload) = decode(&bytes)
        .with_context(|| format!("decoding Map bootstrap document {}", path.display()))?;
    let principal = server_bootstrap_principal(&map_slug());
    let identity = catalog
        .store()
        .ensure_identity(
            &document.tenant_key,
            &principal,
            SERVER_BOOTSTRAP_ISSUER,
            &principal,
            PrincipalKind::Service,
        )
        .await?;
    let scope = MapScope { identity };

    for source in payload.sources {
        if catalog.source(&scope, &source.source_id).await?.is_some() {
            tracing::info!(source = %source.source_id, "Map bootstrap source already registered");
            continue;
        }
        let source = catalog.create_source(&scope, source).await?;
        tracing::info!(source = %source.source_id, dataset = %source.dataset_id, "Map bootstrap source registered");
    }
    for profile in payload.mobility_profiles {
        let metadata = profile.metadata();
        let profile_id = metadata.profile_id.clone();
        let version = metadata.version;
        if catalog
            .mobility_profile(&scope, &profile_id, version)
            .await?
            .is_some()
        {
            tracing::info!(profile = %profile_id, version, "Map bootstrap mobility profile already registered");
            continue;
        }
        catalog.create_mobility_profile(&scope, profile).await?;
        tracing::info!(profile = %profile_id, version, "Map bootstrap mobility profile registered");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(payload: serde_json::Value) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "server": "map",
            "tenant_key": "tenant",
            "payload": payload,
        }))
        .expect("envelope serializes")
    }

    #[test]
    fn payload_is_typed_and_rejects_unknown_fields() {
        let error = decode(&envelope(serde_json::json!({
            "sources": [],
            "legacy_sources": [],
        })))
        .expect_err("unknown payload fields must fail");
        assert!(error.to_string().contains("legacy_sources"));
    }

    #[test]
    fn empty_payload_sections_default() {
        let (document, payload) =
            decode(&envelope(serde_json::json!({}))).expect("empty payload decodes");
        assert_eq!(document.tenant_key, "tenant");
        assert!(payload.sources.is_empty());
        assert!(payload.mobility_profiles.is_empty());
    }

    #[test]
    fn mistargeted_documents_fail_closed() {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "server": "time",
            "tenant_key": "tenant",
            "payload": {},
        }))
        .expect("envelope serializes");
        let error = decode(&bytes).expect_err("wrong server must fail");
        assert!(error.to_string().contains("targets server `time`"));
    }
}
