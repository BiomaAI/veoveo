//! Tenant-scoped restriction reads and operational selection owned by Map.
use anyhow::{Result, ensure};
use chrono::{DateTime, Utc};
use veoveo_platform_store::{MapRestrictionRecord, RecordId};

use super::{MapAccessContext, MapCatalog, decode, integer_version, validate_restriction, wire};
use crate::contract::{
    MapRestrictionPage, MapRestrictionsUri, MobilityFamily, RESTRICTION_PAGE_SIZE, Restriction,
    RestrictionId, RestrictionSummary,
};

// A route must apply every effective restriction. Exceeding this bound fails the
// operation instead of producing a result with an incomplete constraint set.
pub const MAX_EFFECTIVE_RESTRICTIONS: usize = 10_000;

#[derive(Clone, Copy)]
enum Selection<'a> {
    Exact(&'a RestrictionId),
    Page(Option<&'a RestrictionId>),
    Effective {
        at: DateTime<Utc>,
        family: Option<MobilityFamily>,
    },
}

impl MapCatalog {
    /// ```compile_fail
    /// use veoveo_map_mcp::catalog::{MapAccessContext, MapCatalog};
    /// async fn wrong(catalog: MapCatalog, scope: MapAccessContext) {
    ///     catalog.restriction(&scope, "restriction-id").await;
    /// }
    /// ```
    pub async fn restriction(
        &self,
        scope: &MapAccessContext,
        id: &RestrictionId,
    ) -> Result<Option<Restriction>> {
        let mut rows = self
            .select_restrictions(scope, Selection::Exact(id))
            .await?;
        ensure!(rows.len() <= 1, "duplicate Map restriction identity");
        Ok(rows.pop())
    }

    pub async fn restrictions_page(
        &self,
        scope: &MapAccessContext,
        address: &MapRestrictionsUri,
    ) -> Result<MapRestrictionPage> {
        MapRestrictionPage::from_lookahead(
            self.select_restrictions(
                scope,
                Selection::Page(address.cursor().map(|cursor| cursor.after())),
            )
            .await?
            .iter()
            .map(RestrictionSummary::new)
            .collect::<Result<Vec<_>, _>>()?,
        )
        .map_err(Into::into)
    }

    /// Select [valid_from, valid_until), excluding withdrawn restrictions. None
    /// selects every mobility family for a corridor inspection.
    pub async fn effective_restrictions(
        &self,
        scope: &MapAccessContext,
        at: DateTime<Utc>,
        family: Option<MobilityFamily>,
    ) -> Result<Vec<Restriction>> {
        self.select_restrictions(scope, Selection::Effective { at, family })
            .await
    }

    async fn select_restrictions(
        &self,
        scope: &MapAccessContext,
        selection: Selection<'_>,
    ) -> Result<Vec<Restriction>> {
        let (predicate, limit) = match selection {
            Selection::Exact(_) => ("AND restriction_key = $key", 2),
            Selection::Page(None) => ("", RESTRICTION_PAGE_SIZE + 1),
            Selection::Page(Some(_)) => ("AND restriction_key > $after", RESTRICTION_PAGE_SIZE + 1),
            Selection::Effective { .. } => (
                "AND cancelled_by = NONE AND valid_from <= $at
                AND (valid_until = NONE OR valid_until > $at)
                AND ($family = NONE OR affected_mobility_families CONTAINS $family)",
                MAX_EFFECTIVE_RESTRICTIONS + 1,
            ),
        };
        // The statement fragments are closed above. Domain values reach text only
        // when bound to the driver, never while composing a statement.
        let query = self
            .store()
            .client()
            .query(format!(
                "SELECT * FROM map_restriction WHERE tenant = $tenant {predicate}
             ORDER BY restriction_key ASC LIMIT $limit TIMEOUT 5s;"
            ))
            .bind(("tenant", scope.identity.tenant_id.record_id()))
            .bind(("limit", limit));
        let query = match selection {
            Selection::Exact(id) => query.bind(("key", id.to_string())),
            Selection::Page(Some(after)) => query.bind(("after", after.to_string())),
            Selection::Page(None) => query,
            Selection::Effective { at, family } => query
                .bind(("at", at))
                .bind(("family", family.as_ref().map(wire).transpose()?)),
        };
        let mut response = query.await?.check()?;
        let rows: Vec<MapRestrictionRecord> = response.take(0)?;
        if matches!(selection, Selection::Effective { .. }) {
            ensure!(
                rows.len() <= MAX_EFFECTIVE_RESTRICTIONS,
                "operation exceeds the 10000 effective-restriction limit; narrow the tenant restriction catalog before retrying"
            );
        }
        rows.into_iter().map(checked_restriction).collect()
    }

    /// The MCP adapter uses the 101st candidate to report that more matches exist.
    pub async fn complete_restrictions(
        &self,
        scope: &MapAccessContext,
        needle: &str,
    ) -> Result<Vec<RestrictionId>> {
        ensure!(
            needle.len() <= 512 && !needle.chars().any(char::is_control),
            "invalid completion search text"
        );
        let mut response = self
            .store()
            .client()
            .query(
                "SELECT VALUE restriction_key FROM map_restriction WHERE tenant = $tenant
             AND string::lowercase(restriction_key) CONTAINS $needle
             GROUP BY restriction_key ORDER BY restriction_key ASC LIMIT 101 TIMEOUT 5s;",
            )
            .bind(("tenant", scope.identity.tenant_id.record_id()))
            .bind(("needle", needle.to_lowercase()))
            .await?
            .check()?;
        let keys: Vec<String> = response.take(0)?;
        keys.into_iter()
            .map(|key| RestrictionId::parse(key).map_err(Into::into))
            .collect()
    }
}

fn checked_restriction(row: MapRestrictionRecord) -> Result<Restriction> {
    let restriction: Restriction = decode(&row.canonical_json, "restriction")?;
    validate_restriction(&restriction)?;
    let mut families = restriction
        .affected_mobility_families
        .iter()
        .map(wire)
        .collect::<Result<Vec<_>>>()?;
    families.sort();
    ensure!(
        row.id == RecordId::new("map_restriction", restriction.restriction_id.as_str())
            && row.restriction_key == restriction.restriction_id.as_str()
            && row.kind == wire(&restriction.kind)?
            && row.effect_kind == wire(&restriction.effect.kind)?
            && row.affected_mobility_families == families
            && row.valid_from == restriction.valid_from
            && row.valid_until == restriction.valid_until
            && row.cancelled_by == restriction.cancelled_by.as_ref().map(ToString::to_string)
            && row.record_version == integer_version(restriction.record_version)?,
        "stored restriction identity, selection fields or version disagree with its document"
    );
    Ok(restriction)
}

#[cfg(test)]
mod tests;
