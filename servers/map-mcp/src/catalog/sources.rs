//! Map owns tenant-scoped source selection; creator identity is attribution.
use crate::persistence::MapSourceRecord;
use anyhow::{Result, ensure};
use veoveo_platform_store::RecordId;

use super::{MapAccessContext, MapCatalog, decode, integer_version, wire};
use crate::contract::{
    MapSourceId, MapSourcePage, MapSourcesUri, RegisteredSource, SOURCE_PAGE_SIZE, SourceSummary,
};

#[derive(Clone, Copy)]
enum Selection<'a> {
    Exact(&'a MapSourceId),
    Page(Option<&'a MapSourceId>),
}

impl MapCatalog {
    /// ```compile_fail
    /// use veoveo_map_mcp::{catalog::{MapAccessContext, MapCatalog}, contract::MapDatasetId};
    /// async fn wrong(catalog: MapCatalog, scope: MapAccessContext) {
    ///     catalog.source(&scope, &MapDatasetId::new()).await;
    /// }
    /// ```
    pub async fn source(
        &self,
        scope: &MapAccessContext,
        id: &MapSourceId,
    ) -> Result<Option<RegisteredSource>> {
        let mut rows = self.select_sources(scope, Selection::Exact(id)).await?;
        ensure!(rows.len() <= 1, "duplicate Map source identity");
        Ok(rows.pop())
    }

    pub async fn sources_page(
        &self,
        scope: &MapAccessContext,
        address: &MapSourcesUri,
    ) -> Result<MapSourcePage> {
        MapSourcePage::from_lookahead(
            self.select_sources(
                scope,
                Selection::Page(address.cursor().map(|cursor| cursor.after())),
            )
            .await?
            .iter()
            .map(SourceSummary::new)
            .collect::<Result<Vec<_>, _>>()?,
        )
        .map_err(Into::into)
    }

    async fn select_sources(
        &self,
        scope: &MapAccessContext,
        selection: Selection<'_>,
    ) -> Result<Vec<RegisteredSource>> {
        let (sql, limit) = match selection {
            Selection::Exact(_) => (include_str!("../queries/catalog/sources/exact.surql"), 2),
            Selection::Page(None) => (
                include_str!("../queries/catalog/sources/first_page.surql"),
                SOURCE_PAGE_SIZE + 1,
            ),
            Selection::Page(Some(_)) => (
                include_str!("../queries/catalog/sources/after_page.surql"),
                SOURCE_PAGE_SIZE + 1,
            ),
        };
        let query = self
            .store()
            .client()
            .query(sql)
            .bind(("tenant", scope.identity.tenant_id.record_id()))
            .bind(("limit", limit));
        let query = match selection {
            Selection::Exact(id) => query.bind(("key", id.to_string())),
            Selection::Page(Some(id)) => query.bind(("after", id.to_string())),
            Selection::Page(None) => query,
        };
        let mut response = query.await?.check()?;
        let rows: Vec<MapSourceRecord> = response.take(0)?;
        rows.into_iter().map(checked_source).collect()
    }

    /// The MCP adapter uses one lookahead candidate to report remaining matches.
    pub async fn complete_sources(
        &self,
        scope: &MapAccessContext,
        needle: &str,
    ) -> Result<Vec<MapSourceId>> {
        ensure!(
            needle.len() <= 512 && !needle.chars().any(char::is_control),
            "invalid completion search text"
        );
        let mut response = self
            .store()
            .client()
            .query(include_str!(
                "../queries/catalog/sources/complete_sources/statement_1.surql"
            ))
            .bind(("tenant", scope.identity.tenant_id.record_id()))
            .bind(("needle", needle.to_lowercase()))
            .await?
            .check()?;
        let keys: Vec<String> = response.take(0)?;
        keys.into_iter()
            .map(|key| MapSourceId::parse(key).map_err(Into::into))
            .collect()
    }
}

fn checked_source(row: MapSourceRecord) -> Result<RegisteredSource> {
    let source: RegisteredSource = decode(&row.canonical_json, "map source")?;
    source.validate()?;
    let mut families = source
        .map_families
        .iter()
        .map(wire)
        .collect::<Result<Vec<_>>>()?;
    families.sort();
    ensure!(
        row.id == RecordId::new("map_source", source.source_id.as_str())
            && row.source_key == source.source_id.as_str()
            && row.dataset_key == source.dataset_id.as_str()
            && row.name == source.name
            && row.adapter_kind == wire(&source.adapter_kind)?
            && row.authority_class == wire(&source.authority)?
            && row.map_families == families
            && row.enabled == source.enabled
            && row.record_version == integer_version(source.record_version)?,
        "stored source identity, selection fields or version disagree with its document"
    );
    Ok(source)
}

#[cfg(test)]
mod tests;
