//! Source-scoped admission of a federated catalog. Complete catalog callers stay strict.
use super::collect_admitted;
use anyhow::{Result, ensure};
use rmcp::{RoleClient, model::*, service::Peer};
use serde::{Deserialize, Serialize};
use veoveo_gateway_contract::{
    GATEWAY_DISCOVERY_DEGRADATION_META_KEY, GatewayDiscoveryFailure, GatewayDiscoveryFailureCode,
    GatewayDiscoverySurface,
};
use veoveo_types::ServerSlug;

const MAX_METADATA_BYTES: usize = 256 * 1024;
const MAX_FAILURES: usize = 4_096;

/// One budget covers both source catalog surfaces and retains every admitted failure.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SourceCatalogCoverage {
    selected_server: ServerSlug,
    whole_profile_complete: bool,
    unrelated_gateway_failures: Vec<GatewayDiscoveryFailure>,
    #[serde(skip)]
    metadata_bytes: usize,
}

// The producer DTO is shared. These decoding views additionally close unsupported
// metadata fields at this qualification boundary without changing its wire contract.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Degradation {
    failures: Vec<Failure>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Failure {
    server: ServerSlug,
    surface: GatewayDiscoverySurface,
    code: GatewayDiscoveryFailureCode,
}

impl SourceCatalogCoverage {
    pub(crate) fn new(server: &ServerSlug) -> Self {
        Self {
            selected_server: server.clone(),
            whole_profile_complete: true,
            unrelated_gateway_failures: Vec::new(),
            metadata_bytes: 0,
        }
    }

    fn admit(&mut self, meta: Option<&MetaObject>, surface: GatewayDiscoverySurface) -> Result<()> {
        let Some(value) = meta.and_then(|meta| meta.get(GATEWAY_DISCOVERY_DEGRADATION_META_KEY))
        else {
            return Ok(());
        };
        let bytes = serde_json::to_vec(value)?.len();
        ensure!(
            bytes <= MAX_METADATA_BYTES.saturating_sub(self.metadata_bytes),
            "selected source catalog exceeded its degradation metadata byte budget"
        );
        self.metadata_bytes += bytes;
        let degradation: Degradation = serde_json::from_value(value.clone()).map_err(|_| {
            anyhow::anyhow!(
                "selected source catalog has malformed or unsupported degradation metadata"
            )
        })?;
        ensure!(
            degradation.failures.len()
                <= MAX_FAILURES.saturating_sub(self.unrelated_gateway_failures.len()),
            "selected source catalog exceeded its degradation failure budget"
        );
        for failure in &degradation.failures {
            ensure!(
                failure.surface == surface,
                "selected source catalog has inconsistent degradation surface"
            );
            ensure!(
                failure.server != self.selected_server,
                "selected source catalog contains a selected-owner discovery failure"
            );
        }
        self.whole_profile_complete &= degradation.failures.is_empty();
        self.unrelated_gateway_failures
            .extend(
                degradation
                    .failures
                    .into_iter()
                    .map(|failure| GatewayDiscoveryFailure {
                        server: failure.server,
                        surface: failure.surface,
                        code: failure.code,
                    }),
            );
        Ok(())
    }

    pub(crate) fn is_limited(&self) -> bool {
        !self.whole_profile_complete
    }

    pub(crate) async fn templates(
        &mut self,
        client: &Peer<RoleClient>,
    ) -> Result<Vec<ResourceTemplate>> {
        collect_admitted(
            "selected source resources/templates/list",
            |cursor| async move {
                Ok(client
                    .list_resource_templates(Some(
                        PaginatedRequestParams::default().with_cursor(cursor),
                    ))
                    .await?)
            },
            |page: ListResourceTemplatesResult| {
                self.admit(
                    page.meta.as_ref(),
                    GatewayDiscoverySurface::ResourceTemplates,
                )?;
                Ok((page.resource_templates, page.next_cursor))
            },
        )
        .await
    }

    pub(crate) async fn tools(&mut self, client: &Peer<RoleClient>) -> Result<Vec<Tool>> {
        collect_admitted(
            "selected source tools/list",
            |cursor| async move {
                Ok(client
                    .list_tools(Some(PaginatedRequestParams::default().with_cursor(cursor)))
                    .await?)
            },
            |page: ListToolsResult| {
                self.admit(page.meta.as_ref(), GatewayDiscoverySurface::Tools)?;
                Ok((page.tools, page.next_cursor))
            },
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn metadata(value: serde_json::Value) -> MetaObject {
        let mut meta = MetaObject::new();
        meta.insert(GATEWAY_DISCOVERY_DEGRADATION_META_KEY.into(), value);
        meta
    }
    #[test]
    fn selected_source_metadata_is_closed_and_aggregate_bounded() {
        let server = ServerSlug::parse("selected").unwrap();
        let surface = GatewayDiscoverySurface::Tools;
        for value in [
            json!({"failures":[],"unsupported":true}),
            json!({"failures":[{"server":"other","surface":"tools","code":"upstream_unavailable","extra":true}]}),
            json!({"failures":[{"server":"other","surface":"unknown","code":"upstream_unavailable"}]}),
            json!({"failures":null}),
        ] {
            assert!(
                SourceCatalogCoverage::new(&server)
                    .admit(Some(&metadata(value)), surface)
                    .is_err()
            );
        }
        let row =
            json!({"failures":[{"server":"other","surface":"tools","code":"discovery_pending"}]});
        let mut coverage = SourceCatalogCoverage::new(&server);
        coverage
            .admit(Some(&metadata(row.clone())), surface)
            .unwrap();
        assert!(coverage.is_limited());
        assert_eq!(coverage.unrelated_gateway_failures.len(), 1);
        let mut bytes = SourceCatalogCoverage::new(&server);
        let empty = metadata(json!({"failures":[]}));
        let empty_bytes =
            serde_json::to_vec(empty.get(GATEWAY_DISCOVERY_DEGRADATION_META_KEY).unwrap())
                .unwrap()
                .len();
        for _ in 0..MAX_METADATA_BYTES / empty_bytes {
            bytes.admit(Some(&empty), surface).unwrap();
        }
        assert!(bytes.admit(Some(&empty), surface).is_err());
        let row = json!({"server":"a","surface":"tools","code":"discovery_pending"});
        let mut rows = SourceCatalogCoverage::new(&server);
        rows.admit(
            Some(&metadata(
                json!({"failures":vec![row.clone(); MAX_FAILURES]}),
            )),
            surface,
        )
        .unwrap();
        assert_eq!(rows.unrelated_gateway_failures.len(), MAX_FAILURES);
        assert!(
            rows.admit(Some(&metadata(json!({"failures":[row]}))), surface)
                .is_err()
        );
    }
}
