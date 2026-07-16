use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;
use veoveo_platform_store::PrincipalKind;

use crate::{
    catalog::{MapCatalog, MapScope},
    contract::RegisteredSource,
};

const BOOTSTRAP_PRINCIPAL: &str = "map-catalog-bootstrap";
const BOOTSTRAP_ISSUER: &str = "urn:veoveo:installation-bootstrap";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BootstrapCatalog {
    tenant_key: String,
    sources: Vec<RegisteredSource>,
}

pub(super) async fn apply(path: &Path, catalog: &MapCatalog) -> Result<()> {
    let bytes = tokio::fs::read(path)
        .await
        .with_context(|| format!("reading Map bootstrap catalog {}", path.display()))?;
    let bootstrap: BootstrapCatalog = serde_json::from_slice(&bytes)
        .with_context(|| format!("decoding Map bootstrap catalog {}", path.display()))?;
    anyhow::ensure!(
        !bootstrap.tenant_key.trim().is_empty(),
        "Map bootstrap tenant_key must not be empty"
    );
    let identity = catalog
        .store()
        .ensure_identity(
            &bootstrap.tenant_key,
            BOOTSTRAP_PRINCIPAL,
            BOOTSTRAP_ISSUER,
            BOOTSTRAP_PRINCIPAL,
            PrincipalKind::Service,
        )
        .await?;
    let scope = MapScope { identity };

    for source in bootstrap.sources {
        if catalog.source(&scope, &source.source_id).await?.is_some() {
            tracing::info!(source = %source.source_id, "Map bootstrap source already registered");
            continue;
        }
        let source = catalog.create_source(&scope, source).await?;
        tracing::info!(source = %source.source_id, dataset = %source.dataset_id, "Map bootstrap source registered");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_catalog_is_typed_and_rejects_unknown_fields() {
        let error = serde_json::from_value::<BootstrapCatalog>(serde_json::json!({
            "tenant_key": "tenant",
            "sources": [],
            "legacy_sources": []
        }))
        .expect_err("unknown bootstrap fields must fail");
        assert!(error.to_string().contains("unknown field"));
    }
}
